import type { HoverResult } from '../types/hover';
import { renderLookup } from '../ui/lookupView';

/** Shows the sentence with the looked-up word marked, followed by the dictionary entries. */
export function renderHoverResult(root: HTMLElement, result: HoverResult): void {
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
    renderLookup(lookup, result.results, {});
    root.append(sentence, lookup);
}
