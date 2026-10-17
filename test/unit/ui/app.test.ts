import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { AppView } from '../../../src/ui/app';
import { createApp, describeHoverMode } from '../../../src/ui/app';
import type { ScanResult } from '../../../src/types/scan';
import type { FakeBackend } from '../../support/fakeBackend';
import { createFakeBackend } from '../../support/fakeBackend';
import { CAT_ENTRY, CAT_RESULT, EAT_RESULT, HOVER, SENTENCE, SETTINGS, STATUS } from '../../support/fixtures';

interface Mounted {
    root: HTMLElement;
    fake: FakeBackend;
    view: AppView;
}

async function mount(configure?: (fake: FakeBackend) => void): Promise<Mounted> {
    const root = document.createElement('div');
    document.body.replaceChildren(root);
    const fake = createFakeBackend();
    configure?.(fake);
    const view = await createApp(root, fake.backend);
    return { root, fake, view };
}

function text(root: HTMLElement, selector: string): string | null | undefined {
    return root.querySelector(selector)?.textContent;
}

function hover(root: HTMLElement, tokenText: string): void {
    const element = Array.from(root.querySelectorAll<HTMLElement>('.token')).find((candidate) => {
        return candidate.textContent === tokenText;
    });
    element?.dispatchEvent(new MouseEvent('mouseenter'));
}

describe('createApp status', () => {
    beforeEach(() => {
        document.body.replaceChildren();
    });

    it('shows the dictionary size, websocket port and mining hotkey', async () => {
        const { root, fake } = await mount();

        expect(fake.backend.getStatus).toHaveBeenCalledTimes(1);
        expect(text(root, '#status')).toBe('218898 entries - ws 6677 - ocr idle - Alt+M mines');
    });

    it('shows "off" when the websocket server is not running', async () => {
        const { root } = await mount((fake) => {
            vi.mocked(fake.backend.getStatus).mockResolvedValue({ ...STATUS, websocketPort: null });
        });

        expect(text(root, '#status')).toBe('218898 entries - ws off - ocr idle - Alt+M mines');
    });

    it('shows the startup warnings as an error message', async () => {
        const { root } = await mount((fake) => {
            vi.mocked(fake.backend.getStatus).mockResolvedValue({
                ...STATUS,
                warnings: ['clipboard unavailable', 'port in use'],
            });
        });

        expect(text(root, '#message')).toBe('clipboard unavailable | port in use');
        expect(root.querySelector<HTMLElement>('#message')?.dataset['kind']).toBe('error');
    });

    it('hides the dictionary setup when the dictionary has entries', async () => {
        const { root } = await mount();

        expect(root.querySelector<HTMLElement>('#setup')?.hidden).toBe(true);
    });

    it('shows the dictionary setup when the dictionary is empty', async () => {
        const { root } = await mount((fake) => {
            vi.mocked(fake.backend.getStatus).mockResolvedValue({ ...STATUS, dictionaryEntries: 0 });
        });

        expect(root.querySelector<HTMLElement>('#setup')?.hidden).toBe(false);
    });

    it('subscribes to sentences, the mining hotkey and errors', async () => {
        const { fake } = await mount();

        expect(fake.backend.onSentence).toHaveBeenCalledTimes(1);
        expect(fake.backend.onMineHotkey).toHaveBeenCalledTimes(1);
        expect(fake.backend.onAppError).toHaveBeenCalledTimes(1);
    });

    it('unsubscribes from every event on dispose', async () => {
        const { fake, view } = await mount();

        view.dispose();

        expect(fake.unsubscribes.sentence).toHaveBeenCalledTimes(1);
        expect(fake.unsubscribes.hotkey).toHaveBeenCalledTimes(1);
        expect(fake.unsubscribes.error).toHaveBeenCalledTimes(1);
    });

    it('shows errors reported by the backend', async () => {
        const { root, fake } = await mount();

        fake.emitAppError('tokenizer failed');

        expect(text(root, '#message')).toBe('tokenizer failed');
        expect(root.querySelector<HTMLElement>('#message')?.dataset['kind']).toBe('error');
    });
});

describe('createApp dictionary import', () => {
    async function mountEmpty(): Promise<Mounted> {
        return mount((fake) => {
            vi.mocked(fake.backend.getStatus)
                .mockResolvedValueOnce({ ...STATUS, dictionaryEntries: 0 })
                .mockResolvedValue(STATUS);
        });
    }

    function submitImport(root: HTMLElement, path: string): void {
        const input = root.querySelector<HTMLInputElement>('#import-path');
        if (input !== null) {
            input.value = path;
        }
        root.querySelector<HTMLFormElement>('#import-form')?.dispatchEvent(
            new Event('submit', { cancelable: true }),
        );
    }

    it('imports the given path, reports the result and refreshes the status', async () => {
        const { root, fake } = await mountEmpty();

        submitImport(root, '  /data/jmdict-eng.json  ');

        await vi.waitFor(() => {
            expect(text(root, '#message')).toBe('Imported 218898 entries');
        });
        expect(fake.backend.importDictionary).toHaveBeenCalledWith('/data/jmdict-eng.json');
        expect(fake.backend.getStatus).toHaveBeenCalledTimes(2);
        expect(root.querySelector<HTMLElement>('#setup')?.hidden).toBe(true);
        expect(root.querySelector<HTMLElement>('#message')?.dataset['kind']).toBe('success');
    });

    it('shows the failure when the import fails', async () => {
        const { root, fake } = await mountEmpty();
        vi.mocked(fake.backend.importDictionary).mockRejectedValue('cannot read `/x`: not found');

        submitImport(root, '/x');

        await vi.waitFor(() => {
            expect(text(root, '#message')).toBe('cannot read `/x`: not found');
        });
        expect(root.querySelector<HTMLElement>('#message')?.dataset['kind']).toBe('error');
        expect(root.querySelector<HTMLElement>('#setup')?.hidden).toBe(false);
    });

    it('ignores an empty path', async () => {
        const { root, fake } = await mountEmpty();

        submitImport(root, '   ');

        expect(fake.backend.importDictionary).not.toHaveBeenCalled();
    });
});

describe('createApp manual text', () => {
    function submitText(root: HTMLElement, value: string): void {
        const input = root.querySelector<HTMLInputElement>('#text-input');
        if (input !== null) {
            input.value = value;
        }
        root.querySelector<HTMLFormElement>('#text-form')?.dispatchEvent(
            new Event('submit', { cancelable: true }),
        );
    }

    it('sends the trimmed text to the backend and clears the input', async () => {
        const { root, fake } = await mount();

        submitText(root, '  猫が好きです  ');

        expect(fake.backend.submitText).toHaveBeenCalledWith('猫が好きです');
        expect(root.querySelector<HTMLInputElement>('#text-input')?.value).toBe('');
    });

    it('ignores blank text', async () => {
        const { root, fake } = await mount();

        submitText(root, '   ');

        expect(fake.backend.submitText).not.toHaveBeenCalled();
    });

    it('shows the error when the text cannot be submitted', async () => {
        const { root, fake } = await mount();
        vi.mocked(fake.backend.submitText).mockRejectedValue('channel closed');

        submitText(root, '猫');

        await vi.waitFor(() => {
            expect(text(root, '#message')).toBe('channel closed');
        });
    });
});

describe('createApp lookup and mining', () => {
    it('renders a received sentence with its word tokens', async () => {
        const { root, fake } = await mount();

        fake.emitSentence(SENTENCE);

        expect(text(root, '#sentence')).toBe('猫が好きです。');
        expect(root.querySelectorAll('#sentence .token')).toHaveLength(4);
    });

    it('scans from the hovered token, shows the entries and marks the token', async () => {
        const { root, fake } = await mount();
        fake.emitSentence(SENTENCE);

        hover(root, '猫');

        await vi.waitFor(() => {
            expect(text(root, '#lookup .expression')).toBe('猫');
        });
        expect(fake.backend.scanAt).toHaveBeenCalledWith('猫が好きです。', 0);
        expect(text(root, '#sentence .selected')).toBe('猫');
    });

    it('scans from the start offset of a token in the middle of the sentence', async () => {
        const { root, fake } = await mount();
        fake.emitSentence(SENTENCE);

        hover(root, '好き');

        await vi.waitFor(() => {
            expect(fake.backend.scanAt).toHaveBeenCalledWith('猫が好きです。', 2);
        });
    });

    it('shows the empty message when the word has no entry', async () => {
        const { root, fake } = await mount((created) => {
            vi.mocked(created.backend.scanAt).mockResolvedValue([]);
        });
        fake.emitSentence(SENTENCE);

        hover(root, 'が');

        await vi.waitFor(() => {
            expect(text(root, '#lookup .empty')).toBe('No dictionary entry');
        });
    });

    it('shows the error when the lookup fails and keeps the previous lookup', async () => {
        const { root, fake } = await mount();
        fake.emitSentence(SENTENCE);
        hover(root, '猫');
        await vi.waitFor(() => {
            expect(text(root, '#lookup .expression')).toBe('猫');
        });
        vi.mocked(fake.backend.scanAt).mockRejectedValueOnce('position 9 is outside the sentence');

        hover(root, '好き');

        await vi.waitFor(() => {
            expect(text(root, '#message')).toBe('position 9 is outside the sentence');
        });
        expect(text(root, '#lookup .expression')).toBe('猫');
    });

    it('ignores a slow lookup that was overtaken by a newer hover', async () => {
        let resolveFirst: (results: ScanResult[]) => void = () => {
            return undefined;
        };
        const { root, fake } = await mount((created) => {
            vi.mocked(created.backend.scanAt)
                .mockImplementationOnce(() => {
                    return new Promise<ScanResult[]>((resolve) => {
                        resolveFirst = resolve;
                    });
                })
                .mockResolvedValueOnce([EAT_RESULT]);
        });
        fake.emitSentence(SENTENCE);

        hover(root, '猫');
        hover(root, '好き');
        await vi.waitFor(() => {
            expect(text(root, '#lookup .expression')).toBe('食べる');
        });
        resolveFirst([CAT_RESULT]);
        await Promise.resolve();

        expect(text(root, '#lookup .expression')).toBe('食べる');
        expect(text(root, '#sentence .selected')).toBe('好き');
    });

    it('clears the lookup when a new sentence arrives', async () => {
        const { root, fake } = await mount();
        fake.emitSentence(SENTENCE);
        hover(root, '猫');
        await vi.waitFor(() => {
            expect(root.querySelector('#lookup .entry')).not.toBeNull();
        });

        fake.emitSentence({ ...SENTENCE, text: '犬', tokens: [] });

        expect(root.querySelector('#lookup .entry')).toBeNull();
        expect(text(root, '#sentence')).toBe('犬');
    });

    it('mines the clicked entry with the exact card input', async () => {
        const { root, fake } = await mount();
        fake.emitSentence(SENTENCE);
        hover(root, '猫');
        await vi.waitFor(() => {
            expect(root.querySelector('button.mine')).not.toBeNull();
        });

        root.querySelector<HTMLButtonElement>('button.mine')?.click();

        await vi.waitFor(() => {
            expect(text(root, '#message')).toBe('Added 猫 (note 1234)');
        });
        expect(fake.backend.mine).toHaveBeenCalledTimes(1);
        expect(fake.backend.mine).toHaveBeenCalledWith(
            {
                entry: CAT_ENTRY.entry,
                sentence: '猫が好きです。',
                highlight: [0, 1],
                source: 'clipboard - Player',
                audio: null,
                picture: null,
                wordAudio: null,
            },
            null,
        );
        expect(root.querySelector<HTMLElement>('#message')?.dataset['kind']).toBe('success');
    });

    it('shows the failure when mining fails (for example a duplicate)', async () => {
        const { root, fake } = await mount((created) => {
            vi.mocked(created.backend.mine).mockRejectedValue('the note is a duplicate');
        });
        fake.emitSentence(SENTENCE);
        hover(root, '猫');
        await vi.waitFor(() => {
            expect(root.querySelector('button.mine')).not.toBeNull();
        });

        root.querySelector<HTMLButtonElement>('button.mine')?.click();

        await vi.waitFor(() => {
            expect(text(root, '#message')).toBe('the note is a duplicate');
        });
        expect(root.querySelector<HTMLElement>('#message')?.dataset['kind']).toBe('error');
    });

    it('mines the first entry of the longest match when the hotkey is pressed', async () => {
        const { root, fake } = await mount((created) => {
            vi.mocked(created.backend.scanAt).mockResolvedValue([EAT_RESULT, CAT_RESULT]);
        });
        fake.emitSentence({ ...SENTENCE, text: '食べました', tokens: SENTENCE.tokens.slice(0, 1) });
        hover(root, '食べました'.slice(0, 1));
        await vi.waitFor(() => {
            expect(root.querySelector('.entry')).not.toBeNull();
        });

        fake.emitMineHotkey();

        await vi.waitFor(() => {
            expect(fake.backend.mine).toHaveBeenCalledTimes(1);
        });
        const [input, profileId] = vi.mocked(fake.backend.mine).mock.calls[0] ?? [];
        expect(profileId).toBeNull();
        expect(input?.entry.expression).toBe('食べる');
        expect(input?.highlight).toEqual([0, 5]);
        expect(input?.sentence).toBe('食べました');
        await vi.waitFor(() => {
            expect(text(root, '#message')).toBe('Added 食べる (note 1234)');
        });
    });

    it('asks to hover a word when the hotkey is pressed without a selection', async () => {
        const { root, fake } = await mount();
        fake.emitSentence(SENTENCE);

        fake.emitMineHotkey();

        await vi.waitFor(() => {
            expect(text(root, '#message')).toBe('Hover a word with a dictionary entry first');
        });
        expect(fake.backend.mine).not.toHaveBeenCalled();
    });

    it('asks to hover a word when the selected word has no entry', async () => {
        const { root, fake } = await mount((created) => {
            vi.mocked(created.backend.scanAt).mockResolvedValue([]);
        });
        fake.emitSentence(SENTENCE);
        hover(root, 'が');
        await vi.waitFor(() => {
            expect(root.querySelector('.empty')).not.toBeNull();
        });

        fake.emitMineHotkey();

        await vi.waitFor(() => {
            expect(text(root, '#message')).toBe('Hover a word with a dictionary entry first');
        });
        expect(fake.backend.mine).not.toHaveBeenCalled();
    });

    it('forgets the selection when a new sentence arrives', async () => {
        const { root, fake } = await mount();
        fake.emitSentence(SENTENCE);
        hover(root, '猫');
        await vi.waitFor(() => {
            expect(root.querySelector('.entry')).not.toBeNull();
        });

        fake.emitSentence(SENTENCE);
        fake.emitMineHotkey();

        await vi.waitFor(() => {
            expect(text(root, '#message')).toBe('Hover a word with a dictionary entry first');
        });
        expect(fake.backend.mine).not.toHaveBeenCalled();
    });
});

describe('describeHoverMode', () => {
    it('names the key to hold in hold mode', () => {
        expect(describeHoverMode({ ...HOVER, mode: 'hold', key: 'shift' })).toBe('Trigger: hold Shift');
        expect(describeHoverMode({ ...HOVER, mode: 'hold', key: 'control' })).toBe('Trigger: hold Control');
        expect(describeHoverMode({ ...HOVER, mode: 'hold', key: 'alt' })).toBe('Trigger: hold Alt');
    });

    it('shows the resting time in auto mode', () => {
        expect(describeHoverMode({ ...HOVER, mode: 'auto', dwellMs: 450 })).toBe('Trigger: auto (450 ms)');
    });
});

describe('createApp mpv status', () => {
    it('shows when mpv is connected', async () => {
        const { root } = await mount((fake) => {
            vi.mocked(fake.backend.getStatus).mockResolvedValue({ ...STATUS, mpvConnected: true });
        });

        expect(text(root, '#status')).toBe(
            '218898 entries - ws 6677 - ocr idle - mpv connected - Alt+M mines',
        );
    });

    it('does not mention mpv when it is not connected', async () => {
        const { root } = await mount();

        expect(text(root, '#status')).not.toContain('mpv');
    });
});

describe('createApp hover controls', () => {
    function button(root: HTMLElement, selector: string): HTMLButtonElement {
        const element = root.querySelector<HTMLButtonElement>(selector);
        if (element === null) {
            throw new Error(`missing ${selector}`);
        }
        return element;
    }

    it('shows the current trigger and whether hover lookup is on', async () => {
        const { root, fake } = await mount();

        expect(fake.backend.getHover).toHaveBeenCalledTimes(1);
        expect(button(root, '#hover-mode').textContent).toBe('Trigger: hold Shift');
        expect(button(root, '#hover-toggle').textContent).toBe('Hover lookup: on');
    });

    it('switches from hold to auto and shows the new label', async () => {
        const { root, fake } = await mount();

        button(root, '#hover-mode').click();

        await vi.waitFor(() => {
            expect(button(root, '#hover-mode').textContent).toBe('Trigger: hold Shift'.replace('hold Shift', 'auto (300 ms)'));
        });
        expect(fake.backend.setHoverMode).toHaveBeenCalledTimes(1);
        expect(fake.backend.setHoverMode).toHaveBeenCalledWith('auto');
    });

    it('switches back from auto to hold', async () => {
        const { root, fake } = await mount((created) => {
            vi.mocked(created.backend.getHover).mockResolvedValue({ ...HOVER, mode: 'auto' });
        });

        button(root, '#hover-mode').click();

        await vi.waitFor(() => {
            expect(fake.backend.setHoverMode).toHaveBeenCalledWith('hold');
        });
        await vi.waitFor(() => {
            expect(button(root, '#hover-mode').textContent).toBe('Trigger: hold Shift');
        });
    });

    it('turns hover lookup off and on again', async () => {
        const { root, fake } = await mount();

        button(root, '#hover-toggle').click();
        await vi.waitFor(() => {
            expect(button(root, '#hover-toggle').textContent).toBe('Hover lookup: off');
        });
        vi.mocked(fake.backend.setHoverEnabled).mockResolvedValueOnce({ ...HOVER, enabled: true });
        button(root, '#hover-toggle').click();

        await vi.waitFor(() => {
            expect(button(root, '#hover-toggle').textContent).toBe('Hover lookup: on');
        });
        expect(vi.mocked(fake.backend.setHoverEnabled).mock.calls).toEqual([[false], [true]]);
    });

    it('shows the error when changing the trigger fails', async () => {
        const { root, fake } = await mount();
        vi.mocked(fake.backend.setHoverMode).mockRejectedValue('backend gone');

        button(root, '#hover-mode').click();

        await vi.waitFor(() => {
            expect(text(root, '#message')).toBe('backend gone');
        });
        expect(button(root, '#hover-mode').textContent).toBe('Trigger: hold Shift');
    });

    it('shows the error when switching hover lookup fails', async () => {
        const { root, fake } = await mount();
        vi.mocked(fake.backend.setHoverEnabled).mockRejectedValue('backend gone');

        button(root, '#hover-toggle').click();

        await vi.waitFor(() => {
            expect(text(root, '#message')).toBe('backend gone');
        });
    });
});

describe('createApp mining shortcut result', () => {
    it('shows a card that was added with the shortcut', async () => {
        const { root, fake } = await mount();

        fake.emitMineResult({ success: true, message: 'Added 猫 (note 7)' });

        expect(text(root, '#message')).toBe('Added 猫 (note 7)');
        expect(root.querySelector<HTMLElement>('#message')?.dataset['kind']).toBe('success');
    });

    it('shows why a card could not be added', async () => {
        const { root, fake } = await mount();

        fake.emitMineResult({ success: false, message: 'the note is a duplicate' });

        expect(text(root, '#message')).toBe('the note is a duplicate');
        expect(root.querySelector<HTMLElement>('#message')?.dataset['kind']).toBe('error');
    });

    it('unsubscribes from mining results on dispose', async () => {
        const { fake, view } = await mount();

        view.dispose();

        expect(fake.unsubscribes.mine).toHaveBeenCalledTimes(1);
    });
});

describe('createApp hover explanations', () => {
    it('tells the user why a hover lookup showed nothing', async () => {
        const { root, fake } = await mount();

        fake.emitHoverInfo('No text recognized under the cursor');

        expect(text(root, '#message')).toBe('No text recognized under the cursor');
        expect(root.querySelector<HTMLElement>('#message')?.dataset['kind']).toBe('info');
    });

    it('unsubscribes from hover explanations on dispose', async () => {
        const { fake, view } = await mount();

        view.dispose();

        expect(fake.unsubscribes.info).toHaveBeenCalledTimes(1);
    });
});

describe('createApp screen reading controls', () => {
    function button(root: HTMLElement, selector: string): HTMLButtonElement {
        const element = root.querySelector<HTMLButtonElement>(selector);
        if (element === null) {
            throw new Error(`missing ${selector}`);
        }
        return element;
    }

    it('opens the area selector from the button', async () => {
        const { root, fake } = await mount();

        button(root, '#select-area').click();

        expect(fake.backend.selectOcrRegion).toHaveBeenCalledTimes(1);
    });

    it('shows the error when the selector cannot be opened', async () => {
        const { root, fake } = await mount();
        vi.mocked(fake.backend.selectOcrRegion).mockRejectedValue('text recognition is not available');

        button(root, '#select-area').click();

        await vi.waitFor(() => {
            expect(text(root, '#message')).toBe('text recognition is not available');
        });
    });

    it('disables the area button when text recognition is unavailable', async () => {
        const { root } = await mount((fake) => {
            vi.mocked(fake.backend.getStatus).mockResolvedValue({ ...STATUS, ocrAvailable: false });
        });

        expect(button(root, '#select-area').disabled).toBe(true);
        expect(text(root, '#status')).toBe('218898 entries - ws 6677 - ocr unavailable - Alt+M mines');
    });

    it('shows the stop button and the area size while a region is read', async () => {
        const { root } = await mount((fake) => {
            vi.mocked(fake.backend.getStatus).mockResolvedValue({
                ...STATUS,
                ocrRegion: { x: 0, y: 900, width: 1920, height: 150 },
            });
        });

        expect(button(root, '#stop-area').hidden).toBe(false);
        expect(text(root, '#status')).toBe('218898 entries - ws 6677 - ocr 1920x150 - Alt+M mines');
    });

    it('hides the stop button when no region is read', async () => {
        const { root } = await mount();

        expect(button(root, '#stop-area').hidden).toBe(true);
    });

    it('stops reading from the stop button', async () => {
        const { root, fake } = await mount();

        button(root, '#stop-area').click();

        expect(fake.backend.clearOcrRegion).toHaveBeenCalledTimes(1);
    });

    it('shows the error when stopping fails', async () => {
        const { root, fake } = await mount();
        vi.mocked(fake.backend.clearOcrRegion).mockRejectedValue('nothing to stop');

        button(root, '#stop-area').click();

        await vi.waitFor(() => {
            expect(text(root, '#message')).toBe('nothing to stop');
        });
    });

    it('announces a new region and refreshes the status', async () => {
        const { root, fake } = await mount();
        vi.mocked(fake.backend.getStatus).mockResolvedValue({
            ...STATUS,
            ocrRegion: { x: 0, y: 900, width: 1920, height: 150 },
        });

        fake.emitOcrRegion({ x: 0, y: 900, width: 1920, height: 150 });

        await vi.waitFor(() => {
            expect(text(root, '#message')).toBe('Reading 1920x150 pixels of the screen');
        });
        await vi.waitFor(() => {
            expect(text(root, '#status')).toBe('218898 entries - ws 6677 - ocr 1920x150 - Alt+M mines');
        });
        expect(root.querySelector<HTMLElement>('#message')?.dataset['kind']).toBe('success');
    });

    it('announces that reading stopped', async () => {
        const { root, fake } = await mount();

        fake.emitOcrRegion(null);

        await vi.waitFor(() => {
            expect(text(root, '#message')).toBe('Stopped reading the screen');
        });
        expect(root.querySelector<HTMLElement>('#message')?.dataset['kind']).toBe('info');
    });

    it('unsubscribes from region updates on dispose', async () => {
        const { fake, view } = await mount();

        view.dispose();

        expect(fake.unsubscribes.region).toHaveBeenCalledTimes(1);
    });
});

describe('createApp settings', () => {
    it('opens the settings in place of the sentence and loads them from the backend', async () => {
        const { root, fake } = await mount();

        root.querySelector<HTMLButtonElement>('#open-settings')?.click();
        await vi.waitFor(() => {
            expect(root.querySelector<HTMLElement>('#settings')?.hidden).toBe(false);
        });

        expect(fake.backend.getSettings).toHaveBeenCalledTimes(1);
        expect(root.querySelector<HTMLElement>('#sentence')?.hidden).toBe(true);
        expect(root.querySelectorAll('.profile')).toHaveLength(1);
    });

    it('saves the settings, says so and closes the panel', async () => {
        const { root, fake } = await mount();
        root.querySelector<HTMLButtonElement>('#open-settings')?.click();
        await vi.waitFor(() => {
            expect(root.querySelector('.save-settings')).not.toBeNull();
        });

        root.querySelector<HTMLButtonElement>('.save-settings')?.click();
        await vi.waitFor(() => {
            expect(text(root, '#message')).toBe('Settings saved');
        });

        expect(fake.backend.saveSettings).toHaveBeenCalledWith(SETTINGS);
        expect(root.querySelector<HTMLElement>('#settings')?.hidden).toBe(true);
        expect(root.querySelector<HTMLElement>('#sentence')?.hidden).toBe(false);
    });

    it('keeps the panel open and shows the reason when saving fails', async () => {
        const { root, fake } = await mount((created) => {
            vi.mocked(created.backend.saveSettings).mockRejectedValue('profile `x` has an empty deckName');
        });
        root.querySelector<HTMLButtonElement>('#open-settings')?.click();
        await vi.waitFor(() => {
            expect(root.querySelector('.save-settings')).not.toBeNull();
        });

        root.querySelector<HTMLButtonElement>('.save-settings')?.click();
        await vi.waitFor(() => {
            expect(text(root, '#message')).toBe('profile `x` has an empty deckName');
        });

        expect(fake.backend.saveSettings).toHaveBeenCalledTimes(1);
        expect(root.querySelector<HTMLElement>('#settings')?.hidden).toBe(false);
    });

    it('closes with the button and again with the Settings button', async () => {
        const { root } = await mount();
        const open = root.querySelector<HTMLButtonElement>('#open-settings');
        open?.click();
        await vi.waitFor(() => {
            expect(root.querySelector<HTMLElement>('#settings')?.hidden).toBe(false);
        });

        open?.click();

        expect(root.querySelector<HTMLElement>('#settings')?.hidden).toBe(true);
        expect(root.querySelector('.profile')).toBeNull();
    });

    it('shows the reason when the settings cannot be loaded', async () => {
        const { root } = await mount((created) => {
            vi.mocked(created.backend.getSettings).mockRejectedValue('settings unavailable');
        });

        root.querySelector<HTMLButtonElement>('#open-settings')?.click();
        await vi.waitFor(() => {
            expect(text(root, '#message')).toBe('settings unavailable');
        });

        expect(root.querySelector<HTMLElement>('#settings')?.hidden).toBe(true);
    });
});

describe('createApp window controls', () => {
    it.each([
        ['#window-minimize', 'minimizeWindow'],
        ['#window-maximize', 'toggleMaximizeWindow'],
        ['#window-close', 'closeWindow'],
    ] as const)('%s calls %s once', async (selector, command) => {
        const { root, fake } = await mount();

        root.querySelector<HTMLButtonElement>(selector)?.click();

        expect(fake.backend[command]).toHaveBeenCalledTimes(1);
    });

    it('shows the reason when a window command fails', async () => {
        const { root } = await mount((created) => {
            vi.mocked(created.backend.closeWindow).mockRejectedValue('cannot close');
        });

        root.querySelector<HTMLButtonElement>('#window-close')?.click();
        await vi.waitFor(() => {
            expect(text(root, '#message')).toBe('cannot close');
        });
    });

    it('hides the dictionary import while the settings are open and brings it back after', async () => {
        const { root } = await mount((created) => {
            vi.mocked(created.backend.getStatus).mockResolvedValue({ ...STATUS, dictionaryEntries: 0 });
        });
        const setup = root.querySelector<HTMLElement>('#setup');
        expect(setup?.hidden).toBe(false);

        root.querySelector<HTMLButtonElement>('#open-settings')?.click();
        await vi.waitFor(() => {
            expect(root.querySelector('.save-settings')).not.toBeNull();
        });
        expect(setup?.hidden).toBe(true);

        root.querySelector<HTMLButtonElement>('.cancel-settings')?.click();
        await vi.waitFor(() => {
            expect(setup?.hidden).toBe(false);
        });
    });

    it('imports a Yomitan dictionary from the settings and reports what it added', async () => {
        const { root, fake } = await mount();
        root.querySelector<HTMLButtonElement>('#open-settings')?.click();
        await vi.waitFor(() => {
            expect(root.querySelector('.import-yomitan')).not.toBeNull();
        });
        const input = root.querySelector<HTMLInputElement>('input[name="yomitanPath"]');
        if (input) {
            input.value = '/data/pitch.zip';
        }

        root.querySelector<HTMLButtonElement>('.import-yomitan')?.click();
        await vi.waitFor(() => {
            expect(text(root, '#message')).toBe('Imported 120 pitch accents and 300 frequencies');
        });

        expect(fake.backend.importYomitan).toHaveBeenCalledWith('/data/pitch.zip');
        expect(root.querySelector<HTMLElement>('#settings')?.hidden).toBe(false);
    });

    it('shows why a Yomitan import failed', async () => {
        const { root } = await mount((created) => {
            vi.mocked(created.backend.importYomitan).mockRejectedValue('cannot read `/x.zip`: not found');
        });
        root.querySelector<HTMLButtonElement>('#open-settings')?.click();
        await vi.waitFor(() => {
            expect(root.querySelector('.import-yomitan')).not.toBeNull();
        });
        const input = root.querySelector<HTMLInputElement>('input[name="yomitanPath"]');
        if (input) {
            input.value = '/x.zip';
        }

        root.querySelector<HTMLButtonElement>('.import-yomitan')?.click();
        await vi.waitFor(() => {
            expect(text(root, '#message')).toBe('cannot read `/x.zip`: not found');
        });
    });

    it('reads the clicked entry aloud in the main window and shows why it failed', async () => {
        const { root, fake } = await mount();
        fake.emitSentence(SENTENCE);
        hover(root, '猫');
        await vi.waitFor(() => {
            expect(root.querySelector('.speak')).not.toBeNull();
        });

        root.querySelector<HTMLButtonElement>('.speak')?.click();
        expect(fake.backend.speak).toHaveBeenCalledWith('ねこ');

        vi.mocked(fake.backend.speak).mockRejectedValue('no program to play sound was found');
        root.querySelector<HTMLButtonElement>('.speak')?.click();
        await vi.waitFor(() => {
            expect(text(root, '#message')).toBe('no program to play sound was found');
        });
    });

    it('asks Anki about the dictionary form of every word of the sentence and underlines the answer', async () => {
        const { root, fake } = await mount((created) => {
            vi.mocked(created.backend.knownWords).mockResolvedValue([true, false, true]);
        });

        fake.emitSentence(SENTENCE);
        await vi.waitFor(() => {
            expect(root.querySelector('.token.known')).not.toBeNull();
        });

        expect(fake.backend.knownWords).toHaveBeenCalledWith(['猫', 'が', '好き', 'です']);
        const tokens = [...root.querySelectorAll('.token')].map((token) => {
            return [token.textContent, token.className];
        });
        expect(tokens).toEqual([
            ['猫', 'token known'],
            ['が', 'token unknown'],
            ['好き', 'token known'],
            ['です', 'token unknown'],
        ]);
    });

    it('shows the sentence without underlines when Anki cannot be reached', async () => {
        const { root, fake } = await mount((created) => {
            vi.mocked(created.backend.knownWords).mockRejectedValue('cannot reach Anki');
        });

        fake.emitSentence(SENTENCE);
        await vi.waitFor(() => {
            expect(fake.backend.knownWords).toHaveBeenCalled();
        });

        expect(root.querySelectorAll('.token')).toHaveLength(4);
        expect(root.querySelector('.known, .unknown')).toBeNull();
        expect(text(root, '#message')).not.toBe('cannot reach Anki');
    });

    it('does not apply an answer that arrives after the next sentence', async () => {
        let answer: (flags: boolean[]) => void = () => {
            return undefined;
        };
        const { root, fake } = await mount((created) => {
            vi.mocked(created.backend.knownWords).mockImplementationOnce(() => {
                return new Promise<boolean[]>((resolve) => {
                    answer = resolve;
                });
            });
        });
        fake.emitSentence(SENTENCE);
        fake.emitSentence({ ...SENTENCE, text: '犬', tokens: [{ ...SENTENCE.tokens[0] as (typeof SENTENCE.tokens)[number], surface: '犬', lemma: '犬', start: 0, end: 1 }] });

        answer([true, true, true, true]);
        await Promise.resolve();

        expect(root.querySelector('.known')).toBeNull();
    });

    it('underlines the word right after it was added and shows the button as added', async () => {
        const { root, fake } = await mount();
        fake.emitSentence(SENTENCE);
        hover(root, '猫');
        await vi.waitFor(() => {
            expect(root.querySelector('button.mine')).not.toBeNull();
        });

        root.querySelector<HTMLButtonElement>('button.mine')?.click();
        await vi.waitFor(() => {
            expect(root.querySelector('button.mine')?.textContent).toBe('Added \u2713');
        });

        expect(root.querySelector('.token.known')?.textContent).toBe('猫');
        expect(root.querySelectorAll('.token.known')).toHaveLength(1);
    });

    it('does not underline the word when adding fails', async () => {
        const { root, fake } = await mount((created) => {
            vi.mocked(created.backend.mine).mockRejectedValue('duplicate');
        });
        fake.emitSentence(SENTENCE);
        hover(root, '猫');
        await vi.waitFor(() => {
            expect(root.querySelector('button.mine')).not.toBeNull();
        });

        root.querySelector<HTMLButtonElement>('button.mine')?.click();
        await vi.waitFor(() => {
            expect(text(root, '#message')).toBe('duplicate');
        });

        expect(root.querySelector('.token.known')).toBeNull();
        expect(root.querySelector<HTMLButtonElement>('button.mine')?.disabled).toBe(false);
    });

    it('shows a banner when the app found a newer version and installs it on request', async () => {
        const { root, fake } = await mount();
        const banner = root.querySelector<HTMLElement>('#update');
        expect(banner?.hidden).toBe(true);

        fake.emitUpdateAvailable({ version: '0.2.0', currentVersion: '0.1.0', notes: null });

        expect(banner?.hidden).toBe(false);
        expect(text(root, '#update-text')).toBe('Version 0.2.0 is available (you have 0.1.0).');

        root.querySelector<HTMLButtonElement>('#update-install')?.click();

        expect(fake.backend.installUpdate).toHaveBeenCalledTimes(1);
        expect(root.querySelector<HTMLButtonElement>('#update-install')?.disabled).toBe(true);
        expect(text(root, '#update-install')).toBe('Installing\u2026');
    });

    it('lets the user hide the banner for now', async () => {
        const { root, fake } = await mount();
        fake.emitUpdateAvailable({ version: '0.2.0', currentVersion: '0.1.0', notes: 'x' });

        root.querySelector<HTMLButtonElement>('#update-later')?.click();

        expect(root.querySelector<HTMLElement>('#update')?.hidden).toBe(true);
    });

    it('shows why an update could not be installed and lets the user try again', async () => {
        const { root, fake } = await mount((created) => {
            vi.mocked(created.backend.installUpdate).mockRejectedValue('the signature does not match');
        });
        fake.emitUpdateAvailable({ version: '0.2.0', currentVersion: '0.1.0', notes: null });

        root.querySelector<HTMLButtonElement>('#update-install')?.click();
        await vi.waitFor(() => {
            expect(text(root, '#message')).toBe('the signature does not match');
        });

        expect(root.querySelector<HTMLButtonElement>('#update-install')?.disabled).toBe(false);
        expect(text(root, '#update-install')).toBe('Install and restart');
    });

    it('checks for updates from the settings and says when this is the latest version', async () => {
        const { root, fake } = await mount();
        root.querySelector<HTMLButtonElement>('#open-settings')?.click();
        await vi.waitFor(() => {
            expect(root.querySelector('.check-now')).not.toBeNull();
        });

        root.querySelector<HTMLButtonElement>('.check-now')?.click();
        await vi.waitFor(() => {
            expect(text(root, '#message')).toBe('You have the latest version');
        });

        expect(fake.backend.checkUpdate).toHaveBeenCalledTimes(1);
        expect(root.querySelector<HTMLElement>('#update')?.hidden).toBe(true);
    });

    it('shows the banner when the check from the settings finds a newer version', async () => {
        const { root } = await mount((created) => {
            vi.mocked(created.backend.checkUpdate).mockResolvedValue({
                version: '0.3.0',
                currentVersion: '0.1.0',
                notes: null,
            });
        });
        root.querySelector<HTMLButtonElement>('#open-settings')?.click();
        await vi.waitFor(() => {
            expect(root.querySelector('.check-now')).not.toBeNull();
        });

        root.querySelector<HTMLButtonElement>('.check-now')?.click();
        await vi.waitFor(() => {
            expect(root.querySelector<HTMLElement>('#update')?.hidden).toBe(false);
        });

        expect(text(root, '#message')).toBe('Version 0.3.0 is available');
    });

    it('shows why the check failed', async () => {
        const { root } = await mount((created) => {
            vi.mocked(created.backend.checkUpdate).mockRejectedValue('no network');
        });
        root.querySelector<HTMLButtonElement>('#open-settings')?.click();
        await vi.waitFor(() => {
            expect(root.querySelector('.check-now')).not.toBeNull();
        });

        root.querySelector<HTMLButtonElement>('.check-now')?.click();
        await vi.waitFor(() => {
            expect(text(root, '#message')).toBe('no network');
        });
    });
});

