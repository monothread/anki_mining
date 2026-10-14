import type { ScanResult } from './scan';

export type TriggerMode = 'hold' | 'auto';
export type TriggerKey = 'shift' | 'control' | 'alt';

/** What the user can change about the lookup under the mouse cursor. */
export interface HoverStatus {
    enabled: boolean;
    mode: TriggerMode;
    key: TriggerKey;
    dwellMs: number;
}

/** The word found under the cursor, as sent to the popup. */
export interface HoverResult {
    sentence: string;
    /** Where the word starts in `sentence` (UTF-16 code units). */
    wordStart: number;
    results: ScanResult[];
    /** `[x1, y1, x2, y2]` of the character under the cursor, in screen pixels. */
    charBbox: [number, number, number, number];
    /** `[x1, y1, x2, y2]` of the whole line, in screen pixels. */
    lineBbox: [number, number, number, number];
}
