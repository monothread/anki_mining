import type { Profile } from './profile';

/** What to do when the card already exists in the deck. */
export type DuplicatePolicy = 'skip' | 'allow';

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
    /** Whether cards get the word and the sentence read by a synthesized voice. */
    ttsOnCards: boolean;
    /** Whether the app looks for a newer version a little after it starts. */
    autoUpdate: boolean;
}
