import React from 'react';
import ClassTable, {type ClassOption} from '../ClassTable';

interface ClassParallelPickerProps {
    onSelectClass: (schoolClass: string) => void;
    classOptions: ClassOption[];
}

const ClassParallelPicker: React.FC<ClassParallelPickerProps> = ({onSelectClass, classOptions}) => (
    <ClassTable classOptions={classOptions} onSelectClass={onSelectClass} />
);

export default ClassParallelPicker;
