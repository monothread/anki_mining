import type { HoverResult } from '../types/hover';
import type { DeinflectedEntry, ScanResult } from '../types/scan';
import type { LookupViewHandlers } from '../ui/lookupView';
import { renderLookup } from '../ui/lookupView';
import { isWordToken } from '../ui/sentenceView';

export interface PopupHandlers {
    /** Called with the position of the chosen entry; without it entries have no "Add to Anki" button. */
    onMine?: (resultIndex: number, entryIndex: number) => Promise<boolean> | undefined;
    /** Called with the reading of an entry whose listen button was clicked. */
    onSpeak?: (reading: string) => void;
}

/** Shows the sentence with the looked-up word marked, followed by the dictionary entries. */
export function renderHoverResult(
    root: HTMLElement,
    result: HoverResult,
    handlers: PopupHandlers = {},
): void {
    root.replaceChildren();
    const longest = result.results[0]?.length ?? 0;
    const wordEnd = result.wordStart + longest;

    const sentence = document.createElement('p');
    sentence.className = 'popup-sentence';
    const before = document.createElement('span');
    appendText(before, result, 0, result.wordStart);
    const word = document.createElement('span');
    word.className = 'popup-word';
    appendText(word, result, result.wordStart, wordEnd);
    const after = document.createElement('span');
    appendText(after, result, wordEnd, result.sentence.length);
    sentence.append(before, word, after);

    const lookup = document.createElement('div');
    lookup.className = 'lookup';
    const onMine = handlers.onMine;
    const lookupHandlers: LookupViewHandlers = {};
    if (onMine !== undefined) {
        lookupHandlers.onMine = (matched, found) => {
            return onMine(...positionOf(result.results, matched, found));
        };
    }
    if (handlers.onSpeak !== undefined) {
        lookupHandlers.onSpeak = handlers.onSpeak;
    }
    renderLookup(lookup, result.results, lookupHandlers);
    root.append(sentence, lookup);
}

/**
 * Adds the part `[from, to)` of the sentence to `container`: every word of it as an element that
 * `markKnown` can underline (cut where the part starts or ends), the rest as plain text.
 */
function appendText(container: HTMLElement, result: HoverResult, from: number, to: number): void {
    let cursor = from;
    for (const token of result.tokens) {
        const start = Math.max(token.start, from);
        const end = Math.min(token.end, to);
        if (end <= start || !isWordToken(token)) {
            continue;
        }
        if (start > cursor) {
            container.append(result.sentence.slice(cursor, start));
        }
        const piece = document.createElement('span');
        piece.className = 'popup-token';
        piece.dataset['start'] = String(token.start);
        piece.textContent = result.sentence.slice(start, end);
        container.append(piece);
        cursor = end;
    }
    if (cursor < to) {
        container.append(result.sentence.slice(cursor, to));
    }
}

/** The index of the scan result and of the entry inside it. */
function positionOf(
    results: readonly ScanResult[],
    matched: ScanResult,
    found: DeinflectedEntry,
): [number, number] {
    const resultIndex = results.indexOf(matched);
    return [resultIndex, matched.entries.indexOf(found)];
}
