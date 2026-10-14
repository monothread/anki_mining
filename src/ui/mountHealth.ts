import { fetchHealth } from '../ipc/health';
import type { HealthStatus } from '../types/health';

export function formatHealth(status: HealthStatus): string {
    return `${status.name} v${status.version}`;
}

export async function mountHealth(root: HTMLElement): Promise<void> {
    try {
        const status = await fetchHealth();
        root.textContent = formatHealth(status);
    } catch (error) {
        root.textContent = `Backend unavailable: ${String(error)}`;
    }
}
