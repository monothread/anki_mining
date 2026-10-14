import { vi } from 'vitest';
import type { Backend, Unsubscribe } from '../../src/ipc/backend';
import type { MineResult } from '../../src/types/mine';
import type { Region } from '../../src/types/region';
import type { Sentence } from '../../src/types/sentence';
import { CAT_RESULT, HOVER, STATUS } from './fixtures';

export interface FakeBackend {
    backend: Backend;
    emitSentence(sentence: Sentence): void;
    emitMineHotkey(): void;
    emitAppError(message: string): void;
    emitOcrRegion(region: Region | null): void;
    emitHoverInfo(message: string): void;
    emitMineResult(result: MineResult): void;
    unsubscribes: {
        sentence: ReturnType<typeof vi.fn<Unsubscribe>>;
        hotkey: ReturnType<typeof vi.fn<Unsubscribe>>;
        error: ReturnType<typeof vi.fn<Unsubscribe>>;
        region: ReturnType<typeof vi.fn<Unsubscribe>>;
        info: ReturnType<typeof vi.fn<Unsubscribe>>;
        mine: ReturnType<typeof vi.fn<Unsubscribe>>;
    };
}

/** A backend whose commands are spies with sensible defaults and whose events can be emitted. */
export function createFakeBackend(): FakeBackend {
    let sentenceHandler: (sentence: Sentence) => void = () => {
        return undefined;
    };
    let hotkeyHandler: () => void = () => {
        return undefined;
    };
    let errorHandler: (message: string) => void = () => {
        return undefined;
    };
    let regionHandler: (region: Region | null) => void = () => {
        return undefined;
    };
    let infoHandler: (message: string) => void = () => {
        return undefined;
    };
    let mineHandler: (result: MineResult) => void = () => {
        return undefined;
    };
    const unsubscribes = {
        sentence: vi.fn<Unsubscribe>(),
        hotkey: vi.fn<Unsubscribe>(),
        error: vi.fn<Unsubscribe>(),
        region: vi.fn<Unsubscribe>(),
        info: vi.fn<Unsubscribe>(),
        mine: vi.fn<Unsubscribe>(),
    };

    const backend: Backend = {
        getStatus: vi.fn<Backend['getStatus']>().mockResolvedValue(STATUS),
        getHover: vi.fn<Backend['getHover']>().mockResolvedValue(HOVER),
        setHoverMode: vi.fn<Backend['setHoverMode']>().mockImplementation((mode) => {
            return Promise.resolve({ ...HOVER, mode });
        }),
        setHoverEnabled: vi.fn<Backend['setHoverEnabled']>().mockImplementation((enabled) => {
            return Promise.resolve({ ...HOVER, enabled });
        }),
        scanAt: vi.fn<Backend['scanAt']>().mockResolvedValue([CAT_RESULT]),
        mine: vi.fn<Backend['mine']>().mockResolvedValue(1234),
        importDictionary: vi
            .fn<Backend['importDictionary']>()
            .mockResolvedValue({ entries: 218_898, terms: 499_430, skipped: 0 }),
        submitText: vi.fn<Backend['submitText']>().mockResolvedValue(undefined),
        selectOcrRegion: vi.fn<Backend['selectOcrRegion']>().mockResolvedValue(undefined),
        clearOcrRegion: vi.fn<Backend['clearOcrRegion']>().mockResolvedValue(undefined),
        onSentence: vi.fn<Backend['onSentence']>().mockImplementation((handler) => {
            sentenceHandler = handler;
            return Promise.resolve(unsubscribes.sentence);
        }),
        onMineHotkey: vi.fn<Backend['onMineHotkey']>().mockImplementation((handler) => {
            hotkeyHandler = handler;
            return Promise.resolve(unsubscribes.hotkey);
        }),
        onAppError: vi.fn<Backend['onAppError']>().mockImplementation((handler) => {
            errorHandler = handler;
            return Promise.resolve(unsubscribes.error);
        }),
        onMineResult: vi.fn<Backend['onMineResult']>().mockImplementation((handler) => {
            mineHandler = handler;
            return Promise.resolve(unsubscribes.mine);
        }),
        onHoverInfo: vi.fn<Backend['onHoverInfo']>().mockImplementation((handler) => {
            infoHandler = handler;
            return Promise.resolve(unsubscribes.info);
        }),
        onOcrRegion: vi.fn<Backend['onOcrRegion']>().mockImplementation((handler) => {
            regionHandler = handler;
            return Promise.resolve(unsubscribes.region);
        }),
    };

    return {
        backend,
        emitSentence: (sentence) => {
            sentenceHandler(sentence);
        },
        emitMineHotkey: () => {
            hotkeyHandler();
        },
        emitAppError: (message) => {
            errorHandler(message);
        },
        emitMineResult: (result) => {
            mineHandler(result);
        },
        emitHoverInfo: (message) => {
            infoHandler(message);
        },
        emitOcrRegion: (region) => {
            regionHandler(region);
        },
        unsubscribes,
    };
}
