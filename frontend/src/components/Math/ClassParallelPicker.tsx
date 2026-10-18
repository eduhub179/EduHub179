import React from 'react';
import MathClassTable from './MathClassTable';

interface ClassParallelPickerProps {
    onSelectClass: (schoolClass: string) => void;
}

const ClassParallelPicker: React.FC<ClassParallelPickerProps> = ({ onSelectClass }) => (
    <MathClassTable onSelectClass={onSelectClass} />
);

export default ClassParallelPicker;
