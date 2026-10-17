export interface Entry {
    expression: string;
    reading: string;
    meanings: string[];
    partsOfSpeech: string[];
    /** Frequency rank (lower is more common); null when unknown. */
    frequency: number | null;
    /** Downstep positions of the pitch accent (0 is flat); empty when unknown. */
    pitch: number[];
}
