import { describe, expect, it } from 'vitest';
import { buildCardInput, describeSource } from '../../../src/cards/cardInput';
import type { Sentence } from '../../../src/types/sentence';
import { CAT_ENTRY, EAT_ENTRY, EAT_RESULT, SENTENCE } from '../../support/fixtures';

describe('describeSource', () => {
    it('joins the source id and the window title', () => {
        expect(describeSource(SENTENCE)).toBe('clipboard - Player');
    });

    it('uses only the source id when there is no window info', () => {
        const sentence: Sentence = { ...SENTENCE, windowInfo: null };

        expect(describeSource(sentence)).toBe('clipboard');
    });

    it('ignores an empty window title', () => {
        const sentence: Sentence = { ...SENTENCE, windowInfo: { title: '', appName: null } };

        expect(describeSource(sentence)).toBe('clipboard');
    });
});

describe('buildCardInput', () => {
    it('highlights the matched text starting at the token and carries the entry and source', () => {
        const sentence: Sentence = { ...SENTENCE, text: '昨日食べました' };

        const input = buildCardInput(sentence, 2, EAT_RESULT, EAT_ENTRY);

        expect(input).toEqual({
            entry: {
                expression: '食べる',
                reading: 'たべる',
                meanings: ['to eat'],
                partsOfSpeech: ['v1', 'vt'],
                frequency: 120,
                pitch: [],
            },
            sentence: '昨日食べました',
            highlight: [2, 7],
            source: 'clipboard - Player',
            audio: null,
            picture: null,
            wordAudio: null,
        });
    });

    it('uses the length of the scan result, not of the entry expression', () => {
        const input = buildCardInput(
            SENTENCE,
            0,
            { source: '猫', length: 1, entries: [CAT_ENTRY] },
            CAT_ENTRY,
        );

        expect(input.highlight).toEqual([0, 1]);
        expect(input.entry.expression).toBe('猫');
    });
});
