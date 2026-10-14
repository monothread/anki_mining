import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Sentence } from '../../../src/types/sentence';
import type { Token } from '../../../src/types/token';
import { isWordToken, markSelected, renderSentence } from '../../../src/ui/sentenceView';
import { SENTENCE } from '../../support/fixtures';

function token(pos: string): Token {
    return { surface: 'x', lemma: 'x', reading: '', pos, start: 0, end: 1 };
}

function render(sentence: Sentence = SENTENCE): {
    container: HTMLElement;
    onTokenSelect: ReturnType<typeof vi.fn<(token: Token) => void>>;
} {
    const container = document.createElement('div');
    const onTokenSelect = vi.fn<(token: Token) => void>();
    renderSentence(container, sentence, { onTokenSelect });
    return { container, onTokenSelect };
}

describe('isWordToken', () => {
    it('rejects punctuation, symbols and whitespace', () => {
        expect(isWordToken(token('補助記号'))).toBe(false);
        expect(isWordToken(token('記号'))).toBe(false);
        expect(isWordToken(token('空白'))).toBe(false);
    });

    it('accepts words', () => {
        for (const pos of ['名詞', '動詞', '形容詞', '助詞', '助動詞', '未知語']) {
            expect(isWordToken(token(pos))).toBe(true);
        }
    });
});

describe('renderSentence', () => {
    beforeEach(() => {
        document.body.replaceChildren();
    });

    it('renders every token in order and preserves the sentence text', () => {
        const { container } = render();

        expect(container.textContent).toBe('猫が好きです。');
        expect(Array.from(container.children).map((element) => {return element.textContent})).toEqual([
            '猫',
            'が',
            '好き',
            'です',
            '。',
        ]);
    });

    it('makes word tokens interactive and leaves punctuation as plain spans', () => {
        const { container } = render();

        const interactive = Array.from(container.querySelectorAll<HTMLElement>('.token'));

        expect(interactive.map((element) => {return element.textContent})).toEqual(['猫', 'が', '好き', 'です']);
        expect(interactive.map((element) => {return element.dataset['start']})).toEqual(['0', '1', '2', '4']);
        expect(container.lastElementChild?.className).toBe('');
        expect(container.lastElementChild?.getAttribute('data-start')).toBeNull();
    });

    it('keeps text that no token covers, before, between and after tokens', () => {
        const sentence: Sentence = {
            ...SENTENCE,
            text: '「猫 が」好き',
            tokens: [
                { surface: '猫', lemma: '猫', reading: 'ネコ', pos: '名詞', start: 1, end: 2 },
                { surface: 'が', lemma: 'が', reading: 'ガ', pos: '助詞', start: 3, end: 4 },
            ],
        };

        const { container } = render(sentence);

        expect(container.textContent).toBe('「猫 が」好き');
        expect(container.querySelectorAll('.token')).toHaveLength(2);
    });

    it('renders plain text when there are no tokens', () => {
        const { container } = render({ ...SENTENCE, tokens: [] });

        expect(container.textContent).toBe('猫が好きです。');
        expect(container.querySelectorAll('.token')).toHaveLength(0);
    });

    it('calls onTokenSelect with the exact token on hover and on click', () => {
        const { container, onTokenSelect } = render();
        const word = container.querySelectorAll<HTMLElement>('.token')[2];

        word?.dispatchEvent(new MouseEvent('mouseenter'));
        word?.dispatchEvent(new MouseEvent('click', { bubbles: true }));

        const expected = SENTENCE.tokens[2];
        expect(onTokenSelect).toHaveBeenCalledTimes(2);
        expect(onTokenSelect).toHaveBeenNthCalledWith(1, expected);
        expect(onTokenSelect).toHaveBeenNthCalledWith(2, expected);
    });

    it('does not react to hovering punctuation', () => {
        const { container, onTokenSelect } = render();

        container.lastElementChild?.dispatchEvent(new MouseEvent('mouseenter'));
        container.lastElementChild?.dispatchEvent(new MouseEvent('click', { bubbles: true }));

        expect(onTokenSelect).not.toHaveBeenCalled();
    });

    it('replaces the previous content when rendering again', () => {
        const container = document.createElement('div');
        const handlers = { onTokenSelect: vi.fn<(token: Token) => void>() };

        renderSentence(container, SENTENCE, handlers);
        renderSentence(container, { ...SENTENCE, text: '犬', tokens: [] }, handlers);

        expect(container.textContent).toBe('犬');
    });

    it('does not interpret markup in the text', () => {
        const sentence: Sentence = { ...SENTENCE, text: '<b>猫</b>', tokens: [] };

        const { container } = render(sentence);

        expect(container.querySelector('b')).toBeNull();
        expect(container.textContent).toBe('<b>猫</b>');
    });
});

describe('markSelected', () => {
    it('marks only the token starting at the given offset', () => {
        const { container } = render();

        markSelected(container, 2);

        const selected = Array.from(container.querySelectorAll('.selected'));
        expect(selected.map((element) => {return element.textContent})).toEqual(['好き']);
    });

    it('moves the mark when another token is selected', () => {
        const { container } = render();

        markSelected(container, 0);
        markSelected(container, 4);

        const selected = Array.from(container.querySelectorAll('.selected'));
        expect(selected.map((element) => {return element.textContent})).toEqual(['です']);
    });

    it('clears the mark with null or an unknown offset', () => {
        const { container } = render();
        markSelected(container, 0);

        markSelected(container, null);
        expect(container.querySelectorAll('.selected')).toHaveLength(0);

        markSelected(container, 0);
        markSelected(container, 99);
        expect(container.querySelectorAll('.selected')).toHaveLength(0);
    });
});
