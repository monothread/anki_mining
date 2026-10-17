import { describe, expect, it, vi } from 'vitest';
import type { DeinflectedEntry, ScanResult } from '../../../src/types/scan';
import { formatReasons, renderLookup } from '../../../src/ui/lookupView';
import { CAT_ENTRY, CAT_RESULT, EAT_RESULT } from '../../support/fixtures';

type MineHandler = (result: ScanResult, found: DeinflectedEntry) => void;

function render(results: ScanResult[]): {
    container: HTMLElement;
    onMine: ReturnType<typeof vi.fn<MineHandler>>;
} {
    const container = document.createElement('div');
    const onMine = vi.fn<MineHandler>();
    renderLookup(container, results, { onMine });
    return { container, onMine };
}

describe('formatReasons', () => {
    it('joins the inflections with a chain separator', () => {
        expect(formatReasons(['polite', 'past'])).toBe('polite < past');
    });

    it('returns an empty string without reasons and the reason alone when single', () => {
        expect(formatReasons([])).toBe('');
        expect(formatReasons(['past'])).toBe('past');
    });
});

describe('renderLookup', () => {
    it('shows a message when there are no results', () => {
        const { container } = render([]);

        expect(container.querySelector('.empty')?.textContent).toBe('No dictionary entry');
        expect(container.querySelector('.entry')).toBeNull();
    });

    it('renders the expression, reading, parts of speech and meanings of an entry', () => {
        const { container } = render([CAT_RESULT]);

        expect(container.querySelector('.match-source')?.textContent).toBe('猫');
        expect(container.querySelector('.expression')?.textContent).toBe('猫');
        expect(container.querySelector('.reading')?.textContent).toBe('ねこ');
        expect(container.querySelector('.pos')?.textContent).toBe('n');
        expect(
            Array.from(container.querySelectorAll('.meanings li')).map((item) => {return item.textContent}),
        ).toEqual(['cat', 'feline']);
        expect(container.querySelector('.reasons')).toBeNull();
    });

    it('shows the inflection chain from the dictionary form outwards', () => {
        const { container } = render([EAT_RESULT]);

        expect(container.querySelector('.expression')?.textContent).toBe('食べる');
        expect(container.querySelector('.reasons')?.textContent).toBe('polite < past');
        expect(container.querySelector('.pos')?.textContent).toBe('v1, vt');
    });

    it('omits the parts of speech line when there are none', () => {
        const entry: DeinflectedEntry = {
            ...CAT_ENTRY,
            entry: { ...CAT_ENTRY.entry, partsOfSpeech: [] },
        };

        const { container } = render([{ ...CAT_RESULT, entries: [entry] }]);

        expect(container.querySelector('.pos')).toBeNull();
    });

    it('renders one section per scan result in the given order, each with its entries', () => {
        const { container } = render([EAT_RESULT, CAT_RESULT]);

        const sections = Array.from(container.querySelectorAll('.match'));
        expect(sections.map((section) => {return section.querySelector('.match-source')?.textContent})).toEqual([
            '食べました',
            '猫',
        ]);
        expect(container.querySelectorAll('.entry')).toHaveLength(2);
    });

    it('calls onMine with the result and entry of the clicked button', () => {
        const { container, onMine } = render([EAT_RESULT, CAT_RESULT]);

        const buttons = container.querySelectorAll<HTMLButtonElement>('button.mine');
        buttons[1]?.click();

        expect(buttons).toHaveLength(2);
        expect(buttons[0]?.textContent).toBe('Add to Anki');
        expect(buttons[0]?.type).toBe('button');
        expect(onMine).toHaveBeenCalledTimes(1);
        expect(onMine).toHaveBeenCalledWith(CAT_RESULT, CAT_ENTRY);
    });

    it('reports the right entry when a result has several', () => {
        const second: DeinflectedEntry = {
            ...CAT_ENTRY,
            entry: { ...CAT_ENTRY.entry, expression: 'ネコ', reading: 'ネコ' },
        };
        const result: ScanResult = { ...CAT_RESULT, entries: [CAT_ENTRY, second] };
        const { container, onMine } = render([result]);

        container.querySelectorAll<HTMLButtonElement>('button.mine')[1]?.click();

        expect(onMine).toHaveBeenCalledWith(result, second);
    });

    it('does not interpret markup in dictionary data', () => {
        const entry: DeinflectedEntry = {
            ...CAT_ENTRY,
            entry: { ...CAT_ENTRY.entry, meanings: ['<img src=x onerror=alert(1)>'] },
        };

        const { container } = render([{ ...CAT_RESULT, entries: [entry] }]);

        expect(container.querySelector('img')).toBeNull();
        expect(container.querySelector('.meanings li')?.textContent).toBe(
            '<img src=x onerror=alert(1)>',
        );
    });

    it('replaces the previous content', () => {
        const container = document.createElement('div');
        const handlers = { onMine: vi.fn<MineHandler>() };

        renderLookup(container, [CAT_RESULT], handlers);
        renderLookup(container, [], handlers);

        expect(container.querySelector('.entry')).toBeNull();
        expect(container.querySelector('.empty')).not.toBeNull();
    });

    it('shows no "Add to Anki" button when there is no mining handler', () => {
        const container = document.createElement('div');

        renderLookup(container, [CAT_RESULT], {});

        expect(container.querySelector('.entry')).not.toBeNull();
        expect(container.querySelector('button')).toBeNull();
    });
});

describe('renderLookup pitch accent and frequency', () => {
    const withMetadata = (pitch: number[], frequency: number | null): ScanResult => {
        return {
            ...CAT_RESULT,
            entries: [{ ...CAT_ENTRY, entry: { ...CAT_ENTRY.entry, pitch, frequency } }],
        };
    };

    it('shows the plain reading when the pitch is unknown', () => {
        const { container } = render([withMetadata([], null)]);

        expect(container.querySelector('.reading')?.textContent).toBe('ねこ');
        expect(container.querySelector('.mora')).toBeNull();
        expect(container.querySelector('.pitch')).toBeNull();
        expect(container.querySelector('.frequency')).toBeNull();
    });

    it('marks the high moras of the first accent and shows every accent', () => {
        const { container } = render([withMetadata([1, 0], null)]);

        const moras = [...container.querySelectorAll('.mora')];
        expect(moras.map((mora) => {
            return mora.textContent;
        })).toEqual(['ね', 'こ']);
        expect(moras.map((mora) => {
            return mora.classList.contains('high');
        })).toEqual([true, false]);
        expect(moras.map((mora) => {
            return mora.classList.contains('drop');
        })).toEqual([true, false]);
        expect(container.querySelector('.pitch')?.textContent).toBe('[1], [0]');
    });

    it('does not mark a drop for a flat word', () => {
        const { container } = render([withMetadata([0], null)]);

        const moras = [...container.querySelectorAll('.mora')];
        expect(moras.map((mora) => {
            return mora.classList.contains('high');
        })).toEqual([false, true]);
        expect(container.querySelector('.drop')).toBeNull();
    });

    it('shows the frequency rank', () => {
        const { container } = render([withMetadata([], 1520)]);

        const rank = container.querySelector('.frequency');
        expect(rank?.textContent).toBe('#1520');
        expect(rank?.getAttribute('title')).toBe('Frequency rank (lower is more common)');
    });
});

describe('renderLookup listen buttons', () => {
    it('has no listen button unless a handler is given', () => {
        const { container } = render([CAT_RESULT]);

        expect(container.querySelector('.speak')).toBeNull();
    });

    it('has one listen button per entry that reads the reading of that entry', () => {
        const container = document.createElement('div');
        const onSpeak = vi.fn<(reading: string) => void>();
        renderLookup(container, [CAT_RESULT, EAT_RESULT], { onSpeak });

        const buttons = container.querySelectorAll<HTMLButtonElement>('.speak');
        buttons[1]?.click();
        buttons[0]?.click();

        expect(buttons).toHaveLength(2);
        expect(onSpeak.mock.calls).toEqual([['たべる'], ['ねこ']]);
        expect(buttons[0]?.getAttribute('aria-label')).toBe('Listen to 猫');
        expect(buttons[0]?.title).toBe('Listen');
    });
});

