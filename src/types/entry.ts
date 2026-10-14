export interface Entry {
    expression: string;
    reading: string;
    meanings: string[];
    partsOfSpeech: string[];
    /** Frequency rank (lower is more common); null when unknown. */
    frequency: number | null;
}
