import { StrictMode } from "react";
// import { createRoot } from "react-dom/client";
import ReactDOM from 'react-dom/client';
import { BrowserRouter, Routes, Route, Navigate } from "react-router-dom";
import MainPage from "./pages/MainPage.tsx";
import SchedulePage from "./pages/SchedulePage.tsx";

const root = ReactDOM.createRoot(
    document.getElementById('root') as HTMLElement
);

root.render(
    <StrictMode>
        <BrowserRouter>
            <Routes>
                <Route path="/main/*" element={<MainPage />} />
                <Route path="/schedule" element={<SchedulePage />} />

                {/* Если открыть "/" — отправляем на главную */}
                <Route path="/" element={<Navigate to="/main" replace />} />
            </Routes>
        </BrowserRouter>
    </StrictMode>
);