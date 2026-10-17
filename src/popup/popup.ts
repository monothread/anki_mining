import type { PopupBackend } from '../ipc/popup';
import type { MineResult } from '../types/mine';
import { markKnown } from '../ui/known';
import { renderHoverResult } from './view';

const TEMPLATE = `
<div id="popup-body" class="popup-body"></div>
<div id="popup-message" class="popup-message" role="status"></div>
`;

/** Builds the hover popup inside `root` and connects it to `backend`. */
export async function createPopup(root: HTMLElement, backend: PopupBackend): Promise<() => void> {
    root.innerHTML = TEMPLATE;
    const body = root.querySelector<HTMLElement>('#popup-body');
    const message = root.querySelector<HTMLElement>('#popup-message');
    if (body === null || message === null) {
        throw new Error('Missing popup elements');
    }

    const showMessage = (result: MineResult): void => {
        message.textContent = result.message;
        message.dataset['kind'] = result.success ? 'success' : 'error';
    };

    let shownSentence: string | null = null;

    const unsubscribes = await Promise.all([
        backend.onHoverResult((result) => {
            shownSentence = result.sentence;
            message.textContent = '';
            delete message.dataset['kind'];
            renderHoverResult(body, result, {
                onSpeak: (reading) => {
                    void backend.speak(reading).catch((error: unknown) => {
                        showMessage({ success: false, message: String(error) });
                    });
                },
                onMine: (resultIndex, entryIndex) => {
                    void backend
                        .mine(resultIndex, entryIndex)
                        .then(showMessage)
                        .catch((error: unknown) => {
                            showMessage({ success: false, message: String(error) });
                        });
                },
            });
        }),
        backend.onMineResult(showMessage),
        backend.onKnownWords((known) => {
            if (known.sentence === shownSentence) {
                markKnown(body, '.popup-token', known.words);
            }
        }),
    ]);

    return () => {
        for (const unsubscribe of unsubscribes) {
            unsubscribe();
        }
    };
}
