import { beforeEach, describe, expect, it, vi } from 'vitest';

const invokeMock = vi.hoisted(() => {
    return vi.fn();
});

vi.mock('@tauri-apps/api/core', () => {
    return { invoke: invokeMock };
});

import { tauriSelectorBackend } from '../../../src/ipc/selector';

describe('tauriSelectorBackend', () => {
    beforeEach(() => {
        invokeMock.mockReset();
        invokeMock.mockResolvedValue(undefined);
    });

    it('sends the selected region to set_ocr_region', async () => {
        const region = { x: 10, y: 900, width: 1200, height: 120 };

        await tauriSelectorBackend.select(region);

        expect(invokeMock).toHaveBeenCalledTimes(1);
        expect(invokeMock).toHaveBeenCalledWith('set_ocr_region', { region });
    });

    it('asks the backend to close the selector on cancel', async () => {
        await tauriSelectorBackend.cancel();

        expect(invokeMock).toHaveBeenCalledTimes(1);
        expect(invokeMock).toHaveBeenCalledWith('cancel_region_selection');
    });

    it('propagates backend errors', async () => {
        invokeMock.mockRejectedValue('the selected area is too small (3x3)');

        await expect(tauriSelectorBackend.select({ x: 0, y: 0, width: 3, height: 3 })).rejects.toBe(
            'the selected area is too small (3x3)',
        );
    });
});
