import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { PopupBackend } from '../../../src/ipc/popup';
import { createPopup } from '../../../src/popup/popup';
import type { HoverResult } from '../../../src/types/hover';
import type { MineResult } from '../../../src/types/mine';
import { HOVER_RESULT } from '../../support/fixtures';

interface Mounted {
    root: HTMLElement;
    mine: ReturnType<typeof vi.fn<PopupBackend['mine']>>;
    speak: ReturnType<typeof vi.fn<PopupBackend['speak']>>;
    emitHoverResult: (result: HoverResult) => void;
    emitMineResult: (result: MineResult) => void;
    unsubscribes: ReturnType<typeof vi.fn<() => void>>[];
    dispose: () => void;
}

async function mount(): Promise<Mounted> {
    const root = document.createElement('div');
    document.body.replaceChildren(root);
    let hoverHandler: (result: HoverResult) => void = () => {
        return undefined;
    };
    let mineHandler: (result: MineResult) => void = () => {
        return undefined;
    };
    const unsubscribes = [vi.fn<() => void>(), vi.fn<() => void>()];
    const mine = vi
        .fn<PopupBackend['mine']>()
        .mockResolvedValue({ success: true, message: 'Added 友達 (note 1)' });
    const speak = vi.fn<PopupBackend['speak']>().mockResolvedValue(undefined);
    const backend: PopupBackend = {
        mine,
        speak,
        onHoverResult: (handler) => {
            hoverHandler = handler;
            return Promise.resolve(unsubscribes[0] ?? vi.fn<() => void>());
        },
        onMineResult: (handler) => {
            mineHandler = handler;
            return Promise.resolve(unsubscribes[1] ?? vi.fn<() => void>());
        },
    };
    const dispose = await createPopup(root, backend);
    return {
        root,
        mine,
        speak,
        emitHoverResult: (result) => {
            hoverHandler(result);
        },
        emitMineResult: (result) => {
            mineHandler(result);
        },
        unsubscribes,
        dispose,
    };
}

function message(root: HTMLElement): HTMLElement {
    const element = root.querySelector<HTMLElement>('#popup-message');
    if (element === null) {
        throw new Error('no message element');
    }
    return element;
}

describe('createPopup', () => {
    beforeEach(() => {
        document.body.replaceChildren();
    });

    it('shows a received lookup with a button for the entry', async () => {
        const { root, emitHoverResult } = await mount();

        emitHoverResult(HOVER_RESULT);

        expect(root.querySelector('.popup-word')?.textContent).toBe('友達');
        expect(root.querySelectorAll('button.mine')).toHaveLength(1);
    });

    it('adds the chosen entry and shows the result', async () => {
        const { root, mine, emitHoverResult } = await mount();
        emitHoverResult(HOVER_RESULT);

        root.querySelector<HTMLButtonElement>('button.mine')?.click();

        await vi.waitFor(() => {
            expect(message(root).textContent).toBe('Added 友達 (note 1)');
        });
        expect(mine).toHaveBeenCalledTimes(1);
        expect(mine).toHaveBeenCalledWith(0, 0);
        expect(message(root).dataset['kind']).toBe('success');
    });

    it('shows the reason when adding fails', async () => {
        const { root, mine, emitHoverResult } = await mount();
        mine.mockResolvedValue({ success: false, message: 'the note is a duplicate' });
        emitHoverResult(HOVER_RESULT);

        root.querySelector<HTMLButtonElement>('button.mine')?.click();

        await vi.waitFor(() => {
            expect(message(root).textContent).toBe('the note is a duplicate');
        });
        expect(message(root).dataset['kind']).toBe('error');
    });

    it('shows a backend error that was thrown instead of returned', async () => {
        const { root, mine, emitHoverResult } = await mount();
        mine.mockRejectedValue('backend gone');
        emitHoverResult(HOVER_RESULT);

        root.querySelector<HTMLButtonElement>('button.mine')?.click();

        await vi.waitFor(() => {
            expect(message(root).textContent).toBe('backend gone');
        });
        expect(message(root).dataset['kind']).toBe('error');
    });

    it('shows the result of the mining shortcut', async () => {
        const { root, emitMineResult } = await mount();

        emitMineResult({ success: true, message: 'Added 猫 (note 7)' });

        expect(message(root).textContent).toBe('Added 猫 (note 7)');
        expect(message(root).dataset['kind']).toBe('success');
    });

    it('clears the previous message when a new lookup arrives', async () => {
        const { root, emitHoverResult, emitMineResult } = await mount();
        emitMineResult({ success: false, message: 'old problem' });

        emitHoverResult(HOVER_RESULT);

        expect(message(root).textContent).toBe('');
        expect(message(root).dataset['kind']).toBeUndefined();
    });

    it('unsubscribes from both events when disposed', async () => {
        const { unsubscribes, dispose } = await mount();

        dispose();

        expect(unsubscribes[0]).toHaveBeenCalledTimes(1);
        expect(unsubscribes[1]).toHaveBeenCalledTimes(1);
    });

    it('reads the entry aloud when its listen button is clicked', async () => {
        const { root, speak, emitHoverResult } = await mount();
        emitHoverResult(HOVER_RESULT);

        const buttons = root.querySelectorAll<HTMLButtonElement>('.speak');
        buttons[0]?.click();

        expect(buttons.length).toBeGreaterThan(0);
        expect(speak).toHaveBeenCalledTimes(1);
        expect(speak).toHaveBeenCalledWith(HOVER_RESULT.results[0]?.entries[0]?.entry.reading);
    });

    it('shows why the voice could not be played', async () => {
        const { root, speak, emitHoverResult } = await mount();
        speak.mockRejectedValue('The voice is not available (see the warning at the top)');
        emitHoverResult(HOVER_RESULT);

        root.querySelector<HTMLButtonElement>('.speak')?.click();
        await vi.waitFor(() => {
            expect(message(root).textContent).toBe('The voice is not available (see the warning at the top)');
        });

        expect(message(root).dataset['kind']).toBe('error');
    });
});

