import { describe, expect, it } from 'vitest';
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
});
