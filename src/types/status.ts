import type { Region } from './region';

export interface Status {
    dictionaryEntries: number;
    warnings: string[];
    miningHotkey: string;
    websocketPort: number | null;
    defaultProfileId: string;
    /** False when the OCR models or the screen could not be loaded. */
    ocrAvailable: boolean;
    /** Screen area that is being read, if any. */
    ocrRegion: Region | null;
    /** Whether a running mpv is connected. */
    mpvConnected: boolean;
}

export interface ImportStats {
    entries: number;
    terms: number;
    skipped: number;
}
