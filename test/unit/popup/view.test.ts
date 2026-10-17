import { describe, expect, it, vi } from 'vitest';
import { renderHoverResult } from '../../../src/popup/view';
import type { HoverResult } from '../../../src/types/hover';
import { HOVER_RESULT } from '../../support/fixtures';

function render(result: HoverResult = HOVER_RESULT): HTMLElement {
    const root = document.createElement('div');
    renderHoverResult(root, result);
    return root;
}

describe('renderHoverResult', () => {
    it('shows the sentence with the longest match marked as the word', () => {
        const root = render();

        expect(root.querySelector('.popup-sentence')?.textContent).toBe('昨日、友達と映画を見に行きました。');
        expect(root.querySelector('.popup-word')?.textContent).toBe('友達');
    });

    it('keeps the text before and after the word', () => {
        const sentence = render().querySelector('.popup-sentence');

        expect(sentence?.childNodes[0]?.textContent).toBe('昨日、');
        expect(sentence?.childNodes[2]?.textContent).toBe('と映画を見に行きました。');
    });

    it('shows the dictionary entry with reading and meaning but without an "Add to Anki" button', () => {
        const root = render();

        expect(root.querySelector('.expression')?.textContent).toBe('友達');
        expect(root.querySelector('.reading')?.textContent).toBe('ともだち');
        expect(root.querySelector('.meanings li')?.textContent).toBe('friend');
        expect(root.querySelector('button')).toBeNull();
    });

    it('marks the word at the start and at the end of the sentence', () => {
        const atStart = render({ ...HOVER_RESULT, sentence: '友達と', wordStart: 0 });
        const atEnd = render({ ...HOVER_RESULT, sentence: 'と友達', wordStart: 1 });

        expect(atStart.querySelector('.popup-word')?.textContent).toBe('友達');
        expect(atStart.querySelector('.popup-sentence')?.childNodes[0]?.textContent).toBe('');
        expect(atEnd.querySelector('.popup-word')?.textContent).toBe('友達');
        expect(atEnd.querySelector('.popup-sentence')?.childNodes[2]?.textContent).toBe('');
    });

    it('shows no word mark when there are no matches', () => {
        const root = render({ ...HOVER_RESULT, results: [] });

        expect(root.querySelector('.popup-word')?.textContent).toBe('');
        expect(root.querySelector('.empty')?.textContent).toBe('No dictionary entry');
    });

    it('replaces the previous content when rendering again', () => {
        const root = render();

        renderHoverResult(root, { ...HOVER_RESULT, sentence: '犬', wordStart: 0, results: [] });

        expect(root.querySelectorAll('.popup-sentence')).toHaveLength(1);
        expect(root.querySelector('.popup-sentence')?.textContent).toBe('犬');
    });

    it('does not interpret markup in the recognized text', () => {
        const root = render({ ...HOVER_RESULT, sentence: '<img src=x onerror=alert(1)>', wordStart: 0, results: [] });

        expect(root.querySelector('img')).toBeNull();
    });

    it('adds an "Add to Anki" button per entry when a mining handler is given', () => {
        const root = document.createElement('div');

        renderHoverResult(root, HOVER_RESULT, { onMine: vi.fn<(r: number, e: number) => void>() });

        const buttons = root.querySelectorAll<HTMLButtonElement>('button.mine');
        expect(buttons).toHaveLength(1);
        expect(buttons[0]?.textContent).toBe('Add to Anki');
    });

    it('reports the position of the chosen entry', () => {
        const root = document.createElement('div');
        const onMine = vi.fn<(r: number, e: number) => void>();
        const second = {
            ...HOVER_RESULT.results[0],
            source: '友',
            length: 1,
            entries: [
                { ...HOVER_RESULT.results[0]?.entries[0], source: '友' },
                { ...HOVER_RESULT.results[0]?.entries[0], source: '友 (2)' },
            ],
        } as unknown as HoverResult['results'][number];
        const result: HoverResult = { ...HOVER_RESULT, results: [HOVER_RESULT.results[0], second] as HoverResult['results'] };

        renderHoverResult(root, result, { onMine });
        const buttons = root.querySelectorAll<HTMLButtonElement>('button.mine');
        buttons[2]?.click();
        buttons[0]?.click();

        expect(buttons).toHaveLength(3);
        expect(onMine).toHaveBeenNthCalledWith(1, 1, 1);
        expect(onMine).toHaveBeenNthCalledWith(2, 0, 0);
    });

    it('makes every word of the sentence an element that can be underlined, and not the punctuation', () => {
        const root = render();

        const words = [...root.querySelectorAll<HTMLElement>('.popup-token')].map((token) => {
            return [token.textContent, token.dataset['start']];
        });
        expect(words).toEqual([
            ['昨日', '0'],
            ['友達', '3'],
            ['と', '5'],
            ['映画', '6'],
        ]);
        expect(root.querySelector('.popup-word .popup-token')?.textContent).toBe('友達');
    });

    it('cuts a word where the marked match ends inside it', () => {
        const root = render({
            ...HOVER_RESULT,
            sentence: '日本語',
            wordStart: 0,
            results: [{ ...(HOVER_RESULT.results[0] as (typeof HOVER_RESULT.results)[number]), length: 2 }],
            tokens: [{ surface: '日本語', lemma: '日本語', reading: 'ニホンゴ', pos: '名詞', start: 0, end: 3 }],
        });

        expect(root.querySelector('.popup-sentence')?.textContent).toBe('日本語');
        expect(root.querySelector('.popup-word')?.textContent).toBe('日本');
        const pieces = [...root.querySelectorAll<HTMLElement>('.popup-token')].map((token) => {
            return [token.textContent, token.dataset['start']];
        });
        expect(pieces).toEqual([
            ['日本', '0'],
            ['語', '0'],
        ]);
    });
});

