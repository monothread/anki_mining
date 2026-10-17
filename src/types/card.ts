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
    /** The word read by the synthesized voice; the app fills it in when the card is made. */
    wordAudio: MediaFile | null;
}
