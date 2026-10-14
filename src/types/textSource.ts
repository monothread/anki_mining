export interface WindowInfo {
    title: string;
    appName: string | null;
}

export interface TextSourceEvent {
    text: string;
    sourceId: string;
    /** Unix epoch in milliseconds. */
    timestamp: number;
    windowInfo: WindowInfo | null;
}
