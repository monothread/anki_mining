import type { Entry } from './entry';

export interface DeinflectedEntry {
    entry: Entry;
    /** The text that was looked up, exactly as written. */
    source: string;
    /** Inflections undone to reach the entry, outermost first; empty for an exact match. */
    reasons: string[];
}

export interface ScanResult {
    source: string;
    /** Length of `source` in UTF-16 code units. */
    length: number;
    entries: DeinflectedEntry[];
}
