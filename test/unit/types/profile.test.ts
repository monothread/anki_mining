import { describe, expect, it } from 'vitest';
import { DEFAULT_FIELD_MAPPING, MINING_FIELDS } from '../../../src/types/profile';
import type { Profile } from '../../../src/types/profile';

describe('MINING_FIELDS', () => {
    it('lists the mining fields in the same order as the Rust MiningField enum', () => {
        expect(MINING_FIELDS).toEqual([
            'expression',
            'reading',
            'meaning',
            'sentence',
            'sentenceAudio',
            'picture',
            'frequency',
            'pitch',
            'source',
        ]);
    });
});

describe('DEFAULT_FIELD_MAPPING', () => {
    it('maps every mining field to the default AnkiMining note type field name', () => {
        expect(DEFAULT_FIELD_MAPPING).toEqual({
            expression: 'Expression',
            reading: 'Reading',
            meaning: 'Meaning',
            sentence: 'Sentence',
            sentenceAudio: 'SentenceAudio',
            picture: 'Picture',
            frequency: 'Frequency',
            pitch: 'Pitch',
            source: 'Source',
        });
    });

    it('has exactly one entry per mining field', () => {
        expect(Object.keys(DEFAULT_FIELD_MAPPING)).toEqual([...MINING_FIELDS]);
    });

    it('can be used as the field mapping of a profile', () => {
        const profile: Profile = {
            id: 'youtube',
            name: 'YouTube',
            deckName: 'Youtube',
            modelName: 'AnkiMining',
            fieldMapping: DEFAULT_FIELD_MAPPING,
            match: { sourceId: 'browser-extension', appName: null },
        };

        expect(profile.fieldMapping.sentenceAudio).toBe('SentenceAudio');
    });
});
