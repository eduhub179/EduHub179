import React, {useState, useCallback} from 'react';
import '../css/index.css';
import Schedule from '../components/Schedule/Schedule.tsx';
import MainContent from '../components/MainContent.tsx';
import Header from '../components/Header.tsx';

const MIN_WIDTH = 200;
const MAX_WIDTH = 800;
const DEFAULT_WIDTH = 400;

const MainPage: React.FC = () => {
    const [sidebarWidth, setSidebarWidth] = useState(DEFAULT_WIDTH);
    const [isResizing, setIsResizing] = useState(false);

    const handlePointerDown = useCallback((e: React.PointerEvent) => {
        e.preventDefault();
        setIsResizing(true);
        const startX = e.clientX;
        const startWidth = sidebarWidth;

        const handlePointerMove = (moveEvent: PointerEvent) => {
            const newWidth = startWidth + (moveEvent.clientX - startX);
            const clampedWidth = Math.min(Math.max(newWidth, MIN_WIDTH), MAX_WIDTH);
            setSidebarWidth(clampedWidth);
        };

        const handlePointerUp = () => {
            setIsResizing(false);
            window.removeEventListener('pointermove', handlePointerMove);
            window.removeEventListener('pointerup', handlePointerUp);
        };

        window.addEventListener('pointermove', handlePointerMove);
        window.addEventListener('pointerup', handlePointerUp);
    }, [sidebarWidth]);

    const handleDoubleClick = () => {
        setSidebarWidth(DEFAULT_WIDTH);
    };

    return (
        <>
            <Header/>
            <div className={`app-container ${isResizing ? 'app-container--resizing' : ''}`}>
                <div className="sidebar" style={{width: sidebarWidth}}>
                    <Schedule/>
                </div>
                <div
                    className={`resizer ${isResizing ? 'resizer--active' : ''}`}
                    onPointerDown={handlePointerDown}
                    onDoubleClick={handleDoubleClick}
                />
                <div className="main-content-wrapper">
                    <MainContent/>
                </div>
            </div>
        </>
    );
};

export default MainPage;