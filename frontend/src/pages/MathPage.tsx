import React, {useEffect, useMemo, useState} from 'react';
import {useNavigate, useParams} from 'react-router-dom';
import {fetchSchoolClasses, hasAdminRole, type SchoolClassDto} from '../api';
import {useAuth} from '../auth/AuthProvider';
import AuthenticatedMathView from '../components/Math/AuthenticatedMathView';
import AdminMathView from '../components/Math/AdminMathView';
import GuestMathView from '../components/Math/GuestMathView';
import type {ClassOption} from '../components/ClassTable';
import MathTopicsTable from '../components/Math/MathTopicsTable';
import CourseMaterialsLinks from '../components/Materials/CourseMaterialsLinks';
import Header from '../components/Header';
import '../css/MathPage.css';

const getSchoolYear = () => {
    const now = new Date();
    return now.getFullYear() - (now.getMonth() < 7 ? 1 : 0);
};

const getClassLabel = (schoolClass: SchoolClassDto) => {
    const grade = getSchoolYear() + 12 - schoolClass.graduation_year;
    return `${grade}${schoolClass.letter.toLocaleUpperCase('ru-RU')}`;
};

const MathPage: React.FC = () => {
    const {user} = useAuth();
    const {schoolClass: routeClass} = useParams<{ schoolClass: string }>();
    const navigate = useNavigate();
    const [classes, setClasses] = useState<SchoolClassDto[] | null>(null);
    const [classLoadError, setClassLoadError] = useState<string | null>(null);
    const classOptions = useMemo<ClassOption[]>(
        () => (classes ?? []).map((schoolClass) => {
            const label = getClassLabel(schoolClass);
            return {
                grade: Number.parseInt(label, 10),
                letter: schoolClass.letter.toLocaleUpperCase('ru-RU'),
                label,
                value: label,
            };
        }),
        [classes],
    );
    const findClass = (value: string | undefined) => {
        if (!value) {
            return undefined;
        }

        const normalizedValue = value.trim().toLocaleLowerCase('ru-RU');
        return classes?.find((schoolClass) =>
            [schoolClass.id, schoolClass.name, getClassLabel(schoolClass)]
                .some((candidate) => candidate.trim().toLocaleLowerCase('ru-RU') === normalizedValue),
        );
    };
    const selectedClass = findClass(routeClass);
    const schoolClass = selectedClass ? getClassLabel(selectedClass) : undefined;

    useEffect(() => {
        let isCurrent = true;
        fetchSchoolClasses()
            .then(({classes: schoolClasses}) => {
                if (isCurrent) {
                    setClasses(schoolClasses.filter((schoolClass) => schoolClass.is_active));
                }
            })
            .catch((error: unknown) => {
                if (isCurrent) {
                    setClassLoadError(error instanceof Error ? error.message : 'Не удалось загрузить список классов.');
                }
            });

        return () => {
            isCurrent = false;
        };
    }, []);

    useEffect(() => {
        if (!classes || classLoadError) {
            return;
        }

        if (routeClass) {
            if (!selectedClass) {
                navigate('/math', {replace: true});
            } else if (routeClass !== getClassLabel(selectedClass)) {
                navigate(`/math/${encodeURIComponent(getClassLabel(selectedClass))}`, {replace: true});
            }
        } else if (user?.schoolClass) {
            const profileClass = findClass(user.schoolClass);
            if (profileClass) {
                navigate(`/math/${encodeURIComponent(getClassLabel(profileClass))}`, {replace: true});
            }
        }
    }, [classes, classLoadError, navigate, routeClass, selectedClass, user]);

    const selectClass = (nextClass: string) => {
        navigate(`/math/${encodeURIComponent(nextClass)}`);
    };

    const mathView = classes === null ? (
        <p className="math-content__description">Загрузка списка классов...</p>
    ) : classLoadError ? (
        <p className="math-topics__error" role="alert">{classLoadError}</p>
    ) : classes.length === 0 ? (
        <p className="math-content__description">В базе данных пока нет доступных классов.</p>
    ) : hasAdminRole(user) ? (
        <AdminMathView
            classOptions={classOptions}
            onSelectClass={selectClass}
            schoolClass={schoolClass}
        />
    ) : user ? (
        <AuthenticatedMathView
            classOptions={classOptions}
            onSelectClass={selectClass}
            schoolClass={schoolClass}
            user={user}
        />
    ) : (
        <GuestMathView classOptions={classOptions} onSelectClass={selectClass} schoolClass={schoolClass}/>
    );

    return (
        <>
            <Header/>
            <main className="math-page">
                {mathView}
                {schoolClass && selectedClass && (
                    <MathTopicsTable
                        schoolClass={selectedClass}
                        user={user}
                    />
                )}
                {schoolClass && selectedClass && (
                    <CourseMaterialsLinks
                        schoolClass={selectedClass}
                        user={user}
                        filter={(course) => /математ|алгебр|геометр/i.test(course.subject_name)}
                    />
                )}
            </main>
        </>
    );
};

export default MathPage;
