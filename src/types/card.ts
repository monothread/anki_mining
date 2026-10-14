import type { Entry } from './entry';

export interface MediaFile {
    filename: string;
    /** Base64-encoded file contents. */
    data: string;
}

export interface CardInput {
    entry: Entry;
    sentence: string;
    /** UTF-16 offsets `[start, end)` of the mined word inside the sentence. */
    highlight: [number, number] | null;
    source: string;
    audio: MediaFile | null;
    picture: MediaFile | null;
}
