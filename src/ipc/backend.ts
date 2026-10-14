import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { CardInput } from '../types/card';
import type { HoverStatus, TriggerMode } from '../types/hover';
import type { Region } from '../types/region';
import type { ScanResult } from '../types/scan';
import type { Sentence } from '../types/sentence';
import type { ImportStats, Status } from '../types/status';

/** Removes the event subscription it was returned for. */
export type Unsubscribe = () => void;

/** Everything the UI needs from the Rust side; replaced by a fake in tests. */
export interface Backend {
    getStatus: () => Promise<Status>;
    getHover: () => Promise<HoverStatus>;
    setHoverMode: (mode: TriggerMode) => Promise<HoverStatus>;
    setHoverEnabled: (enabled: boolean) => Promise<HoverStatus>;
    scanAt: (sentence: string, start: number) => Promise<ScanResult[]>;
    mine: (input: CardInput, profileId: string | null) => Promise<number>;
    importDictionary: (path: string) => Promise<ImportStats>;
    submitText: (text: string) => Promise<void>;
    /** Opens the full-screen selector; the result arrives through `onOcrRegion`. */
    selectOcrRegion: () => Promise<void>;
    /** Stops reading the screen. */
    clearOcrRegion: () => Promise<void>;
    onSentence: (handler: (sentence: Sentence) => void) => Promise<Unsubscribe>;
    onMineHotkey: (handler: () => void) => Promise<Unsubscribe>;
    onAppError: (handler: (message: string) => void) => Promise<Unsubscribe>;
    onOcrRegion: (handler: (region: Region | null) => void) => Promise<Unsubscribe>;
    /** Why a hover lookup showed nothing (no text, punctuation, no dictionary entry). */
    onHoverInfo: (handler: (message: string) => void) => Promise<Unsubscribe>;
}

export const tauriBackend: Backend = {
    getStatus: async () => {
        return invoke<Status>('get_status');
    },
    getHover: async () => {
        return invoke<HoverStatus>('get_hover');
    },
    setHoverMode: async (mode) => {
        return invoke<HoverStatus>('set_hover_mode', { mode });
    },
    setHoverEnabled: async (enabled) => {
        return invoke<HoverStatus>('set_hover_enabled', { enabled });
    },
    scanAt: async (sentence, start) => {
        return invoke<ScanResult[]>('scan_at', { sentence, start });
    },
    mine: async (input, profileId) => {
        return invoke<number>('mine', { input, profileId });
    },
    importDictionary: async (path) => {
        return invoke<ImportStats>('import_dictionary', { path });
    },
    submitText: async (text) => {
        await invoke('submit_text', { text });
    },
    selectOcrRegion: async () => {
        await invoke('open_region_selector');
    },
    clearOcrRegion: async () => {
        await invoke('set_ocr_region', { region: null });
    },
    onSentence: async (handler) => {
        return listen<Sentence>('sentence', (message) => {
            handler(message.payload);
        });
    },
    onMineHotkey: async (handler) => {
        return listen('mine-hotkey', () => {
            handler();
        });
    },
    onAppError: async (handler) => {
        return listen<string>('app-error', (message) => {
            handler(message.payload);
        });
    },
    onOcrRegion: async (handler) => {
        return listen<Region | null>('ocr-region', (message) => {
            handler(message.payload);
        });
    },
    onHoverInfo: async (handler) => {
        return listen<string>('hover-info', (message) => {
            handler(message.payload);
        });
    },
};
