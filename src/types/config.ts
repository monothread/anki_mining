import type { Profile } from './profile';

export interface Config {
    ankiConnectUrl: string;
    websocketPort: number;
    miningHotkey: string;
    defaultProfileId: string;
    profiles: Profile[];
}
