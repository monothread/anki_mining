import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { HoverResult } from '../types/hover';
import type { MineResult } from '../types/mine';
import type { KnownWords } from '../ui/known';

/** What the hover popup window needs from the backend. */
export interface PopupBackend {
    /** Adds the chosen entry of the word that is shown. */
    mine: (resultIndex: number, entryIndex: number) => Promise<MineResult>;
    /** Reads a text aloud with the synthesized voice. */
    speak: (text: string) => Promise<void>;
    onHoverResult: (handler: (result: HoverResult) => void) => Promise<() => void>;
    onMineResult: (handler: (result: MineResult) => void) => Promise<() => void>;
    /** Which words of the sentence already have a card (arrives a moment after the sentence). */
    onKnownWords: (handler: (known: KnownWords) => void) => Promise<() => void>;
}

export const tauriPopupBackend: PopupBackend = {
    mine: async (resultIndex, entryIndex) => {
        return invoke<MineResult>('mine_hover', { resultIndex, entryIndex });
    },
    speak: async (text) => {
        await invoke('speak_text', { text });
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
    onKnownWords: async (handler) => {
        return listen<KnownWords>('hover-known', (message) => {
            handler(message.payload);
        });
    },
};
