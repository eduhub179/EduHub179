import { StrictMode } from "react";
// import { createRoot } from "react-dom/client";
import ReactDOM from 'react-dom/client';
import { BrowserRouter, Routes, Route, Navigate } from "react-router-dom";
import SchedulePage from "./pages/SchedulePage.tsx";
import MainPage from "./pages/MainPage.tsx";
import RegisterPage from "./pages/RegisterPage.tsx";
import LoginPage from "./pages/LoginPage.tsx";
import ProfilePage from "./pages/ProfilePage.tsx";
import MathPage from "./pages/MathPage.tsx";
import { AuthProvider } from "./auth/AuthProvider.tsx";

const root = ReactDOM.createRoot(
    document.getElementById('root') as HTMLElement
);

root.render(
    <StrictMode>
        <BrowserRouter>
            <AuthProvider>
                <Routes>
                    <Route path="/main/*" element={<MainPage />} />
                    <Route path="/schedule" element={<SchedulePage />} />
                    <Route path="/register" element={<RegisterPage />} />
                    <Route path="/login" element={<LoginPage />} />
                    <Route path="/profile" element={<ProfilePage />} />
                    <Route path="/math" element={<MathPage />} />
                    <Route path="/math/:schoolClass" element={<MathPage />} />

                    {/* Если открыть "/" — отправляем на главную */}
                    <Route path="/" element={<Navigate to="/main" replace />} />
                </Routes>
            </AuthProvider>
        </BrowserRouter>
    </StrictMode>
);