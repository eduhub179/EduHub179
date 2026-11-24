import React from 'react';
import { useSearchParams } from 'react-router-dom';
import Header from '../components/Header.tsx';
import { useAuth } from '../auth/AuthProvider';
import '../css/GamesPage.css';

// The game is a standalone page in public/games/logscale/ (no build step, works offline for
// solo modes). Online party mode talks to /api/party on the same origin.
const GAME_URL = '/games/logscale/index.html';

const GamesPage: React.FC = () => {
    const { user } = useAuth();
    const [searchParams] = useSearchParams();

    const params = new URLSearchParams();
    const room = searchParams.get('room');
    if (room) params.set('room', room);
    const firstName = user?.name?.split(' ')[0];
    if (firstName) params.set('name', firstName);
    const query = params.toString();

    return (
        <div className="games-page">
            <Header />
            <iframe
                className="games-page__frame"
                src={query ? `${GAME_URL}?${query}` : GAME_URL}
                title="Порядок величин"
                allow="clipboard-write"
            />
        </div>
    );
};

export default GamesPage;
