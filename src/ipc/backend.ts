import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import type { CardInput } from '../types/card';
import type { HoverStatus, TriggerMode } from '../types/hover';
import type { MineResult } from '../types/mine';
import type { Region } from '../types/region';
import type { ScanResult } from '../types/scan';
import type { Sentence } from '../types/sentence';
import type { MiningSettings } from '../types/settings';
import type { ImportStats, MetadataStats, Status } from '../types/status';
import type { UpdateInfo } from '../types/update';

/** Removes the event subscription it was returned for. */
export type Unsubscribe = () => void;

/** Everything the UI needs from the Rust side; replaced by a fake in tests. */
export interface Backend {
    minimizeWindow: () => Promise<void>;
    toggleMaximizeWindow: () => Promise<void>;
    /** Closes the window, which closes the whole app. */
    closeWindow: () => Promise<void>;
    getStatus: () => Promise<Status>;
    getHover: () => Promise<HoverStatus>;
    setHoverMode: (mode: TriggerMode) => Promise<HoverStatus>;
    setHoverEnabled: (enabled: boolean) => Promise<HoverStatus>;
    getSettings: () => Promise<MiningSettings>;
    /** Saves the settings; rejects with the reason when they are not valid. */
    saveSettings: (settings: MiningSettings) => Promise<MiningSettings>;
    scanAt: (sentence: string, start: number) => Promise<ScanResult[]>;
    mine: (input: CardInput, profileId: string | null) => Promise<number>;
    importDictionary: (path: string) => Promise<ImportStats>;
    /** Adds the pitch accents and frequencies of a Yomitan dictionary zip. */
    importYomitan: (path: string) => Promise<MetadataStats>;
    /** For each word, whether a card of it is already in the deck (rejects when Anki cannot be reached). */
    knownWords: (words: string[]) => Promise<boolean[]>;
    /** Asks GitHub whether a newer version exists; null when this is the latest. */
    checkUpdate: () => Promise<UpdateInfo | null>;
    /** Downloads and installs the newer version, then restarts the app. */
    installUpdate: () => Promise<void>;
    /** The app found a newer version a little after it started. */
    onUpdateAvailable: (handler: (update: UpdateInfo) => void) => Promise<Unsubscribe>;
    /** Reads a text aloud with the synthesized voice. */
    speak: (text: string) => Promise<void>;
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
    /** A word was added (or could not be added) with the mining shortcut. */
    onMineResult: (handler: (result: MineResult) => void) => Promise<Unsubscribe>;
}

export const tauriBackend: Backend = {
    minimizeWindow: async () => {
        await getCurrentWindow().minimize();
    },
    toggleMaximizeWindow: async () => {
        await getCurrentWindow().toggleMaximize();
    },
    closeWindow: async () => {
        await getCurrentWindow().close();
    },
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
    getSettings: async () => {
        return invoke<MiningSettings>('get_settings');
    },
    saveSettings: async (settings) => {
        return invoke<MiningSettings>('save_settings', { settings });
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
    importYomitan: async (path) => {
        return invoke<MetadataStats>('import_yomitan', { path });
    },
    knownWords: async (words) => {
        return invoke<boolean[]>('known_words', { words });
    },
    checkUpdate: async () => {
        return invoke<UpdateInfo | null>('check_update');
    },
    installUpdate: async () => {
        await invoke('install_update');
    },
    onUpdateAvailable: async (handler) => {
        return listen<UpdateInfo>('update-available', (message) => {
            handler(message.payload);
        });
    },
    speak: async (text) => {
        await invoke('speak_text', { text });
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
    onMineResult: async (handler) => {
        return listen<MineResult>('mine-result', (message) => {
            handler(message.payload);
        });
    },
};
