import { invoke } from '@tauri-apps/api/core';
import type { Region } from '../types/region';

/** What the full-screen selector window can ask of the backend. */
export interface SelectorBackend {
    select: (region: Region) => Promise<void>;
    cancel: () => Promise<void>;
}

export const tauriSelectorBackend: SelectorBackend = {
    select: async (region) => {
        await invoke('set_ocr_region', { region });
    },
    cancel: async () => {
        await invoke('cancel_region_selection');
    },
};
