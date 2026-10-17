import type { PopupBackend } from '../ipc/popup';
import type { MineResult } from '../types/mine';
import { markAdded, markKnown } from '../ui/known';
import { setMineState } from '../ui/lookupView';
import type { HoverResult } from '../types/hover';
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

    const showAdding = (): void => {
        message.textContent = 'Adding to Anki\u2026';
        message.dataset['kind'] = 'info';
    };

    /** The button of the entry the mining shortcut adds (the first one). */
    const shortcutButton = (): HTMLButtonElement | null => {
        return body.querySelector<HTMLButtonElement>('button.mine');
    };

    const showMessage = (result: MineResult): void => {
        message.textContent = result.message;
        message.dataset['kind'] = result.success ? 'success' : 'error';
    };

    let shown: HoverResult | null = null;

    /** Underlines the words of the sentence that are the entry that was just added. */
    const markEntryAdded = (resultIndex: number, entryIndex: number): void => {
        const current = shown;
        const expression = current?.results[resultIndex]?.entries[entryIndex]?.entry.expression;
        if (current === null || expression === undefined) {
            return;
        }
        const starts = current.tokens
            .filter((token) => {
                return token.start === current.wordStart || token.lemma === expression;
            })
            .map((token) => {
                return token.start;
            });
        markAdded(body, '.popup-token', starts);
    };

    const unsubscribes = await Promise.all([
        backend.onHoverResult((result) => {
            shown = result;
            message.textContent = '';
            delete message.dataset['kind'];
            renderHoverResult(body, result, {
                onSpeak: (reading) => {
                    void backend.speak(reading).catch((error: unknown) => {
                        showMessage({ success: false, message: String(error) });
                    });
                },
                onMine: (resultIndex, entryIndex) => {
                    showAdding();
                    return backend
                        .mine(resultIndex, entryIndex)
                        .then((outcome) => {
                            showMessage(outcome);
                            if (outcome.success) {
                                markEntryAdded(resultIndex, entryIndex);
                            }
                            return outcome.success;
                        })
                        .catch((error: unknown) => {
                            showMessage({ success: false, message: String(error) });
                            return false;
                        });
                },
            });
        }),
        backend.onMineStarted(() => {
            showAdding();
            const button = shortcutButton();
            if (button !== null) {
                setMineState(button, 'loading');
            }
        }),
        backend.onMineResult((outcome) => {
            showMessage(outcome);
            const button = shortcutButton();
            if (button !== null) {
                setMineState(button, outcome.success ? 'done' : 'idle');
            }
            if (outcome.success) {
                // The shortcut adds the first entry of the word that is shown.
                markEntryAdded(0, 0);
            }
        }),
        backend.onKnownWords((known) => {
            if (known.sentence === shown?.sentence) {
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
