import type { Sentence } from '../types/sentence';
import type { Token } from '../types/token';

/** Part-of-speech values that are never worth looking up. */
const NON_WORD_POS: ReadonlySet<string> = new Set(['補助記号', '記号', '空白']);

export function isWordToken(token: Token): boolean {
    return !NON_WORD_POS.has(token.pos);
}

export interface SentenceViewHandlers {
    onTokenSelect: (token: Token) => void;
}

/** Draws the sentence; word tokens are interactive, everything else is plain text. */
export function renderSentence(
    container: HTMLElement,
    sentence: Sentence,
    handlers: SentenceViewHandlers,
): void {
    container.replaceChildren();
    let cursor = 0;

    for (const token of sentence.tokens) {
        if (token.start > cursor) {
            container.append(sentence.text.slice(cursor, token.start));
        }
        container.append(createTokenElement(sentence.text, token, handlers));
        cursor = token.end;
    }
    if (cursor < sentence.text.length) {
        container.append(sentence.text.slice(cursor));
    }
}

/** Highlights the token starting at `start`; `null` clears the highlight. */
export function markSelected(container: HTMLElement, start: number | null): void {
    for (const element of container.querySelectorAll<HTMLElement>('.token')) {
        element.classList.toggle('selected', element.dataset['start'] === String(start));
    }
}

function createTokenElement(
    text: string,
    token: Token,
    handlers: SentenceViewHandlers,
): HTMLElement {
    const element = document.createElement('span');
    element.textContent = text.slice(token.start, token.end);
    if (!isWordToken(token)) {
        return element;
    }

    element.className = 'token';
    element.dataset['start'] = String(token.start);
    const select = (): void => {
        handlers.onTokenSelect(token);
    };
    element.addEventListener('mouseenter', select);
    element.addEventListener('click', select);
    return element;
}
