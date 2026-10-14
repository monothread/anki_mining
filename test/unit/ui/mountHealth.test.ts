import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { HealthStatus } from '../../../src/types/health';

const fetchHealthMock = vi.hoisted(() => {
    return vi.fn();
});

vi.mock('../../../src/ipc/health', () => {
    return { fetchHealth: fetchHealthMock };
});

import { formatHealth, mountHealth } from '../../../src/ui/mountHealth';

describe('formatHealth', () => {
    it('formats the name and the version with a "v" prefix', () => {
        const status: HealthStatus = { name: 'anki-mining', version: '0.1.0' };

        expect(formatHealth(status)).toBe('anki-mining v0.1.0');
    });
});

describe('mountHealth', () => {
    beforeEach(() => {
        fetchHealthMock.mockReset();
    });

    it('renders the formatted status into the root element', async () => {
        fetchHealthMock.mockResolvedValue({ name: 'anki-mining', version: '0.1.0' });
        const root = document.createElement('main');

        await mountHealth(root);

        expect(fetchHealthMock).toHaveBeenCalledTimes(1);
        expect(root.textContent).toBe('anki-mining v0.1.0');
    });

    it('renders the error message when the backend call fails', async () => {
        fetchHealthMock.mockRejectedValue(new Error('backend down'));
        const root = document.createElement('main');

        await mountHealth(root);

        expect(root.textContent).toBe('Backend unavailable: Error: backend down');
    });
});
