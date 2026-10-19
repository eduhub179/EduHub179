import React from 'react';
import '../css/ClassTable.css';

export interface ClassOption {
    grade: number;
    letter: string;
    label: string;
    value: string;
}

interface ClassTableProps {
    selectedClass?: string;
    onSelectClass: (schoolClass: string) => void;
    classOptions: ClassOption[];
}

const ClassTable: React.FC<ClassTableProps> = ({selectedClass, onSelectClass, classOptions}) => {
    const grades = [...new Set(classOptions.map(({grade}) => grade))].sort((first, second) => first - second);
    const letters = [...new Set(classOptions.map(({letter}) => letter))]
        .sort((first, second) => first.localeCompare(second, 'ru-RU'));

    return (
        <div className="math-class-table-wrap">
            <table className="math-class-table" aria-label="Выбор класса">
                <tbody>
                    {grades.map((grade) => (
                        <tr key={grade}>
                            {letters.map((letter) => {
                                const option = classOptions.find((item) => item.grade === grade && item.letter === letter);
                                return (
                                    <td key={letter}>
                                        {option && (
                                            <button
                                                aria-pressed={selectedClass === option.value}
                                                className="math-class-table__button"
                                                onClick={() => onSelectClass(option.value)}
                                                type="button"
                                            >
                                                {option.label}
                                            </button>
                                        )}
                                    </td>
                                );
                            })}
                        </tr>
                    ))}
                </tbody>
            </table>
        </div>
    );
};

export default ClassTable;
