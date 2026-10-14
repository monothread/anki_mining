import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { HealthStatus } from '../../../src/types/health';

const invokeMock = vi.hoisted(() => {
    return vi.fn();
});

vi.mock('@tauri-apps/api/core', () => {
    return { invoke: invokeMock };
});

import { fetchHealth } from '../../../src/ipc/health';

describe('fetchHealth', () => {
    beforeEach(() => {
        invokeMock.mockReset();
    });

    it('invokes the health_check command without arguments and returns its result', async () => {
        const status: HealthStatus = { name: 'anki-mining', version: '0.1.0' };
        invokeMock.mockResolvedValue(status);

        const result = await fetchHealth();

        expect(invokeMock).toHaveBeenCalledTimes(1);
        expect(invokeMock).toHaveBeenCalledWith('health_check');
        expect(result).toEqual({ name: 'anki-mining', version: '0.1.0' });
    });

    it('propagates the error when the backend call fails', async () => {
        invokeMock.mockRejectedValue(new Error('backend down'));

        await expect(fetchHealth()).rejects.toThrow('backend down');
        expect(invokeMock).toHaveBeenCalledWith('health_check');
    });
});
