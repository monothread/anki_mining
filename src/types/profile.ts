export const MINING_FIELDS = [
    'expression',
    'reading',
    'meaning',
    'sentence',
    'sentenceAudio',
    'picture',
    'frequency',
    'pitch',
    'source',
] as const;

export type MiningField = (typeof MINING_FIELDS)[number];

/** Maps each mining field to the name of the Anki note type field that receives it. */
export type FieldMapping = Record<MiningField, string>;

export const DEFAULT_FIELD_MAPPING: FieldMapping = {
    expression: 'Expression',
    reading: 'Reading',
    meaning: 'Meaning',
    sentence: 'Sentence',
    sentenceAudio: 'SentenceAudio',
    picture: 'Picture',
    frequency: 'Frequency',
    pitch: 'Pitch',
    source: 'Source',
};

export interface ProfileMatch {
    sourceId: string | null;
    appName: string | null;
    /** Part of the window title (any case). */
    windowTitle: string | null;
}

export interface Profile {
    id: string;
    name: string;
    deckName: string;
    modelName: string;
    fieldMapping: FieldMapping;
    match: ProfileMatch;
}
