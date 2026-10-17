/** Whether the word that starts at `start` already has a card in Anki. */
export interface KnownWord {
    start: number;
    known: boolean;
}

/** The words of a hover popup that already have a card, as the app tells the popup. */
export interface KnownWords {
    /** The sentence the answer is for, so a late answer is not applied to another sentence. */
    sentence: string;
    words: KnownWord[];
}

/** Underlines the word elements (found with `selector`, carrying `data-start`) by whether they are in Anki. */
export function markKnown(container: HTMLElement, selector: string, words: readonly KnownWord[]): void {
    const known = new Map<string, boolean>();
    for (const word of words) {
        known.set(String(word.start), word.known);
    }
    for (const element of container.querySelectorAll<HTMLElement>(selector)) {
        const start = element.dataset['start'];
        const flag = start === undefined ? undefined : known.get(start);
        element.classList.toggle('known', flag === true);
        element.classList.toggle('unknown', flag === false);
    }
}

/** Removes every underline. */
export function clearKnown(container: HTMLElement, selector: string): void {
    for (const element of container.querySelectorAll<HTMLElement>(selector)) {
        element.classList.remove('known', 'unknown');
    }
}

/** Underlines as known the word elements that start at one of `starts` (the others are left as they are). */
export function markAdded(container: HTMLElement, selector: string, starts: readonly number[]): void {
    const added = new Set(starts.map(String));
    for (const element of container.querySelectorAll<HTMLElement>(selector)) {
        const start = element.dataset['start'];
        if (start !== undefined && added.has(start)) {
            element.classList.add('known');
            element.classList.remove('unknown');
        }
    }
}
