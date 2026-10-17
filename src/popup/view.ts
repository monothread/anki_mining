import type { HoverResult } from '../types/hover';
import type { DeinflectedEntry, ScanResult } from '../types/scan';
import type { LookupViewHandlers } from '../ui/lookupView';
import { renderLookup } from '../ui/lookupView';

export interface PopupHandlers {
    /** Called with the position of the chosen entry; without it entries have no "Add to Anki" button. */
    onMine?: (resultIndex: number, entryIndex: number) => void;
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
    sentence.append(result.sentence.slice(0, result.wordStart));
    const word = document.createElement('span');
    word.className = 'popup-word';
    word.textContent = result.sentence.slice(result.wordStart, wordEnd);
    sentence.append(word);
    sentence.append(result.sentence.slice(wordEnd));

    const lookup = document.createElement('div');
    lookup.className = 'lookup';
    const onMine = handlers.onMine;
    const lookupHandlers: LookupViewHandlers = {};
    if (onMine !== undefined) {
        lookupHandlers.onMine = (matched, found) => {
            onMine(...positionOf(result.results, matched, found));
        };
    }
    if (handlers.onSpeak !== undefined) {
        lookupHandlers.onSpeak = handlers.onSpeak;
    }
    renderLookup(lookup, result.results, lookupHandlers);
    root.append(sentence, lookup);
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
