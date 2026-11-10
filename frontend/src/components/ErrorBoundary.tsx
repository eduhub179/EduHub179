import React from 'react';

interface ErrorBoundaryState {
    failed: boolean;
}

/**
 * Catches a crash while rendering a page: a short message instead of a blank screen.
 * The error itself goes to the browser console only, never to the screen.
 */
class ErrorBoundary extends React.Component<{ children: React.ReactNode }, ErrorBoundaryState> {
    state: ErrorBoundaryState = { failed: false };

    static getDerivedStateFromError(): ErrorBoundaryState {
        return { failed: true };
    }

    componentDidCatch(error: unknown) {
        console.error('page crashed', error);
    }

    render() {
        if (!this.state.failed) {
            return this.props.children;
        }
        return (
            <main style={{ maxWidth: 480, margin: '80px auto', padding: '0 16px', textAlign: 'center' }}>
                <h1 style={{ fontSize: '1.5rem' }}>Что-то пошло не так</h1>
                <p style={{ color: 'var(--color-text-muted)' }}>Обновите страницу. Если не поможет, попробуйте позже.</p>
                <button
                    className="register-form__submit"
                    onClick={() => window.location.reload()}
                    style={{ maxWidth: 240 }}
                    type="button"
                >
                    Обновить страницу
                </button>
            </main>
        );
    }
}

export default ErrorBoundary;
