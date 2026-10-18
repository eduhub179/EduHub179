import React, { useEffect } from 'react';
import { useNavigate, useParams } from 'react-router-dom';
import { useAuth } from '../auth/AuthProvider';
import AuthenticatedMathView from '../components/Math/AuthenticatedMathView';
import AdminMathView from '../components/Math/AdminMathView';
import GuestMathView from '../components/Math/GuestMathView';
import MathTopicsTable from '../components/Math/MathTopicsTable';
import { isAvailableClass } from '../components/Math/mathClasses';
import Header from '../components/Header';
import '../css/MathPage.css';

const MathPage: React.FC = () => {
    const { user } = useAuth();
    const { schoolClass: routeClass } = useParams<{ schoolClass: string }>();
    const navigate = useNavigate();
    const schoolClass = routeClass && isAvailableClass(routeClass) ? routeClass : undefined;

    useEffect(() => {
        if (routeClass && !isAvailableClass(routeClass)) {
            navigate('/math', { replace: true });
            return;
        }

        if (!routeClass) {
            if (user?.schoolClass && isAvailableClass(user.schoolClass)) {
                navigate(`/math/${encodeURIComponent(user.schoolClass)}`, { replace: true });
            }
        }
    }, [navigate, routeClass, user]);

    const selectClass = (nextClass: string) => {
        navigate(`/math/${encodeURIComponent(nextClass)}`);
    };

    const mathView = user?.name.trim().toLowerCase() === 'admin' ? (
        <AdminMathView
            onSelectClass={selectClass}
            schoolClass={schoolClass}
        />
    ) : user ? (
        <AuthenticatedMathView
            onSelectClass={selectClass}
            schoolClass={schoolClass}
            user={user}
        />
    ) : (
        <GuestMathView onSelectClass={selectClass} schoolClass={schoolClass} />
    );

    return (
        <>
            <Header />
            <main className="math-page">
                {mathView}
                {schoolClass && (
                    <MathTopicsTable isAdmin={user?.name.trim().toLowerCase() === 'admin'} />
                )}
            </main>
        </>
    );
};

export default MathPage;
