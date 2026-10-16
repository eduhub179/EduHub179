# Local model comparison (overnight run, 2026-10-16)

Primary metric: test_holdout2.tsv (50 messages); tie-break: speed. RAM = `ollama ps` size of the loaded model (GB).
All 5 candidates pulled OK, none skipped. qwen3 / T-lite-2.1 run with thinking off (@nothink).

| Model | Disk | RAM (GB) | holdout2 | holdout | s/request (h2) | Format errors | Notes |
|---|---|---|---|---|---|---|---|
| **qwen2.5:7b** (current) | 4.7 GB | 5.1 | **49/50** | 48/50 | **5.0** | 0 | fastest; only #45 missed |
| t-tech/T-lite-it-2.1:q4_K_M @nothink | 5.0 GB | 6.5 | 49/50 | — | 7.4 | 0 | same single miss (#45), 1.5× slower |
| qwen3:8b @nothink | 5.2 GB | 6.7 | 49/50 | — | 8.4 | 0 | same single miss (#45), slowest |
| T-lite-it-1.0 GGUF Q4_K_M | 4.7 GB | 5.5 | 48/50 | 47/50 | 7.8 | 0 | false positives on chatter (homework/test) |
| qwen3:4b @nothink | 2.5 GB | 3.9 | 47/50 | — | 6.4 | 0 | missed time/room in #18, false homework #26 |

(holdout was run only for qwen2.5:7b and T-lite-1.0; s/request on holdout: 8.4 and 8.6.)

## Errors
- Shared by qwen2.5:7b, T-lite-2.1, qwen3:8b, qwen3:4b — #45 "Сделала таблицу с вашими баллами, проверьте" → classified as chatter, expected i|m. Looks borderline/label-ambiguous, not a model weakness.
- T-lite-1.0: treats opinion/requests about homework/tests as events (#6 "Скиньте дз по алгебре", #30 "Вчерашняя контрольная была ужасной", holdout #32 "Я думаю, тест будет лёгкий").
- qwen3:4b: #18 typed as lesson_change and lost time 15:30 / room 205; #26 speculation → homework.
- qwen2.5:7b on holdout: #14 (expected m), #29 (expected chatter) — T-lite-1.0 made the same two plus one more.

## Recommendation
Keep **qwen2.5:7b**. No candidate beats it on holdout2 (best is a tie at 49/50) and it is 1.5–1.7× faster with the smallest RAM among 7–8B models. T-lite-2.1 is the only reasonable alternative (same score, Russian-tuned) if speed stops mattering. Not tested: RuadaptQwen2.5-7B-Lite (skipped to save budget), Vikhr (skipped per instructions).
