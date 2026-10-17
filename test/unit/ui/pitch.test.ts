import { describe, expect, it } from 'vitest';
import { pitchLabel, pitchPattern, splitMoras } from '../../../src/ui/pitch';

describe('splitMoras', () => {
    it('gives one mora per kana', () => {
        expect(splitMoras('ねこ')).toEqual(['ね', 'こ']);
    });

    it('joins small kana to the kana before them', () => {
        expect(splitMoras('きょう')).toEqual(['きょ', 'う']);
        expect(splitMoras('シャツ')).toEqual(['シャ', 'ツ']);
        expect(splitMoras('ファイル')).toEqual(['ファ', 'イ', 'ル']);
    });

    it('keeps the small tsu and the long vowel mark as moras of their own', () => {
        expect(splitMoras('がっこう')).toEqual(['が', 'っ', 'こ', 'う']);
        expect(splitMoras('コーヒー')).toEqual(['コ', 'ー', 'ヒ', 'ー']);
    });

    it('handles an empty reading and a leading small kana', () => {
        expect(splitMoras('')).toEqual([]);
        expect(splitMoras('ゃ')).toEqual(['ゃ']);
    });
});

describe('pitchPattern', () => {
    it('is low then high for a flat word', () => {
        expect(pitchPattern(3, 0)).toEqual([false, true, true]);
    });

    it('is high then low when the pitch drops after the first mora', () => {
        expect(pitchPattern(3, 1)).toEqual([true, false, false]);
    });

    it('rises after the first mora and drops after the given one', () => {
        expect(pitchPattern(4, 2)).toEqual([false, true, false, false]);
        expect(pitchPattern(4, 3)).toEqual([false, true, true, false]);
        expect(pitchPattern(4, 4)).toEqual([false, true, true, true]);
    });

    it('has no moras for an empty reading and a single mora is high or low by its accent', () => {
        expect(pitchPattern(0, 0)).toEqual([]);
        expect(pitchPattern(1, 0)).toEqual([false]);
        expect(pitchPattern(1, 1)).toEqual([true]);
    });
});

describe('pitchLabel', () => {
    it('lists the accents in brackets', () => {
        expect(pitchLabel([1, 0])).toBe('[1], [0]');
        expect(pitchLabel([3])).toBe('[3]');
        expect(pitchLabel([])).toBe('');
    });
});
