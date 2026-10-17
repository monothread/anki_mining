import type { Backend, Unsubscribe } from '../ipc/backend';
import { buildCardInput } from '../cards/cardInput';
import type { HoverStatus } from '../types/hover';
import type { Region } from '../types/region';
import type { DeinflectedEntry, ScanResult } from '../types/scan';
import type { Sentence } from '../types/sentence';
import type { Status } from '../types/status';
import type { Token } from '../types/token';
import { renderLookup } from './lookupView';
import { renderSettings } from './settingsView';
import { markSelected, renderSentence } from './sentenceView';

/** Label of the button that switches how lookups start (the other mode is what a click gives). */
export function describeHoverMode(hover: HoverStatus): string {
    if (hover.mode === 'auto') {
        return `Trigger: auto (${String(hover.dwellMs)} ms)`;
    }
    const key = hover.key.charAt(0).toUpperCase() + hover.key.slice(1);
    return `Trigger: hold ${key}`;
}

/** Describes the screen area being read, e.g. `ocr 1920x150`. */
export function describeOcr(available: boolean, region: Region | null): string {
    if (!available) {
        return 'ocr unavailable';
    }
    if (region === null) {
        return 'ocr idle';
    }
    return `ocr ${String(region.width)}x${String(region.height)}`;
}

export type MessageKind = 'info' | 'success' | 'error';

interface Selection {
    sentence: Sentence;
    token: Token;
    results: ScanResult[];
}

export interface AppView {
    /** Stops listening to backend events. */
    dispose: () => void;
}

const TEMPLATE = `
<header class="bar" data-tauri-drag-region>
    <span class="title" data-tauri-drag-region>AnkiMining</span>
    <span class="status" id="status" data-tauri-drag-region></span>
    <span class="actions">
        <button type="button" id="hover-mode" class="action" title="How a lookup under the mouse starts"></button>
        <button type="button" id="hover-toggle" class="action"></button>
        <button type="button" id="open-settings" class="action">Settings</button>
        <button type="button" id="select-area" class="action">Read screen area</button>
        <button type="button" id="stop-area" class="action" hidden>Stop reading</button>
    </span>
    <span class="window-controls">
        <button type="button" id="window-minimize" class="window-button" title="Minimize" aria-label="Minimize">&#8211;</button>
        <button type="button" id="window-maximize" class="window-button" title="Maximize" aria-label="Maximize">&#9633;</button>
        <button type="button" id="window-close" class="window-button close" title="Close" aria-label="Close">&#10005;</button>
    </span>
</header>
<section id="setup" class="setup" hidden>
    <p>The dictionary is empty. Enter the path of a <code>jmdict-eng-*.json</code> file (jmdict-simplified):</p>
    <form id="import-form">
        <input id="import-path" type="text" placeholder="/path/to/jmdict-eng.json" autocomplete="off" />
        <button type="submit">Import</button>
    </form>
</section>
<section id="settings" class="settings" hidden></section>
<main id="sentence" class="sentence">Pause the video and move the mouse over a Japanese subtitle, or copy text, or paste it below.</main>
<section id="lookup" class="lookup"></section>
<footer class="footer">
    <form id="text-form">
        <input id="text-input" type="text" placeholder="Paste Japanese text and press Enter" autocomplete="off" />
    </form>
    <div id="message" class="message" role="status"></div>
</footer>
`;

function requireElement<T extends HTMLElement>(
    root: HTMLElement,
    selector: string,
    type: new () => T,
): T {
    const element = root.querySelector(selector);
    if (!(element instanceof type)) {
        throw new Error(`Missing element ${selector}`);
    }
    return element;
}

/** Builds the overlay inside `root` and connects it to `backend`. */
export async function createApp(root: HTMLElement, backend: Backend): Promise<AppView> {
    root.innerHTML = TEMPLATE;
    const statusElement = requireElement(root, '#status', HTMLElement);
    const setupElement = requireElement(root, '#setup', HTMLElement);
    const sentenceElement = requireElement(root, '#sentence', HTMLElement);
    const lookupElement = requireElement(root, '#lookup', HTMLElement);
    const messageElement = requireElement(root, '#message', HTMLElement);
    const hoverModeButton = requireElement(root, '#hover-mode', HTMLButtonElement);
    const minimizeButton = requireElement(root, '#window-minimize', HTMLButtonElement);
    const maximizeButton = requireElement(root, '#window-maximize', HTMLButtonElement);
    const closeButton = requireElement(root, '#window-close', HTMLButtonElement);
    const settingsButton = requireElement(root, '#open-settings', HTMLButtonElement);
    const settingsElement = requireElement(root, '#settings', HTMLElement);
    const hoverToggleButton = requireElement(root, '#hover-toggle', HTMLButtonElement);
    const selectAreaButton = requireElement(root, '#select-area', HTMLButtonElement);
    const stopAreaButton = requireElement(root, '#stop-area', HTMLButtonElement);
    const importForm = requireElement(root, '#import-form', HTMLFormElement);
    const importPath = requireElement(root, '#import-path', HTMLInputElement);
    const textForm = requireElement(root, '#text-form', HTMLFormElement);
    const textInput = requireElement(root, '#text-input', HTMLInputElement);

    let selection: Selection | null = null;
    let requestCounter = 0;

    const showMessage = (text: string, kind: MessageKind): void => {
        messageElement.textContent = text;
        messageElement.dataset['kind'] = kind;
    };

    const showStatus = (status: Status): void => {
        const port = status.websocketPort === null ? 'off' : String(status.websocketPort);
        const ocr = describeOcr(status.ocrAvailable, status.ocrRegion);
        const mpv = status.mpvConnected ? ' - mpv connected' : '';
        statusElement.textContent = `${String(status.dictionaryEntries)} entries - ws ${port} - ${ocr}${mpv} - ${status.miningHotkey} mines`;
        setupElement.hidden = status.dictionaryEntries > 0;
        selectAreaButton.disabled = !status.ocrAvailable;
        stopAreaButton.hidden = status.ocrRegion === null;
        if (status.warnings.length > 0) {
            showMessage(status.warnings.join(' | '), 'error');
        }
    };

    let hover: HoverStatus | null = null;

    const showHover = (status: HoverStatus): void => {
        hover = status;
        hoverModeButton.textContent = describeHoverMode(status);
        hoverToggleButton.textContent = status.enabled ? 'Hover lookup: on' : 'Hover lookup: off';
    };

    const refreshStatus = async (): Promise<void> => {
        showStatus(await backend.getStatus());
        showHover(await backend.getHover());
    };

    const mineEntry = async (
        current: Selection,
        result: ScanResult,
        found: DeinflectedEntry,
    ): Promise<void> => {
        const input = buildCardInput(current.sentence, current.token.start, result, found);
        try {
            const noteId = await backend.mine(input, null);
            showMessage(`Added ${found.entry.expression} (note ${String(noteId)})`, 'success');
        } catch (error) {
            showMessage(String(error), 'error');
        }
    };

    const selectToken = async (sentence: Sentence, token: Token): Promise<void> => {
        requestCounter += 1;
        const requestId = requestCounter;
        let results: ScanResult[];
        try {
            results = await backend.scanAt(sentence.text, token.start);
        } catch (error) {
            showMessage(String(error), 'error');
            return;
        }
        if (requestId !== requestCounter) {
            return;
        }

        const current: Selection = { sentence, token, results };
        selection = current;
        markSelected(sentenceElement, token.start);
        renderLookup(lookupElement, results, {
            onMine: (result, found) => {
                void mineEntry(current, result, found);
            },
            onSpeak: (reading) => {
                void backend.speak(reading).catch((error: unknown) => {
                    showMessage(String(error), 'error');
                });
            },
        });
    };

    const showSentence = (sentence: Sentence): void => {
        requestCounter += 1;
        selection = null;
        lookupElement.replaceChildren();
        renderSentence(sentenceElement, sentence, {
            onTokenSelect: (token) => {
                void selectToken(sentence, token);
            },
        });
    };

    const mineSelection = async (): Promise<void> => {
        const first = selection?.results[0]?.entries[0];
        if (selection === null || selection.results[0] === undefined || first === undefined) {
            showMessage('Hover a word with a dictionary entry first', 'info');
            return;
        }
        await mineEntry(selection, selection.results[0], first);
    };

    const windowCommand = (command: () => Promise<void>): void => {
        void command().catch((error: unknown) => {
            showMessage(String(error), 'error');
        });
    };
    minimizeButton.addEventListener('click', () => {
        windowCommand(backend.minimizeWindow);
    });
    maximizeButton.addEventListener('click', () => {
        windowCommand(backend.toggleMaximizeWindow);
    });
    closeButton.addEventListener('click', () => {
        windowCommand(backend.closeWindow);
    });

    const closeSettings = (): void => {
        settingsElement.hidden = true;
        settingsElement.replaceChildren();
        sentenceElement.hidden = false;
        lookupElement.hidden = false;
        void refreshStatus().catch((error: unknown) => {
            showMessage(String(error), 'error');
        });
    };

    const openSettings = async (): Promise<void> => {
        const settings = await backend.getSettings();
        sentenceElement.hidden = true;
        lookupElement.hidden = true;
        setupElement.hidden = true;
        settingsElement.hidden = false;
        renderSettings(settingsElement, settings, {
            onSave: (changed) => {
                void backend
                    .saveSettings(changed)
                    .then(() => {
                        showMessage('Settings saved', 'success');
                        closeSettings();
                    })
                    .catch((error: unknown) => {
                        showMessage(String(error), 'error');
                    });
            },
            onCancel: closeSettings,
            onImportYomitan: (path) => {
                showMessage('Importing the dictionary...', 'info');
                void backend
                    .importYomitan(path)
                    .then((stats) => {
                        showMessage(
                            `Imported ${String(stats.pitch)} pitch accents and ${String(stats.frequency)} frequencies`,
                            'success',
                        );
                    })
                    .catch((error: unknown) => {
                        showMessage(String(error), 'error');
                    });
            },
        });
    };

    settingsButton.addEventListener('click', () => {
        if (!settingsElement.hidden) {
            closeSettings();
            return;
        }
        void openSettings().catch((error: unknown) => {
            showMessage(String(error), 'error');
        });
    });

    hoverModeButton.addEventListener('click', () => {
        if (hover === null) {
            return;
        }
        const next = hover.mode === 'hold' ? 'auto' : 'hold';
        void backend
            .setHoverMode(next)
            .then(showHover)
            .catch((error: unknown) => {
                showMessage(String(error), 'error');
            });
    });

    hoverToggleButton.addEventListener('click', () => {
        if (hover === null) {
            return;
        }
        void backend
            .setHoverEnabled(!hover.enabled)
            .then(showHover)
            .catch((error: unknown) => {
                showMessage(String(error), 'error');
            });
    });

    selectAreaButton.addEventListener('click', () => {
        void backend.selectOcrRegion().catch((error: unknown) => {
            showMessage(String(error), 'error');
        });
    });

    stopAreaButton.addEventListener('click', () => {
        void backend.clearOcrRegion().catch((error: unknown) => {
            showMessage(String(error), 'error');
        });
    });

    importForm.addEventListener('submit', (event) => {
        event.preventDefault();
        const path = importPath.value.trim();
        if (path === '') {
            return;
        }
        showMessage('Importing the dictionary...', 'info');
        void backend
            .importDictionary(path)
            .then(async (stats) => {
                showMessage(`Imported ${String(stats.entries)} entries`, 'success');
                await refreshStatus();
            })
            .catch((error: unknown) => {
                showMessage(String(error), 'error');
            });
    });

    textForm.addEventListener('submit', (event) => {
        event.preventDefault();
        const text = textInput.value.trim();
        if (text === '') {
            return;
        }
        textInput.value = '';
        void backend.submitText(text).catch((error: unknown) => {
            showMessage(String(error), 'error');
        });
    });

    const subscriptions: Unsubscribe[] = await Promise.all([
        backend.onSentence(showSentence),
        backend.onMineHotkey(() => {
            void mineSelection();
        }),
        backend.onAppError((message) => {
            showMessage(message, 'error');
        }),
        backend.onMineResult((result) => {
            showMessage(result.message, result.success ? 'success' : 'error');
        }),
        backend.onHoverInfo((message) => {
            showMessage(message, 'info');
        }),
        backend.onOcrRegion((region) => {
            if (region === null) {
                showMessage('Stopped reading the screen', 'info');
            } else {
                showMessage(`Reading ${String(region.width)}x${String(region.height)} pixels of the screen`, 'success');
            }
            void refreshStatus();
        }),
    ]);
    await refreshStatus();

    return {
        dispose: () => {
            for (const unsubscribe of subscriptions) {
                unsubscribe();
            }
        },
    };
}
