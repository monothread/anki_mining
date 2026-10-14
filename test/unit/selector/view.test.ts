import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { SelectorBackend } from '../../../src/ipc/selector';
import { createSelector } from '../../../src/selector/view';

interface Mounted {
    root: HTMLElement;
    backend: SelectorBackend;
    select: ReturnType<typeof vi.fn<SelectorBackend['select']>>;
    cancel: ReturnType<typeof vi.fn<SelectorBackend['cancel']>>;
    onError: ReturnType<typeof vi.fn<(message: string) => void>>;
}

function mount(pixelRatio = 1): Mounted {
    const root = document.createElement('div');
    document.body.replaceChildren(root);
    const select = vi.fn<SelectorBackend['select']>().mockResolvedValue(undefined);
    const cancel = vi.fn<SelectorBackend['cancel']>().mockResolvedValue(undefined);
    const onError = vi.fn<(message: string) => void>();
    createSelector(root, { select, cancel }, pixelRatio, onError);
    return { root, backend: { select, cancel }, select, cancel, onError };
}

function mouse(type: string, x: number, y: number, button = 0): MouseEvent {
    return new MouseEvent(type, { clientX: x, clientY: y, button, bubbles: true });
}

function box(root: HTMLElement): HTMLElement {
    const element = root.querySelector<HTMLElement>('#box');
    if (element === null) {
        throw new Error('no box');
    }
    return element;
}

describe('createSelector', () => {
    beforeEach(() => {
        document.body.replaceChildren();
    });

    it('shows a hint and keeps the selection box hidden at first', () => {
        const { root } = mount();

        expect(root.querySelector('.hint')?.textContent).toBe(
            'Drag over the area with the subtitles - Esc cancels',
        );
        expect(box(root).hidden).toBe(true);
    });

    it('draws the box while dragging, whatever the direction', () => {
        const { root } = mount();

        root.dispatchEvent(mouse('mousedown', 300, 500));
        root.dispatchEvent(mouse('mousemove', 100, 450));

        const element = box(root);
        expect(element.hidden).toBe(false);
        expect(element.style.left).toBe('100px');
        expect(element.style.top).toBe('450px');
        expect(element.style.width).toBe('200px');
        expect(element.style.height).toBe('50px');
    });

    it('reports the dragged region in physical pixels on mouse up and hides the box', () => {
        const { root, select } = mount(2);

        root.dispatchEvent(mouse('mousedown', 100, 400));
        root.dispatchEvent(mouse('mousemove', 500, 440));
        root.dispatchEvent(mouse('mouseup', 500, 440));

        expect(select).toHaveBeenCalledTimes(1);
        expect(select).toHaveBeenCalledWith({ x: 200, y: 800, width: 800, height: 80 });
        expect(box(root).hidden).toBe(true);
    });

    it('ignores a click without a real drag', () => {
        const { root, select, cancel } = mount();

        root.dispatchEvent(mouse('mousedown', 100, 400));
        root.dispatchEvent(mouse('mouseup', 103, 402));

        expect(select).not.toHaveBeenCalled();
        expect(cancel).not.toHaveBeenCalled();
        expect(box(root).hidden).toBe(true);
    });

    it('ignores mouse movement and release when no drag started', () => {
        const { root, select } = mount();

        root.dispatchEvent(mouse('mousemove', 100, 400));
        root.dispatchEvent(mouse('mouseup', 500, 440));

        expect(select).not.toHaveBeenCalled();
        expect(box(root).hidden).toBe(true);
    });

    it('cancels with Escape', () => {
        const { cancel } = mount();

        document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));

        expect(cancel).toHaveBeenCalledTimes(1);
    });

    it('does not cancel with other keys', () => {
        const { cancel } = mount();

        document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));

        expect(cancel).not.toHaveBeenCalled();
    });

    it('cancels with a right click and drops the drag in progress', () => {
        const { root, select, cancel } = mount();
        root.dispatchEvent(mouse('mousedown', 100, 400));

        root.dispatchEvent(mouse('mousedown', 120, 410, 2));
        root.dispatchEvent(mouse('mouseup', 600, 500));

        expect(cancel).toHaveBeenCalledTimes(1);
        expect(select).not.toHaveBeenCalled();
        expect(box(root).hidden).toBe(true);
    });

    it('reports a failure to select through onError', async () => {
        const { root, select, onError } = mount();
        select.mockRejectedValue('the selected area is too small');

        root.dispatchEvent(mouse('mousedown', 100, 400));
        root.dispatchEvent(mouse('mouseup', 500, 440));

        await vi.waitFor(() => {
            expect(onError).toHaveBeenCalledWith('the selected area is too small');
        });
    });

    it('reports a failure to cancel through onError', async () => {
        const { cancel, onError } = mount();
        cancel.mockRejectedValue('window not found');

        document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));

        await vi.waitFor(() => {
            expect(onError).toHaveBeenCalledWith('window not found');
        });
    });
});
