import { describe, expect, it } from 'vitest';
import { clearKnown, markAdded, markKnown } from '../../../src/ui/known';

function words(): HTMLElement {
    const root = document.createElement('div');
    root.innerHTML =
        '<span class="token" data-start="0">猫</span>' +
        '<span class="token" data-start="1">が</span>' +
        '<span class="token" data-start="2">好き</span>' +
        '<span class="token">no start</span>';
    return root;
}

function classes(root: HTMLElement): string[] {
    return [...root.querySelectorAll('.token')].map((element) => {
        return element.className;
    });
}

describe('markKnown', () => {
    it('marks each word as known or unknown by where it starts', () => {
        const root = words();

        markKnown(root, '.token', [
            { start: 0, known: true },
            { start: 2, known: false },
        ]);

        expect(classes(root)).toEqual(['token known', 'token', 'token unknown', 'token']);
    });

    it('replaces the marks of an earlier answer', () => {
        const root = words();
        markKnown(root, '.token', [{ start: 0, known: true }]);

        markKnown(root, '.token', [{ start: 0, known: false }]);

        expect(classes(root)[0]).toBe('token unknown');
    });

    it('does nothing for an empty answer and clearKnown removes every mark', () => {
        const root = words();
        markKnown(root, '.token', []);
        expect(classes(root)).toEqual(['token', 'token', 'token', 'token']);

        markKnown(root, '.token', [{ start: 1, known: true }]);
        clearKnown(root, '.token');
        expect(classes(root)).toEqual(['token', 'token', 'token', 'token']);
    });
});

describe('markAdded', () => {
    it('marks only the given words as known and leaves the other marks alone', () => {
        const root = words();
        markKnown(root, '.token', [
            { start: 0, known: false },
            { start: 1, known: true },
        ]);

        markAdded(root, '.token', [0, 2]);

        expect(classes(root)).toEqual(['token known', 'token known', 'token known', 'token']);
    });

    it('does nothing for words that are not there', () => {
        const root = words();

        markAdded(root, '.token', [9]);
        markAdded(root, '.token', []);

        expect(classes(root)).toEqual(['token', 'token', 'token', 'token']);
    });
});

