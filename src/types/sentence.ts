import type { Token } from './token';
import type { WindowInfo } from './textSource';

/** A cleaned and tokenized sentence pushed by the backend. */
export interface Sentence {
    text: string;
    tokens: Token[];
    sourceId: string;
    windowInfo: WindowInfo | null;
    timestamp: number;
}
