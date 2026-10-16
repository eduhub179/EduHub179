import React, { useEffect, useState } from 'react';
import { useNavigate, useParams } from 'react-router-dom';
import { useAuth } from '../auth/AuthProvider';
import AuthenticatedMathView from '../components/Math/AuthenticatedMathView';
import GuestMathView from '../components/Math/GuestMathView';
import { isAvailableClass } from '../components/Math/mathClasses';
import Header from '../components/Header';
import '../css/MathPage.css';

const LAST_GUEST_CLASS_KEY = 'eduhub.math.lastClass';

const MathPage: React.FC = () => {
    const { user } = useAuth();
    const { schoolClass: routeClass } = useParams<{ schoolClass: string }>();
    const navigate = useNavigate();
    const [lastGuestClass] = useState(() => {
        const storedClass = localStorage.getItem(LAST_GUEST_CLASS_KEY);
        return storedClass && isAvailableClass(storedClass) ? storedClass : undefined;
    });
    const schoolClass = routeClass && isAvailableClass(routeClass) ? routeClass : undefined;

    useEffect(() => {
        if (routeClass && !isAvailableClass(routeClass)) {
            navigate('/math', { replace: true });
            return;
        }

        if (!routeClass) {
            const initialClass = user
                ? (user.schoolClass && isAvailableClass(user.schoolClass) ? user.schoolClass : undefined)
                : lastGuestClass;
            if (initialClass) {
                navigate(`/math/${encodeURIComponent(initialClass)}`, { replace: true });
            }
            return;
        }

        if (!user) {
            localStorage.setItem(LAST_GUEST_CLASS_KEY, routeClass);
        }
    }, [lastGuestClass, navigate, routeClass, user]);

    const selectClass = (nextClass: string) => {
        navigate(`/math/${encodeURIComponent(nextClass)}`);
    };

    return (
        <>
            <Header />
            <main className="math-page">
                {user ? (
                    <AuthenticatedMathView
                        onSelectClass={selectClass}
                        schoolClass={schoolClass}
                        user={user}
                    />
                ) : (
                    <GuestMathView onSelectClass={selectClass} schoolClass={schoolClass} />
                )}
            </main>
        </>
    );
};

export default MathPage;
