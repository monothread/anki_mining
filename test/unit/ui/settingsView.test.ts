import { beforeEach, describe, expect, it, vi } from 'vitest';
import { DEFAULT_FIELD_MAPPING } from '../../../src/types/profile';
import type { MiningSettings } from '../../../src/types/settings';
import {
    newProfile,
    nextProfileId,
    readSettings,
    renderSettings,
} from '../../../src/ui/settingsView';
import { SETTINGS } from '../../support/fixtures';

function mount(settings: MiningSettings = SETTINGS) {
    const root = document.createElement('div');
    document.body.replaceChildren(root);
    const onSave = vi.fn<(changed: MiningSettings) => void>();
    const onCancel = vi.fn<() => void>();
    renderSettings(root, settings, { onSave, onCancel });
    return { root, onSave, onCancel };
}

function field(root: ParentNode, name: string): HTMLInputElement {
    const found = root.querySelector(`input[name="${name}"]`);
    if (!(found instanceof HTMLInputElement)) {
        throw new Error(`no input ${name}`);
    }
    return found;
}

function type(input: HTMLInputElement, value: string): void {
    input.value = value;
}

function click(root: ParentNode, selector: string): void {
    const found = root.querySelector(selector);
    if (!(found instanceof HTMLElement)) {
        throw new Error(`nothing matches ${selector}`);
    }
    found.click();
}

const TWO_PROFILES: MiningSettings = {
    ...SETTINGS,
    profiles: [
        ...SETTINGS.profiles,
        {
            id: 'player',
            name: 'Player',
            deckName: 'Anime',
            modelName: 'Basic',
            fieldMapping: { ...DEFAULT_FIELD_MAPPING, expression: 'Front' },
            match: { sourceId: 'screen', appName: 'mpv', windowTitle: 'episode' },
        },
    ],
};

describe('renderSettings', () => {
    beforeEach(() => {
        document.body.replaceChildren();
    });

    it('shows the card options', () => {
        const { root } = mount();

        expect(field(root, 'screenshotOnCards').checked).toBe(true);
        expect(field(root, 'audioOnCards').checked).toBe(true);
        expect(field(root, 'audioSeconds').value).toBe('8');
        expect(field(root, 'closeToTray').checked).toBe(false);
        expect(root.querySelector<HTMLSelectElement>('select[name="duplicatePolicy"]')?.value).toBe('skip');
        expect(root.querySelector<HTMLSelectElement>('select[name="defaultProfileId"]')?.value).toBe('default');
    });

    it('shows every profile with its deck, note type, rules and field mapping', () => {
        const { root } = mount(TWO_PROFILES);

        const cards = root.querySelectorAll<HTMLElement>('.profile');
        expect(cards).toHaveLength(2);
        const player = cards[1];
        expect(player?.dataset['profileId']).toBe('player');
        expect(field(player ?? root, 'deckName').value).toBe('Anime');
        expect(field(player ?? root, 'modelName').value).toBe('Basic');
        expect(field(player ?? root, 'appName').value).toBe('mpv');
        expect(field(player ?? root, 'windowTitle').value).toBe('episode');
        expect(field(player ?? root, 'sourceId').value).toBe('screen');
        expect(field(player ?? root, 'map-expression').value).toBe('Front');
        expect(field(player ?? root, 'map-sentenceAudio').value).toBe('SentenceAudio');
    });

    it('does not let the only profile be removed', () => {
        const { root } = mount();

        expect(root.querySelector<HTMLButtonElement>('.remove-profile')?.disabled).toBe(true);
    });

    it('reports the settings that were typed when saving', () => {
        const { root, onSave } = mount(TWO_PROFILES);
        const player = root.querySelectorAll<HTMLElement>('.profile')[1] ?? root;
        type(field(player, 'deckName'), ' Games ');
        type(field(player, 'appName'), '');
        type(field(player, 'map-picture'), 'Image');
        type(field(root, 'audioSeconds'), '12');
        field(root, 'screenshotOnCards').checked = false;
        field(root, 'closeToTray').checked = true;
        const policy = root.querySelector<HTMLSelectElement>('select[name="duplicatePolicy"]');
        if (policy) {
            policy.value = 'allow';
        }

        click(root, '.save-settings');

        expect(onSave).toHaveBeenCalledTimes(1);
        expect(onSave).toHaveBeenCalledWith({
            defaultProfileId: 'default',
            profiles: [
                TWO_PROFILES.profiles[0],
                {
                    id: 'player',
                    name: 'Player',
                    deckName: 'Games',
                    modelName: 'Basic',
                    fieldMapping: { ...DEFAULT_FIELD_MAPPING, expression: 'Front', picture: 'Image' },
                    match: { sourceId: 'screen', appName: null, windowTitle: 'episode' },
                },
            ],
            screenshotOnCards: false,
            audioOnCards: true,
            audioSeconds: 12,
            duplicatePolicy: 'allow',
            closeToTray: true,
        });
    });

    it('falls back to one second when the clip length is not a number', () => {
        const { root } = mount();
        type(field(root, 'audioSeconds'), '');

        expect(readSettings(root).audioSeconds).toBe(1);
    });

    it('adds a profile with the default note type and keeps what was typed', () => {
        const { root, onSave } = mount();
        type(field(root, 'deckName'), 'Edited');

        click(root, '.add-profile');
        click(root, '.save-settings');

        const saved = onSave.mock.calls[0]?.[0];
        expect(saved?.profiles).toHaveLength(2);
        expect(saved?.profiles[0]?.deckName).toBe('Edited');
        expect(saved?.profiles[1]).toEqual({
            id: 'profile-2',
            name: 'Profile 2',
            deckName: 'Mining',
            modelName: 'AnkiMining',
            fieldMapping: DEFAULT_FIELD_MAPPING,
            match: { sourceId: null, appName: null, windowTitle: null },
        });
    });

    it('removes a profile and falls back to another default when the default is removed', () => {
        const { root, onSave } = mount(TWO_PROFILES);

        click(root, '.profile[data-profile-id="default"] .remove-profile');
        click(root, '.save-settings');

        const saved = onSave.mock.calls[0]?.[0];
        expect(saved?.profiles.map((profile) => {
            return profile.id;
        })).toEqual(['player']);
        expect(saved?.defaultProfileId).toBe('player');
    });

    it('keeps the default profile when another one is removed', () => {
        const { root, onSave } = mount(TWO_PROFILES);

        click(root, '.profile[data-profile-id="player"] .remove-profile');
        click(root, '.save-settings');

        const saved = onSave.mock.calls[0]?.[0];
        expect(saved?.defaultProfileId).toBe('default');
        expect(saved?.profiles).toHaveLength(1);
    });

    it('calls onCancel when closed', () => {
        const { root, onCancel, onSave } = mount();

        click(root, '.cancel-settings');

        expect(onCancel).toHaveBeenCalledTimes(1);
        expect(onSave).not.toHaveBeenCalled();
    });
});

describe('profile ids', () => {
    it('picks the first free number', () => {
        expect(nextProfileId([])).toBe('profile-1');
        expect(nextProfileId(SETTINGS.profiles)).toBe('profile-2');
        const taken = [
            ...SETTINGS.profiles,
            { ...newProfile(SETTINGS.profiles), id: 'profile-2' },
        ];
        expect(nextProfileId(taken)).toBe('profile-3');
    });
});
