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
}
