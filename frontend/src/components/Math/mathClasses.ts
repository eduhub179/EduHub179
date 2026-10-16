const classLetters = ['А', 'Б', 'В'];
const parallelGrades = Array.from({ length: 5 }, (_, index) => index + 7);

export const availableClasses = parallelGrades.flatMap((grade) =>
    classLetters.map((letter) => `${grade}${letter}`),
);

export const isAvailableClass = (schoolClass: string): boolean =>
    availableClasses.includes(schoolClass);

export default parallelGrades.map((grade) => ({
    grade,
    classes: classLetters.map((letter) => `${grade}${letter}`),
}));
