export interface Token {
    surface: string;
    lemma: string;
    reading: string;
    pos: string;
    /** UTF-16 code unit offset (inclusive), usable directly with String.prototype.slice. */
    start: number;
    /** UTF-16 code unit offset (exclusive). */
    end: number;
}
