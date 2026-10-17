import type { DeinflectedEntry, ScanResult } from '../types/scan';
import { pitchLabel, pitchPattern, splitMoras } from './pitch';

export interface LookupViewHandlers {
    /** When absent, entries are shown without an "Add to Anki" button. */
    /**
     * When it returns a promise the button shows that the card is being added and then, when the
     * promise is true, that it was added.
     */
    onMine?: (result: ScanResult, found: DeinflectedEntry) => Promise<boolean> | undefined;
    /** When absent, entries have no listen button. Called with the reading of the entry. */
    onSpeak?: (reading: string) => void;
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

const MINE_LABEL = 'Add to Anki';

type MineState = 'idle' | 'loading' | 'done';

/** The look of the "Add to Anki" button: ready, adding (spinner) or added. */
function setMineState(button: HTMLButtonElement, state: MineState): void {
    button.classList.toggle('loading', state === 'loading');
    button.classList.toggle('done', state === 'done');
    button.disabled = state !== 'idle';
    button.replaceChildren();
    if (state === 'loading') {
        button.append(createElement('span', 'spinner'), 'Adding\u2026');
    } else {
        button.textContent = state === 'done' ? 'Added \u2713' : MINE_LABEL;
    }
    button.setAttribute('aria-busy', String(state === 'loading'));
}

function createEntryElement(
    result: ScanResult,
    found: DeinflectedEntry,
    handlers: LookupViewHandlers,
): HTMLElement {
    const element = createElement('article', 'entry');
    const heading = createElement('div', 'entry-heading');
    heading.append(createElement('span', 'expression', found.entry.expression));
    heading.append(createReadingElement(found.entry.reading, found.entry.pitch));
    const onSpeak = handlers.onSpeak;
    if (onSpeak !== undefined) {
        const listen = createElement('button', 'speak', '\u25B6');
        listen.type = 'button';
        listen.title = 'Listen';
        listen.setAttribute('aria-label', `Listen to ${found.entry.expression}`);
        listen.addEventListener('click', () => {
            onSpeak(found.entry.reading);
        });
        heading.append(listen);
    }
    if (found.entry.pitch.length > 0) {
        heading.append(createElement('span', 'pitch', pitchLabel(found.entry.pitch)));
    }
    if (found.entry.frequency !== null) {
        const rank = createElement('span', 'frequency', `#${String(found.entry.frequency)}`);
        rank.title = 'Frequency rank (lower is more common)';
        heading.append(rank);
    }
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
        const button = createElement('button', 'mine', MINE_LABEL);
        button.type = 'button';
        button.addEventListener('click', () => {
            if (button.disabled) {
                return;
            }
            const outcome = onMine(result, found);
            if (outcome === undefined) {
                return;
            }
            setMineState(button, 'loading');
            void outcome
                .then((added) => {
                    setMineState(button, added ? 'done' : 'idle');
                })
                .catch(() => {
                    setMineState(button, 'idle');
                });
        });
        element.append(button);
    }
    return element;
}

/** The reading; with a known pitch accent every mora is marked high or low (the first accent is drawn). */
function createReadingElement(reading: string, pitch: readonly number[]): HTMLElement {
    const element = createElement('span', 'reading');
    const first = pitch[0];
    if (first === undefined) {
        element.textContent = reading;
        return element;
    }
    const moras = splitMoras(reading);
    const pattern = pitchPattern(moras.length, first);
    moras.forEach((mora, index) => {
        const part = createElement('span', pattern[index] === true ? 'mora high' : 'mora', mora);
        if (pattern[index] === true && pattern[index + 1] === false) {
            part.classList.add('drop');
        }
        element.append(part);
    });
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
