import type { HoverResult, HoverStatus } from '../../src/types/hover';
import type { DeinflectedEntry, ScanResult } from '../../src/types/scan';
import type { Sentence } from '../../src/types/sentence';
import type { Status } from '../../src/types/status';

export const SENTENCE: Sentence = {
    text: '猫が好きです。',
    tokens: [
        { surface: '猫', lemma: '猫', reading: 'ネコ', pos: '名詞', start: 0, end: 1 },
        { surface: 'が', lemma: 'が', reading: 'ガ', pos: '助詞', start: 1, end: 2 },
        { surface: '好き', lemma: '好き', reading: 'スキ', pos: '形状詞', start: 2, end: 4 },
        { surface: 'です', lemma: 'です', reading: 'デス', pos: '助動詞', start: 4, end: 6 },
        { surface: '。', lemma: '。', reading: '', pos: '補助記号', start: 6, end: 7 },
    ],
    sourceId: 'clipboard',
    windowInfo: { title: 'Player', appName: 'mpv' },
    timestamp: 1_760_400_000_000,
};

export const CAT_ENTRY: DeinflectedEntry = {
    entry: {
        expression: '猫',
        reading: 'ねこ',
        meanings: ['cat', 'feline'],
        partsOfSpeech: ['n'],
        frequency: null,
    },
    source: '猫',
    reasons: [],
};

export const EAT_ENTRY: DeinflectedEntry = {
    entry: {
        expression: '食べる',
        reading: 'たべる',
        meanings: ['to eat'],
        partsOfSpeech: ['v1', 'vt'],
        frequency: 120,
    },
    source: '食べました',
    reasons: ['past', 'polite'],
};

export const CAT_RESULT: ScanResult = { source: '猫', length: 1, entries: [CAT_ENTRY] };

export const EAT_RESULT: ScanResult = { source: '食べました', length: 5, entries: [EAT_ENTRY] };

export const STATUS: Status = {
    dictionaryEntries: 218_898,
    warnings: [],
    miningHotkey: 'Alt+M',
    websocketPort: 6677,
    defaultProfileId: 'default',
    ocrAvailable: true,
    ocrRegion: null,
};

export const HOVER: HoverStatus = { enabled: true, mode: 'hold', key: 'shift', dwellMs: 300 };

export const HOVER_RESULT: HoverResult = {
    sentence: '昨日、友達と映画を見に行きました。',
    wordStart: 3,
    results: [
        {
            source: '友達',
            length: 2,
            entries: [
                {
                    entry: {
                        expression: '友達',
                        reading: 'ともだち',
                        meanings: ['friend'],
                        partsOfSpeech: ['n'],
                        frequency: null,
                    },
                    source: '友達',
                    reasons: [],
                },
            ],
        },
    ],
    charBbox: [160, 700, 200, 750],
    lineBbox: [40, 700, 760, 750],
    vertical: false,
};
