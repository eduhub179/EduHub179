import React, {useEffect, useState} from 'react';
import {Link} from 'react-router-dom';
import {fetchClassCourses, formatClassName, type CourseDto, type SchoolClassDto} from '../../api';
import type {User} from '../../auth/authService';
import '../../css/CourseMaterialsPage.css';

interface CourseMaterialsLinksProps {
    schoolClass: SchoolClassDto;
    user: User | null;
    /** Which courses to list, e.g. only math subjects on the math page. */
    filter?: (course: CourseDto) => boolean;
}

/** Links to the materials pages of a class's courses (students see only their own). */
const CourseMaterialsLinks: React.FC<CourseMaterialsLinksProps> = ({schoolClass, user, filter}) => {
    const [courses, setCourses] = useState<CourseDto[] | null>(null);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        if (!user?.id) {
            return;
        }
        let isCurrent = true;
        setCourses(null);
        setError(null);
        fetchClassCourses(schoolClass.id)
            .then((loaded) => {
                if (isCurrent) setCourses(loaded);
            })
            .catch((loadError: unknown) => {
                if (isCurrent) {
                    setError(loadError instanceof Error ? loadError.message : 'Не удалось загрузить курсы.');
                }
            });
        return () => {
            isCurrent = false;
        };
    }, [schoolClass.id, user?.id]);

    if (!user?.id) {
        return null;
    }
    const visible = (courses ?? []).filter((course) => !filter || filter(course));

    return (
        <section className="math-topics course-materials-links" aria-label="Материалы курсов">
            <h2>Материалы курсов</h2>
            {error ? (
                <p className="course-materials__error" role="alert">{error}</p>
            ) : courses === null ? (
                <p className="course-materials__muted">Загрузка курсов...</p>
            ) : visible.length === 0 ? (
                <p className="course-materials__muted">Курсов для этого класса нет.</p>
            ) : (
                <ul>
                    {visible.map((course) => (
                        <li key={course.lesson_id}>
                            <Link to={`/courses/${course.lesson_id}/materials`}>
                                {course.subject_name}
                                {course.target_name !== schoolClass.name ? ` · ${formatClassName(course.target_name)}` : ''}
                            </Link>
                        </li>
                    ))}
                </ul>
            )}
        </section>
    );
};

export default CourseMaterialsLinks;
