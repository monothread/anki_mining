import { DEFAULT_FIELD_MAPPING, MINING_FIELDS } from '../types/profile';
import type { MiningField, Profile } from '../types/profile';
import type { DuplicatePolicy, MiningSettings } from '../types/settings';

export interface SettingsHandlers {
    onSave: (settings: MiningSettings) => void;
    onCancel: () => void;
    /** Called with the path of a Yomitan dictionary zip to take pitch accents and frequencies from. */
    onImportYomitan: (path: string) => void;
}

export const MIN_AUDIO_SECONDS = 1;
export const MAX_AUDIO_SECONDS = 30;

const FIELD_LABELS: Record<MiningField, string> = {
    expression: 'Expression',
    reading: 'Reading',
    meaning: 'Meaning',
    sentence: 'Sentence',
    sentenceAudio: 'Sentence audio',
    picture: 'Picture',
    frequency: 'Frequency',
    pitch: 'Pitch',
    source: 'Source',
    wordAudio: 'Word audio (voice)',
};

/** The id for a new profile: `profile-2`, `profile-3`, ... the first one nobody uses. */
export function nextProfileId(profiles: readonly Profile[]): string {
    const used = new Set(profiles.map((profile) => {
        return profile.id;
    }));
    let number = profiles.length + 1;
    while (used.has(`profile-${String(number)}`)) {
        number += 1;
    }
    return `profile-${String(number)}`;
}

/** A profile with the default note type that matches nothing, to be filled in by the user. */
export function newProfile(profiles: readonly Profile[]): Profile {
    const id = nextProfileId(profiles);
    return {
        id,
        name: `Profile ${id.slice('profile-'.length)}`,
        deckName: 'Mining',
        modelName: 'AnkiMining',
        fieldMapping: { ...DEFAULT_FIELD_MAPPING },
        match: { sourceId: null, appName: null, windowTitle: null },
    };
}

function element<K extends keyof HTMLElementTagNameMap>(
    tag: K,
    className?: string,
    text?: string,
): HTMLElementTagNameMap[K] {
    const created = document.createElement(tag);
    if (className !== undefined) {
        created.className = className;
    }
    if (text !== undefined) {
        created.textContent = text;
    }
    return created;
}

function labelled(label: string, input: HTMLElement, hint?: string): HTMLLabelElement {
    const wrapper = element('label', 'setting');
    wrapper.append(element('span', 'setting-label', label), input);
    if (hint !== undefined) {
        wrapper.title = hint;
    }
    return wrapper;
}

function textInput(name: string, value: string, placeholder = ''): HTMLInputElement {
    const input = element('input');
    input.type = 'text';
    input.name = name;
    input.value = value;
    input.placeholder = placeholder;
    input.autocomplete = 'off';
    return input;
}

function checkbox(name: string, checked: boolean): HTMLInputElement {
    const input = element('input');
    input.type = 'checkbox';
    input.name = name;
    input.checked = checked;
    return input;
}

function optional(value: string): string | null {
    const trimmed = value.trim();
    return trimmed === '' ? null : trimmed;
}

function profileCard(profile: Profile, removable: boolean): HTMLElement {
    const card = element('fieldset', 'profile');
    card.dataset['profileId'] = profile.id;
    card.append(element('legend', undefined, profile.id));
    card.append(
        labelled('Name', textInput('name', profile.name)),
        labelled('Deck', textInput('deckName', profile.deckName)),
        labelled('Note type', textInput('modelName', profile.modelName)),
        labelled('Application', textInput('appName', profile.match.appName ?? '', 'e.g. mpv'), 'The window class of the program, any case'),
        labelled('Window title contains', textInput('windowTitle', profile.match.windowTitle ?? '', 'e.g. YouTube')),
        labelled('Source', textInput('sourceId', profile.match.sourceId ?? '', 'e.g. screen'), 'screen, mpv, clipboard, websocket...'),
    );
    const details = element('details', 'mapping');
    details.append(element('summary', undefined, 'Field mapping'));
    for (const field of MINING_FIELDS) {
        details.append(labelled(FIELD_LABELS[field], textInput(`map-${field}`, profile.fieldMapping[field])));
    }
    card.append(details);
    const remove = element('button', 'action remove-profile', 'Remove profile');
    remove.type = 'button';
    remove.disabled = !removable;
    card.append(remove);
    return card;
}

/** Reads the settings out of the form that `renderSettings` built. */
export function readSettings(root: HTMLElement): MiningSettings {
    const input = (scope: ParentNode, name: string): HTMLInputElement => {
        const found = scope.querySelector(`[name="${name}"]`);
        if (!(found instanceof HTMLInputElement)) {
            throw new Error(`Missing field ${name}`);
        }
        return found;
    };
    const select = (name: string): HTMLSelectElement => {
        const found = root.querySelector(`select[name="${name}"]`);
        if (!(found instanceof HTMLSelectElement)) {
            throw new Error(`Missing field ${name}`);
        }
        return found;
    };

    const profiles: Profile[] = [...root.querySelectorAll<HTMLElement>('.profile')].map((card) => {
        const fieldMapping = { ...DEFAULT_FIELD_MAPPING };
        for (const field of MINING_FIELDS) {
            fieldMapping[field] = input(card, `map-${field}`).value.trim();
        }
        return {
            id: card.dataset['profileId'] ?? '',
            name: input(card, 'name').value.trim(),
            deckName: input(card, 'deckName').value.trim(),
            modelName: input(card, 'modelName').value.trim(),
            fieldMapping,
            match: {
                sourceId: optional(input(card, 'sourceId').value),
                appName: optional(input(card, 'appName').value),
                windowTitle: optional(input(card, 'windowTitle').value),
            },
        };
    });
    const seconds = Number.parseInt(input(root, 'audioSeconds').value, 10);
    return {
        defaultProfileId: select('defaultProfileId').value,
        profiles,
        screenshotOnCards: input(root, 'screenshotOnCards').checked,
        audioOnCards: input(root, 'audioOnCards').checked,
        audioSeconds: Number.isNaN(seconds) ? MIN_AUDIO_SECONDS : seconds,
        duplicatePolicy: select('duplicatePolicy').value === 'allow' ? 'allow' : 'skip',
        closeToTray: input(root, 'closeToTray').checked,
        ttsOnCards: input(root, 'ttsOnCards').checked,
    };
}

function defaultProfileSelect(settings: MiningSettings): HTMLSelectElement {
    const select = element('select');
    select.name = 'defaultProfileId';
    for (const profile of settings.profiles) {
        const option = element('option', undefined, profile.name === '' ? profile.id : profile.name);
        option.value = profile.id;
        option.selected = profile.id === settings.defaultProfileId;
        select.append(option);
    }
    return select;
}

function duplicateSelect(policy: DuplicatePolicy): HTMLSelectElement {
    const select = element('select');
    select.name = 'duplicatePolicy';
    const choices: [DuplicatePolicy, string][] = [
        ['skip', 'Skip (do not add the card again)'],
        ['allow', 'Allow (add it again)'],
    ];
    for (const [value, label] of choices) {
        const option = element('option', undefined, label);
        option.value = value;
        option.selected = value === policy;
        select.append(option);
    }
    return select;
}

function audioSecondsInput(seconds: number): HTMLInputElement {
    const input = element('input');
    input.type = 'number';
    input.name = 'audioSeconds';
    input.min = String(MIN_AUDIO_SECONDS);
    input.max = String(MAX_AUDIO_SECONDS);
    input.value = String(seconds);
    return input;
}

/** Draws the settings form inside `root`. Adding or removing a profile redraws it. */
export function renderSettings(root: HTMLElement, settings: MiningSettings, handlers: SettingsHandlers): void {
    root.replaceChildren();

    const general = element('fieldset', 'general');
    general.append(
        element('legend', undefined, 'Cards'),
        labelled('Screenshot', checkbox('screenshotOnCards', settings.screenshotOnCards)),
        labelled('Sentence audio', checkbox('audioOnCards', settings.audioOnCards), 'Needs parec (PulseAudio or PipeWire)'),
        labelled('Spoken word and sentence (voice)', checkbox('ttsOnCards', settings.ttsOnCards), 'A synthesized Japanese voice reads the word and, when there is no recording, the sentence (VOICEVOX:春日部つむぎ)'),
        labelled('Longest audio clip (seconds)', audioSecondsInput(settings.audioSeconds)),
        labelled('When the card already exists', duplicateSelect(settings.duplicatePolicy)),
        labelled('Default profile', defaultProfileSelect(settings)),
    );
    root.append(general);

    const dictionary = element('fieldset', 'dictionary');
    const yomitanPath = textInput('yomitanPath', '', '/path/to/dictionary.zip');
    const importButton = element('button', 'action import-yomitan', 'Import');
    importButton.type = 'button';
    importButton.addEventListener('click', () => {
        const path = yomitanPath.value.trim();
        if (path !== '') {
            handlers.onImportYomitan(path);
        }
    });
    const importRow = element('div', 'import-row');
    importRow.append(yomitanPath, importButton);
    dictionary.append(
        element('legend', undefined, 'Dictionary'),
        labelled('Pitch accent or frequency dictionary (Yomitan .zip)', importRow, 'Adds its pitch accents and frequencies to the bundled dictionary'),
    );
    root.append(dictionary);

    const application = element('fieldset', 'application');
    application.append(
        element('legend', undefined, 'Application'),
        labelled('Keep running in the system tray when the window is closed', checkbox('closeToTray', settings.closeToTray)),
    );
    root.append(application);

    const removable = settings.profiles.length > 1;
    for (const profile of settings.profiles) {
        root.append(profileCard(profile, removable));
    }

    const redraw = (change: (current: MiningSettings) => MiningSettings): void => {
        renderSettings(root, change(readSettings(root)), handlers);
    };

    const add = element('button', 'action add-profile', 'Add profile');
    add.type = 'button';
    add.addEventListener('click', () => {
        redraw((current) => {
            return { ...current, profiles: [...current.profiles, newProfile(current.profiles)] };
        });
    });

    for (const button of root.querySelectorAll<HTMLButtonElement>('.remove-profile')) {
        button.addEventListener('click', () => {
            const id = button.closest<HTMLElement>('.profile')?.dataset['profileId'];
            redraw((current) => {
                const profiles = current.profiles.filter((profile) => {
                    return profile.id !== id;
                });
                const defaultProfileId = profiles.some((profile) => {
                    return profile.id === current.defaultProfileId;
                })
                    ? current.defaultProfileId
                    : (profiles[0]?.id ?? current.defaultProfileId);
                return { ...current, profiles, defaultProfileId };
            });
        });
    }

    const buttons = element('div', 'settings-buttons');
    const save = element('button', 'action save-settings', 'Save');
    save.type = 'button';
    save.addEventListener('click', () => {
        handlers.onSave(readSettings(root));
    });
    const cancel = element('button', 'action cancel-settings', 'Close');
    cancel.type = 'button';
    cancel.addEventListener('click', () => {
        handlers.onCancel();
    });
    buttons.append(add, save, cancel);
    root.append(buttons);
}
