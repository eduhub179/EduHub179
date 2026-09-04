import React from 'react';
import Header from '../components/Header';
import FullScheduleTable from '../components/Schedule/ScheduleTable.tsx';
import '../css/FullSchedule.css';

const FullSchedule: React.FC = () => {
    return (
        <>
            <Header />

            <main className="full-schedule">
                <div className="full-schedule__header">
                    <h1>Полное расписание</h1>
                </div>

                <FullScheduleTable />
            </main>
        </>
    );
};

export default FullSchedule;