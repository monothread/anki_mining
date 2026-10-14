import { invoke } from '@tauri-apps/api/core';
import type { HealthStatus } from '../types/health';

export async function fetchHealth(): Promise<HealthStatus> {
    return invoke<HealthStatus>('health_check');
}
