import type { DeinflectedEntry, ScanResult } from '../types/scan';

export interface LookupViewHandlers {
    /** When absent, entries are shown without an "Add to Anki" button. */
    onMine?: (result: ScanResult, found: DeinflectedEntry) => void;
}

/** Joins inflections as a chain, innermost first: `past < polite`. */
export function formatReasons(reasons: readonly string[]): string {
    return reasons.join(' < ');
}

export function renderLookup(
    container: HTMLElement,
    results: readonly ScanResult[],
    handlers: LookupViewHandlers,
): void {
    container.replaceChildren();
    if (results.length === 0) {
        container.append(createElement('p', 'empty', 'No dictionary entry'));
        return;
    }

    for (const result of results) {
        const section = createElement('section', 'match');
        section.append(createElement('h3', 'match-source', result.source));
        for (const found of result.entries) {
            section.append(createEntryElement(result, found, handlers));
        }
        container.append(section);
    }
}

function createEntryElement(
    result: ScanResult,
    found: DeinflectedEntry,
    handlers: LookupViewHandlers,
): HTMLElement {
    const element = createElement('article', 'entry');
    const heading = createElement('div', 'entry-heading');
    heading.append(createElement('span', 'expression', found.entry.expression));
    heading.append(createElement('span', 'reading', found.entry.reading));
    if (found.reasons.length > 0) {
        heading.append(createElement('span', 'reasons', formatReasons([...found.reasons].reverse())));
    }
    element.append(heading);

    if (found.entry.partsOfSpeech.length > 0) {
        element.append(createElement('div', 'pos', found.entry.partsOfSpeech.join(', ')));
    }
    const meanings = createElement('ol', 'meanings');
    for (const meaning of found.entry.meanings) {
        meanings.append(createElement('li', '', meaning));
    }
    element.append(meanings);

    const onMine = handlers.onMine;
    if (onMine !== undefined) {
        const button = createElement('button', 'mine', 'Add to Anki');
        button.type = 'button';
        button.addEventListener('click', () => {
            onMine(result, found);
        });
        element.append(button);
    }
    return element;
}

function createElement<K extends keyof HTMLElementTagNameMap>(
    tag: K,
    className: string,
    text?: string,
): HTMLElementTagNameMap[K] {
    const element = document.createElement(tag);
    if (className !== '') {
        element.className = className;
    }
    if (text !== undefined) {
        element.textContent = text;
    }
    return element;
}
