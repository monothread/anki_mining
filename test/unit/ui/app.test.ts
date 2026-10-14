import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { AppView } from '../../../src/ui/app';
import { createApp, describeHoverMode } from '../../../src/ui/app';
import type { ScanResult } from '../../../src/types/scan';
import type { FakeBackend } from '../../support/fakeBackend';
import { createFakeBackend } from '../../support/fakeBackend';
import { CAT_ENTRY, CAT_RESULT, EAT_RESULT, HOVER, SENTENCE, STATUS } from '../../support/fixtures';

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
