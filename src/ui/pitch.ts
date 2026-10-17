const SMALL_KANA = new Set('ゃゅょぁぃぅぇぉゎャュョァィゥェォヮ');

/** Splits a kana reading into moras: a small kana joins the kana before it, `っ` and `ー` stand alone. */
export function splitMoras(reading: string): string[] {
    const moras: string[] = [];
    for (const character of reading) {
        const last = moras.length - 1;
        if (SMALL_KANA.has(character) && last >= 0) {
            moras[last] = `${moras[last] ?? ''}${character}`;
        } else {
            moras.push(character);
        }
    }
    return moras;
}

/** Whether each mora is pronounced high for an accent whose pitch drops after mora `position` (0: no drop). */
export function pitchPattern(moraCount: number, position: number): boolean[] {
    const pattern: boolean[] = [];
    for (let index = 0; index < moraCount; index += 1) {
        const mora = index + 1;
        if (position === 0) {
            pattern.push(index > 0);
        } else if (position === 1) {
            pattern.push(index === 0);
        } else {
            pattern.push(index > 0 && mora <= position);
        }
    }
    return pattern;
}

/** The accents as `[1], [0]`, the way they are written on cards. */
export function pitchLabel(positions: readonly number[]): string {
    return positions
        .map((position) => {
            return `[${String(position)}]`;
        })
        .join(', ');
}
