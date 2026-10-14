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
    defaultProfileId: string;
    profiles: Profile[];
}
