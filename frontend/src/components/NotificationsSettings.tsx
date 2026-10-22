import React, {useEffect, useState} from 'react';
import {
    disablePush,
    enablePush,
    getPushState,
    needsInstallForPush,
    preparePush,
    type EnableStep,
    type PreparedPush,
    type PushState,
} from '../notifications/push';

const STEP_TEXT: Record<EnableStep, string> = {
    permission: 'Разрешите уведомления в запросе браузера (если окна не видно — нажмите на значок колокольчика или замка в адресной строке)…',
    save: 'Сохраняем подписку…',
};

const STATUS_TEXT: Record<PushState, string> = {
    enabled: 'Уведомления включены на этом устройстве: новое и изменённое ДЗ придёт, даже когда сайт закрыт.',
    disabled: 'Уведомления выключены на этом устройстве.',
    denied: 'Уведомления заблокированы в браузере. Разрешите их в настройках сайта (значок замка в адресной строке) и обновите страницу.',
    unsupported: 'Этот браузер не поддерживает уведомления.',
};

/** Profile block: turn browser push notifications on/off for this device. */
const NotificationsSettings: React.FC = () => {
    const [state, setState] = useState<PushState | null>(null);
    const [busy, setBusy] = useState(false);
    const [step, setStep] = useState<EnableStep | null>(null);
    const [error, setError] = useState<string | null>(null);
    // Service worker + VAPID key, ready before the click (Safari subscribes only on a click).
    const [prepared, setPrepared] = useState<PreparedPush | null>(null);

    useEffect(() => {
        let isCurrent = true;
        getPushState()
            .then((current) => isCurrent && setState(current))
            .catch(() => isCurrent && setState('unsupported'));
        return () => {
            isCurrent = false;
        };
    }, []);

    useEffect(() => {
        if (state !== 'disabled' || prepared) {
            return;
        }
        let isCurrent = true;
        preparePush()
            .then((ready) => isCurrent && setPrepared(ready))
            .catch((prepareError: unknown) => {
                if (isCurrent) {
                    setError(prepareError instanceof Error ? prepareError.message : 'Уведомления недоступны.');
                }
            });
        return () => {
            isCurrent = false;
        };
    }, [prepared, state]);

    const toggle = async () => {
        if (state !== 'enabled' && !prepared) {
            return;
        }
        // No awaits before enablePush: it must start inside the click (Safari).
        setBusy(true);
        setError(null);
        try {
            setState(state === 'enabled' ? await disablePush() : await enablePush(prepared!, setStep));
        } catch (toggleError) {
            setError(toggleError instanceof Error ? toggleError.message : 'Не удалось изменить настройку уведомлений.');
        } finally {
            setBusy(false);
            setStep(null);
        }
    };

    if (state === null) {
        return null;
    }
    const iosHint = state === 'unsupported' && needsInstallForPush();

    return (
        <section className="notifications-settings" aria-labelledby="notifications-title">
            <h2 id="notifications-title">Уведомления</h2>
            <p className="register-card__description">
                {iosHint
                    ? 'На iPhone уведомления работают только в установленном приложении: нажмите «Поделиться» → «На экран „Домой“» и откройте сайт оттуда.'
                    : STATUS_TEXT[state]}
            </p>
            {busy && step && <p className="register-card__description" role="status">{STEP_TEXT[step]}</p>}
            {error && <p className="register-card__error" role="alert">{error}</p>}
            {(state === 'enabled' || state === 'disabled') && (
                <button
                    className="register-form__submit"
                    type="button"
                    disabled={busy || (state === 'disabled' && !prepared)}
                    onClick={() => void toggle()}
                >
                    {busy
                        ? 'Подождите…'
                        : state === 'enabled'
                            ? 'Выключить уведомления'
                            : prepared ? 'Включить уведомления' : 'Подготовка…'}
                </button>
            )}
        </section>
    );
};

export default NotificationsSettings;
