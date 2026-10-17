/** A newer version of the app that can be installed. */
export interface UpdateInfo {
    version: string;
    currentVersion: string;
    /** What changed, as written in the release; null when there is no text. */
    notes: string | null;
}
