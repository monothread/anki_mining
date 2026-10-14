import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { HoverResult } from '../types/hover';
import type { MineResult } from '../types/mine';

/** What the hover popup window needs from the backend. */
export interface PopupBackend {
    /** Adds the chosen entry of the word that is shown. */
    mine: (resultIndex: number, entryIndex: number) => Promise<MineResult>;
    onHoverResult: (handler: (result: HoverResult) => void) => Promise<() => void>;
    onMineResult: (handler: (result: MineResult) => void) => Promise<() => void>;
}

export const tauriPopupBackend: PopupBackend = {
    mine: async (resultIndex, entryIndex) => {
        return invoke<MineResult>('mine_hover', { resultIndex, entryIndex });
    },
    onHoverResult: async (handler) => {
        return listen<HoverResult>('hover-result', (message) => {
            handler(message.payload);
        });
    },
    onMineResult: async (handler) => {
        return listen<MineResult>('mine-result', (message) => {
            handler(message.payload);
        });
    },
};
