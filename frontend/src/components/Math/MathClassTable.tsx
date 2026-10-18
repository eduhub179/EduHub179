import React from 'react';
import classParallels from './mathClasses';

interface MathClassTableProps {
    selectedClass?: string;
    onSelectClass: (schoolClass: string) => void;
}

const MathClassTable: React.FC<MathClassTableProps> = ({ selectedClass, onSelectClass }) => (
    <div className="math-class-table-wrap">
        <table className="math-class-table" aria-label="Выбор класса">
            <thead>
                <tr>
                    <th scope="col">Параллель</th>
                    {['А', 'Б', 'В'].map((letter) => (
                        <th key={letter} scope="col">{letter}</th>
                    ))}
                </tr>
            </thead>
            <tbody>
                {classParallels.map(({ grade, classes }) => (
                    <tr key={grade}>
                        <th scope="row">{grade} класс</th>
                        {classes.map((schoolClass) => (
                            <td key={schoolClass}>
                                <button
                                    aria-pressed={selectedClass === schoolClass}
                                    className="math-class-table__button"
                                    onClick={() => onSelectClass(schoolClass)}
                                    type="button"
                                >
                                    {schoolClass}
                                </button>
                            </td>
                        ))}
                    </tr>
                ))}
            </tbody>
        </table>
    </div>
);

export default MathClassTable;
