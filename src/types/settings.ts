import type { Profile } from './profile';

/** What to do when the card already exists in the deck. */
export type DuplicatePolicy = 'skip' | 'allow';

/** The language of the meanings shown for a word (Portuguese falls back to English). */
export type DictionaryLanguage = 'en' | 'pt';

/** The settings about cards that can be changed while the app runs. */
export interface MiningSettings {
    defaultProfileId: string;
    profiles: Profile[];
    screenshotOnCards: boolean;
    audioOnCards: boolean;
    audioSeconds: number;
    duplicatePolicy: DuplicatePolicy;
    /** Whether closing the window keeps the app running in the system tray. */
    closeToTray: boolean;
    dictionaryLanguage: DictionaryLanguage;
}
