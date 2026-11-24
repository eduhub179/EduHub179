//! Online party mode for the «Порядок величин» game (frontend/public/games/logscale/).
//!
//! Players on different devices join a room by a short code and take turns on the same
//! question: the first player picks any interval on the log scale, every next player must
//! pick an interval nested in the previous pick. The server keeps the room, checks turns
//! and nesting, and does the scoring. Clients poll `GET /api/party/rooms/:code` about once
//! a second, so nothing special (WebSocket upgrade) is needed in the reverse proxy.
//!
//! Rooms live in memory only: they are small, short-lived and do not need the database.
//! A restart of the API ends running games. Idle rooms are dropped after `ROOM_TTL`.
//! The question bank stays in the game page; the host's page sends the drawn questions
//! (bank index, scale start, true answer) when the game starts.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use axum::{
    extract::Path,
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::router::ApiError;

pub(crate) const MAX_PLAYERS: usize = 8;
const MAX_ROOMS: usize = 500;
const MAX_ROUNDS: usize = 20;
const MAX_NAME: usize = 20;
const ROOM_TTL: Duration = Duration::from_secs(3 * 60 * 60);
const ONLINE_FOR: Duration = Duration::from_secs(12);
const MISS: i64 = -30;
const CODE_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ";
const CODE_LEN: usize = 4;
const TOKEN_HEADER: &str = "x-party-token";

static ROOMS: LazyLock<Mutex<HashMap<String, Room>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Handlers keep their own state (`ROOMS`), so the routes fit any router state.
pub fn party_routes<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route("/api/party/rooms", post(create_room_handler))
        .route("/api/party/rooms/:code", get(room_state_handler))
        .route("/api/party/rooms/:code/join", post(join_handler))
        .route("/api/party/rooms/:code/start", post(start_handler))
        .route("/api/party/rooms/:code/pick", post(pick_handler))
        .route("/api/party/rooms/:code/skip", post(skip_handler))
        .route("/api/party/rooms/:code/next", post(next_handler))
        .route("/api/party/rooms/:code/lobby", post(lobby_handler))
        .route("/api/party/rooms/:code/leave", post(leave_handler))
}

// ---------- model ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    Lobby,
    Question,
    Reveal,
    Final,
}

impl Phase {
    fn as_str(self) -> &'static str {
        match self {
            Phase::Lobby => "lobby",
            Phase::Question => "question",
            Phase::Reveal => "reveal",
            Phase::Final => "final",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Player {
    pub id: u32,
    pub name: String,
    token: String,
    pub score: i64,
    pub hits: u32,
    last_seen: Instant,
    pub left: bool,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub(crate) struct DeckItem {
    /// Index in the game page's question bank.
    pub i: u32,
    /// Where the default selection starts on the ruler, 0..1.
    pub start: f64,
    /// The true answer.
    pub x: f64,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Pick {
    pub player: u32,
    pub a: f64,
    pub b: f64,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Outcome {
    pub player: u32,
    pub a: f64,
    pub b: f64,
    pub hit: bool,
    pub pts: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct Answered {
    pub i: u32,
    pub x: f64,
    pub results: Vec<Outcome>,
}

#[derive(Debug, Clone)]
pub(crate) struct Room {
    pub code: String,
    pub version: u64,
    pub phase: Phase,
    pub host: u32,
    pub players: Vec<Player>,
    next_id: u32,
    pub rounds: usize,
    pub bank: u32,
    pub deck: Vec<DeckItem>,
    pub idx: usize,
    /// Turn order for the current question (player ids).
    pub order: Vec<u32>,
    pub turn_pos: usize,
    pub picks: Vec<Pick>,
    pub history: Vec<Answered>,
    touched: Instant,
}

/// Points for a hit: 100, halved for every factor of 10 in width, at least 1.
pub(crate) fn points(a: f64, b: f64) -> i64 {
    let p = (100.0 * 2f64.powf(-(b / a).log10())).round() as i64;
    p.max(1)
}

fn err(status: StatusCode, message: &str) -> ApiError {
    ApiError {
        status,
        message: message.to_string(),
    }
}

fn clean_name(raw: &str) -> Result<String, ApiError> {
    let name: String = raw
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if name.is_empty() {
        return Err(err(StatusCode::BAD_REQUEST, "Введите имя"));
    }
    Ok(name.chars().take(MAX_NAME).collect())
}

fn random_code() -> String {
    let bytes = Uuid::new_v4().into_bytes();
    bytes[..CODE_LEN]
        .iter()
        .map(|b| CODE_ALPHABET[*b as usize % CODE_ALPHABET.len()] as char)
        .collect()
}

impl Room {
    pub(crate) fn new(code: String, host_name: String, rounds: usize, bank: u32) -> (Self, String) {
        let now = Instant::now();
        let token = Uuid::new_v4().to_string();
        let room = Room {
            code,
            version: 1,
            phase: Phase::Lobby,
            host: 1,
            players: vec![Player {
                id: 1,
                name: host_name,
                token: token.clone(),
                score: 0,
                hits: 0,
                last_seen: now,
                left: false,
            }],
            next_id: 2,
            rounds,
            bank,
            deck: Vec::new(),
            idx: 0,
            order: Vec::new(),
            turn_pos: 0,
            picks: Vec::new(),
            history: Vec::new(),
            touched: now,
        };
        (room, token)
    }

    fn bump(&mut self) {
        self.version += 1;
        self.touched = Instant::now();
    }

    fn active(&self) -> impl Iterator<Item = &Player> {
        self.players.iter().filter(|p| !p.left)
    }

    fn player_by_token(&mut self, token: Option<&str>) -> Option<&mut Player> {
        let token = token?;
        self.players
            .iter_mut()
            .find(|p| !p.left && p.token == token)
    }

    fn caller(&mut self, token: Option<&str>) -> Result<u32, ApiError> {
        let player = self
            .player_by_token(token)
            .ok_or_else(|| err(StatusCode::FORBIDDEN, "Вы не в этой комнате"))?;
        player.last_seen = Instant::now();
        Ok(player.id)
    }

    fn require_host(&mut self, token: Option<&str>) -> Result<u32, ApiError> {
        let id = self.caller(token)?;
        if id != self.host {
            return Err(err(
                StatusCode::FORBIDDEN,
                "Это может сделать только ведущий",
            ));
        }
        Ok(id)
    }

    pub(crate) fn join(&mut self, name: String) -> Result<(u32, String), ApiError> {
        if self.phase != Phase::Lobby {
            return Err(err(StatusCode::CONFLICT, "Игра уже началась"));
        }
        if self.active().count() >= MAX_PLAYERS {
            return Err(err(StatusCode::CONFLICT, "В комнате уже 8 игроков"));
        }
        let mut name = name;
        if self
            .active()
            .any(|p| p.name.to_lowercase() == name.to_lowercase())
        {
            let base: String = name.chars().take(MAX_NAME - 2).collect();
            let n = (2..)
                .find(|n| {
                    let candidate = format!("{base} {n}").to_lowercase();
                    !self.active().any(|p| p.name.to_lowercase() == candidate)
                })
                .unwrap_or(2);
            name = format!("{base} {n}");
        }
        let id = self.next_id;
        self.next_id += 1;
        let token = Uuid::new_v4().to_string();
        self.players.push(Player {
            id,
            name,
            token: token.clone(),
            score: 0,
            hits: 0,
            last_seen: Instant::now(),
            left: false,
        });
        self.bump();
        Ok((id, token))
    }

    pub(crate) fn start(&mut self, deck: Vec<DeckItem>, bank: u32) -> Result<(), ApiError> {
        if !matches!(self.phase, Phase::Lobby | Phase::Final) {
            return Err(err(StatusCode::CONFLICT, "Игра уже идёт"));
        }
        if self.active().count() < 2 {
            return Err(err(StatusCode::CONFLICT, "Нужно хотя бы два игрока"));
        }
        if deck.is_empty() || deck.len() > MAX_ROUNDS {
            return Err(err(StatusCode::BAD_REQUEST, "Неверное число вопросов"));
        }
        let valid = deck.iter().all(|d| {
            d.i < bank
                && d.x.is_finite()
                && d.x > 0.0
                && d.start.is_finite()
                && (0.0..=1.0).contains(&d.start)
        });
        if !valid {
            return Err(err(StatusCode::BAD_REQUEST, "Неверные вопросы"));
        }
        for p in &mut self.players {
            p.score = 0;
            p.hits = 0;
        }
        self.players.retain(|p| !p.left);
        self.bank = bank;
        self.rounds = deck.len();
        self.deck = deck;
        self.idx = 0;
        self.history.clear();
        self.begin_question();
        self.bump();
        Ok(())
    }

    fn begin_question(&mut self) {
        let ids: Vec<u32> = self.active().map(|p| p.id).collect();
        let n = ids.len().max(1);
        let s = self.idx % n;
        self.order = (0..ids.len()).map(|k| ids[(s + k) % n]).collect();
        self.turn_pos = 0;
        self.picks.clear();
        self.phase = Phase::Question;
        self.skip_departed();
    }

    pub(crate) fn current_turn(&self) -> Option<u32> {
        if self.phase != Phase::Question {
            return None;
        }
        self.order.get(self.turn_pos).copied()
    }

    /// The interval the next pick must stay inside: the last pick of this question.
    pub(crate) fn limit(&self) -> Option<(f64, f64)> {
        self.picks.last().map(|p| (p.a, p.b))
    }

    fn is_left(&self, id: u32) -> bool {
        !self.players.iter().any(|p| p.id == id && !p.left)
    }

    /// Moves past players who left; reveals the answer when everyone has had a turn.
    fn skip_departed(&mut self) {
        while self.turn_pos < self.order.len() && self.is_left(self.order[self.turn_pos]) {
            self.turn_pos += 1;
        }
        if self.phase == Phase::Question && self.turn_pos >= self.order.len() {
            self.reveal();
        }
    }

    pub(crate) fn pick(&mut self, player: u32, a: f64, b: f64) -> Result<(), ApiError> {
        if self.current_turn() != Some(player) {
            return Err(err(StatusCode::CONFLICT, "Сейчас не ваш ход"));
        }
        if !(a.is_finite() && b.is_finite() && a > 0.0 && b > 0.0) {
            return Err(err(
                StatusCode::BAD_REQUEST,
                "Границы должны быть положительными числами",
            ));
        }
        let (mut a, mut b) = if a <= b { (a, b) } else { (b, a) };
        if let Some((la, lb)) = self.limit() {
            // Small tolerance: the page rounds values to 3 significant digits.
            const EPS: f64 = 1e-9;
            if a < la * (1.0 - EPS) || b > lb * (1.0 + EPS) {
                return Err(err(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "Отрезок должен быть внутри отрезка предыдущего игрока",
                ));
            }
            a = a.max(la);
            b = b.min(lb);
        }
        self.picks.push(Pick { player, a, b });
        self.turn_pos += 1;
        self.skip_departed();
        self.bump();
        Ok(())
    }

    pub(crate) fn skip_turn(&mut self) -> Result<(), ApiError> {
        if self.current_turn().is_none() {
            return Err(err(StatusCode::CONFLICT, "Сейчас некого пропускать"));
        }
        self.turn_pos += 1;
        self.skip_departed();
        self.bump();
        Ok(())
    }

    fn reveal(&mut self) {
        let Some(q) = self.deck.get(self.idx).copied() else {
            return;
        };
        let x = q.x;
        let results: Vec<Outcome> = self
            .picks
            .iter()
            .map(|p| {
                let hit = x >= p.a && x <= p.b;
                let pts = if hit { points(p.a, p.b) } else { MISS };
                Outcome {
                    player: p.player,
                    a: p.a,
                    b: p.b,
                    hit,
                    pts,
                }
            })
            .collect();
        for r in &results {
            if let Some(pl) = self.players.iter_mut().find(|p| p.id == r.player) {
                pl.score += r.pts;
                if r.hit {
                    pl.hits += 1;
                }
            }
        }
        self.history.push(Answered { i: q.i, x, results });
        self.phase = Phase::Reveal;
    }

    pub(crate) fn next(&mut self) -> Result<(), ApiError> {
        if self.phase != Phase::Reveal {
            return Err(err(StatusCode::CONFLICT, "Ответ ещё не открыт"));
        }
        self.idx += 1;
        if self.idx < self.deck.len() {
            self.begin_question();
        } else {
            self.phase = Phase::Final;
        }
        self.bump();
        Ok(())
    }

    pub(crate) fn back_to_lobby(&mut self) -> Result<(), ApiError> {
        if self.phase != Phase::Final {
            return Err(err(StatusCode::CONFLICT, "Игра ещё идёт"));
        }
        self.players.retain(|p| !p.left);
        self.phase = Phase::Lobby;
        self.bump();
        Ok(())
    }

    pub(crate) fn leave(&mut self, player: u32) {
        if let Some(p) = self.players.iter_mut().find(|p| p.id == player) {
            p.left = true;
        }
        if self.phase == Phase::Lobby {
            self.players.retain(|p| !p.left);
        }
        if self.host == player {
            let next = self.active().map(|p| p.id).next();
            if let Some(next) = next {
                self.host = next;
            }
        }
        if self.phase == Phase::Question {
            self.skip_departed();
        }
        self.bump();
    }

    pub(crate) fn snapshot(&self, me: Option<u32>) -> Value {
        let now = Instant::now();
        let players: Vec<Value> = self
            .players
            .iter()
            .map(|p| {
                json!({
                    "id": p.id,
                    "name": p.name,
                    "score": p.score,
                    "hits": p.hits,
                    "online": !p.left && now.duration_since(p.last_seen) < ONLINE_FOR,
                    "left": p.left,
                })
            })
            .collect();
        let outcome = |r: &Outcome| json!({"player": r.player, "a": r.a, "b": r.b, "hit": r.hit, "pts": r.pts});
        let question = match self.phase {
            Phase::Question | Phase::Reveal => self.deck.get(self.idx).map(|q| {
                let mut v = json!({"i": q.i, "start": q.start});
                if self.phase == Phase::Reveal {
                    v["x"] = json!(q.x);
                }
                v
            }),
            _ => None,
        };
        let reveal = if self.phase == Phase::Reveal {
            self.history
                .last()
                .map(|h| h.results.iter().map(outcome).collect::<Vec<_>>())
        } else {
            None
        };
        let history: Vec<Value> = if self.phase == Phase::Final {
            self.history
                .iter()
                .map(|h| json!({"i": h.i, "x": h.x, "results": h.results.iter().map(outcome).collect::<Vec<_>>()}))
                .collect()
        } else {
            Vec::new()
        };
        json!({
            "code": self.code,
            "version": self.version,
            "phase": self.phase.as_str(),
            "host": self.host,
            "me": me,
            "players": players,
            "rounds": self.rounds,
            "bank": self.bank,
            "idx": self.idx,
            "question": question,
            "order": self.order,
            "turn": self.current_turn(),
            "picks": self.picks.iter().map(|p| json!({"player": p.player, "a": p.a, "b": p.b})).collect::<Vec<_>>(),
            "reveal": reveal,
            "history": history,
        })
    }
}

// ---------- handlers ----------

fn token(headers: &HeaderMap) -> Option<&str> {
    headers.get(TOKEN_HEADER).and_then(|v| v.to_str().ok())
}

fn normalize_code(code: &str) -> String {
    code.trim().to_uppercase()
}

fn with_room<T>(
    code: &str,
    f: impl FnOnce(&mut Room) -> Result<T, ApiError>,
) -> Result<T, ApiError> {
    let mut rooms = ROOMS.lock().unwrap_or_else(|e| e.into_inner());
    let room = rooms
        .get_mut(&normalize_code(code))
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "Комната не найдена. Проверьте код"))?;
    f(room)
}

fn drop_idle(rooms: &mut HashMap<String, Room>) {
    let now = Instant::now();
    rooms.retain(|_, r| now.duration_since(r.touched) < ROOM_TTL && r.active().next().is_some());
}

#[derive(Deserialize)]
pub(crate) struct CreateBody {
    name: String,
    #[serde(default = "default_rounds")]
    rounds: usize,
    bank: u32,
}

fn default_rounds() -> usize {
    5
}

async fn create_room_handler(Json(body): Json<CreateBody>) -> Result<Json<Value>, ApiError> {
    let name = clean_name(&body.name)?;
    let rounds = body.rounds.clamp(1, MAX_ROUNDS);
    let mut rooms = ROOMS.lock().unwrap_or_else(|e| e.into_inner());
    drop_idle(&mut rooms);
    if rooms.len() >= MAX_ROOMS {
        return Err(err(
            StatusCode::SERVICE_UNAVAILABLE,
            "Слишком много комнат, попробуйте позже",
        ));
    }
    let code = (0..50)
        .map(|_| random_code())
        .find(|c| !rooms.contains_key(c))
        .ok_or_else(|| {
            err(
                StatusCode::SERVICE_UNAVAILABLE,
                "Не удалось создать комнату",
            )
        })?;
    let (room, token) = Room::new(code.clone(), name, rounds, body.bank);
    let state = room.snapshot(Some(room.host));
    rooms.insert(code.clone(), room);
    Ok(Json(
        json!({"code": code, "token": token, "player": 1, "state": state}),
    ))
}

#[derive(Deserialize)]
pub(crate) struct JoinBody {
    name: String,
}

async fn join_handler(
    Path(code): Path<String>,
    Json(body): Json<JoinBody>,
) -> Result<Json<Value>, ApiError> {
    let name = clean_name(&body.name)?;
    with_room(&code, |room| {
        let (id, token) = room.join(name)?;
        Ok(Json(
            json!({"code": room.code, "token": token, "player": id, "state": room.snapshot(Some(id))}),
        ))
    })
}

async fn room_state_handler(
    Path(code): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    with_room(&code, |room| {
        let me = room.player_by_token(token(&headers)).map(|p| {
            p.last_seen = Instant::now();
            p.id
        });
        Ok(Json(room.snapshot(me)))
    })
}

#[derive(Deserialize)]
pub(crate) struct StartBody {
    deck: Vec<DeckItem>,
    bank: u32,
}

async fn start_handler(
    Path(code): Path<String>,
    headers: HeaderMap,
    Json(body): Json<StartBody>,
) -> Result<Json<Value>, ApiError> {
    with_room(&code, |room| {
        let me = room.require_host(token(&headers))?;
        room.start(body.deck, body.bank)?;
        Ok(Json(room.snapshot(Some(me))))
    })
}

#[derive(Deserialize)]
pub(crate) struct PickBody {
    a: f64,
    b: f64,
}

async fn pick_handler(
    Path(code): Path<String>,
    headers: HeaderMap,
    Json(body): Json<PickBody>,
) -> Result<Json<Value>, ApiError> {
    with_room(&code, |room| {
        let me = room.caller(token(&headers))?;
        room.pick(me, body.a, body.b)?;
        Ok(Json(room.snapshot(Some(me))))
    })
}

async fn skip_handler(
    Path(code): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    with_room(&code, |room| {
        let me = room.require_host(token(&headers))?;
        room.skip_turn()?;
        Ok(Json(room.snapshot(Some(me))))
    })
}

async fn next_handler(
    Path(code): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    with_room(&code, |room| {
        let me = room.require_host(token(&headers))?;
        room.next()?;
        Ok(Json(room.snapshot(Some(me))))
    })
}

async fn lobby_handler(
    Path(code): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    with_room(&code, |room| {
        let me = room.require_host(token(&headers))?;
        room.back_to_lobby()?;
        Ok(Json(room.snapshot(Some(me))))
    })
}

async fn leave_handler(
    Path(code): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    with_room(&code, |room| {
        let me = room.caller(token(&headers))?;
        room.leave(me);
        Ok(Json(json!({"ok": true})))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deck(xs: &[f64]) -> Vec<DeckItem> {
        xs.iter()
            .enumerate()
            .map(|(i, x)| DeckItem {
                i: i as u32,
                start: 0.5,
                x: *x,
            })
            .collect()
    }

    fn room_with(n: usize) -> Room {
        let (mut room, _) = Room::new("ABCD".into(), "Аня".into(), 3, 93);
        for k in 1..n {
            room.join(format!("Игрок {k}")).unwrap();
        }
        room
    }

    #[test]
    fn points_halve_per_order_of_magnitude() {
        assert_eq!(points(1.0, 1.0), 100);
        assert_eq!(points(1.0, 10.0), 50);
        assert_eq!(points(1.0, 100.0), 25);
        assert_eq!(points(1.0, 1e30), 1);
    }

    #[test]
    fn players_take_turns_with_nested_intervals_and_get_scored() {
        let mut room = room_with(3);
        room.start(deck(&[50.0, 5.0]), 93).unwrap();
        assert_eq!(room.order, vec![1, 2, 3]);
        assert_eq!(room.current_turn(), Some(1));
        assert!(room.pick(2, 1.0, 10.0).is_err(), "not player 2's turn");
        room.pick(1, 1.0, 1000.0).unwrap();
        assert!(room.pick(2, 0.5, 10.0).is_err(), "must be nested");
        room.pick(2, 10.0, 100.0).unwrap();
        room.pick(3, 20.0, 40.0).unwrap();
        assert_eq!(room.phase, Phase::Reveal);
        let scores: Vec<i64> = room.players.iter().map(|p| p.score).collect();
        assert_eq!(scores, vec![points(1.0, 1000.0), 50, MISS]);
        room.next().unwrap();
        // Second question starts with the next player in the circle.
        assert_eq!(room.order, vec![2, 3, 1]);
        assert_eq!(room.current_turn(), Some(2));
    }

    #[test]
    fn same_interval_as_previous_is_allowed() {
        let mut room = room_with(2);
        room.start(deck(&[5.0]), 93).unwrap();
        room.pick(1, 1.0, 10.0).unwrap();
        room.pick(2, 1.0, 10.0).unwrap();
        assert_eq!(room.phase, Phase::Reveal);
    }

    #[test]
    fn leaving_or_skipping_moves_the_turn_on() {
        let mut room = room_with(3);
        room.start(deck(&[5.0, 5.0]), 93).unwrap();
        room.pick(1, 1.0, 10.0).unwrap();
        room.leave(2);
        assert_eq!(room.current_turn(), Some(3));
        room.skip_turn().unwrap();
        assert_eq!(room.phase, Phase::Reveal);
        assert_eq!(room.history[0].results.len(), 1);
        room.leave(1);
        assert_eq!(room.host, 3, "host passes to the next player");
    }

    #[test]
    fn join_only_in_lobby_and_up_to_eight() {
        let mut room = room_with(8);
        assert!(room.join("Ещё".into()).is_err());
        let mut room = room_with(2);
        room.start(deck(&[5.0]), 93).unwrap();
        assert!(room.join("Поздний".into()).is_err());
    }

    #[test]
    fn duplicate_names_get_a_number() {
        let mut room = room_with(1);
        room.join("аня".into()).unwrap();
        assert_eq!(room.players[1].name, "аня 2");
    }

    #[test]
    fn start_needs_two_players_and_a_valid_deck() {
        let mut room = room_with(1);
        assert!(room.start(deck(&[5.0]), 93).is_err());
        let mut room = room_with(2);
        assert!(room
            .start(
                vec![DeckItem {
                    i: 200,
                    start: 0.5,
                    x: 1.0
                }],
                93
            )
            .is_err());
        assert!(room.start(deck(&[-1.0]), 93).is_err());
    }

    #[test]
    fn answer_is_hidden_until_reveal() {
        let mut room = room_with(2);
        room.start(deck(&[5.0]), 93).unwrap();
        let s = room.snapshot(Some(1));
        assert!(s["question"].get("x").is_none());
        room.pick(1, 1.0, 10.0).unwrap();
        room.pick(2, 1.0, 10.0).unwrap();
        assert_eq!(room.snapshot(Some(1))["question"]["x"], json!(5.0));
    }
}
