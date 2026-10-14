export interface AnkiMedia {
    filename: string;
    /** Base64-encoded file contents. */
    data: string;
    /** Note fields that receive the media reference. */
    fields: string[];
}

export interface AnkiNote {
    deckName: string;
    modelName: string;
    fields: Record<string, string>;
    tags: string[];
    audio: AnkiMedia[];
    picture: AnkiMedia[];
}
