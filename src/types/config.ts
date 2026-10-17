import type { TriggerKey, TriggerMode } from './hover';
import type { Profile } from './profile';

export interface Config {
    ankiConnectUrl: string;
    websocketPort: number;
    miningHotkey: string;
    /** How the lookup under the mouse cursor starts. */
    hoverMode: TriggerMode;
    /** The key to hold in `hold` mode. */
    hoverKey: TriggerKey;
    /** How long the cursor must rest in `auto` mode, in milliseconds. */
    hoverDwellMs: number;
    /** Whether the lookup under the mouse cursor is on when the app starts. */
    hoverEnabled: boolean;
    /** Whether cards get a screenshot of the window the word was found in. */
    screenshotOnCards: boolean;
    /** Whether cards get the sentence audio (needs `parec`). */
    audioOnCards: boolean;
    /** The longest audio clip put on a card, in seconds. */
    audioSeconds: number;
    /** What to do when the card already exists in the deck. */
    duplicatePolicy: 'skip' | 'allow';
    /** Whether closing the window keeps the app running in the system tray. */
    closeToTray: boolean;
    defaultProfileId: string;
    profiles: Profile[];
}
