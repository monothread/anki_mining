import type { CardInput } from '../types/card';
import type { DeinflectedEntry, ScanResult } from '../types/scan';
import type { Sentence } from '../types/sentence';

/** Describes where a sentence came from, e.g. `clipboard - mpv`. */
export function describeSource(sentence: Sentence): string {
    const parts = [sentence.sourceId];
    const title = sentence.windowInfo?.title;
    if (title !== undefined && title !== '') {
        parts.push(title);
    }
    return parts.join(' - ');
}

/** Builds the mining request for `entry`, highlighting the matched text starting at `start`. */
export function buildCardInput(
    sentence: Sentence,
    start: number,
    result: ScanResult,
    found: DeinflectedEntry,
): CardInput {
    return {
        entry: found.entry,
        sentence: sentence.text,
        highlight: [start, start + result.length],
        source: describeSource(sentence),
        audio: null,
        picture: null,
    };
}
