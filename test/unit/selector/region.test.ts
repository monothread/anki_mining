import { describe, expect, it } from 'vitest';
import { MIN_REGION_HEIGHT, MIN_REGION_WIDTH, regionFromDrag } from '../../../src/selector/region';

describe('regionFromDrag', () => {
    it('returns the rectangle between the two points', () => {
        expect(regionFromDrag({ x: 100, y: 800 }, { x: 900, y: 900 }, 1)).toEqual({
            x: 100,
            y: 800,
            width: 800,
            height: 100,
        });
    });

    it('accepts drags in any direction', () => {
        const expected = { x: 100, y: 800, width: 800, height: 100 };

        expect(regionFromDrag({ x: 900, y: 900 }, { x: 100, y: 800 }, 1)).toEqual(expected);
        expect(regionFromDrag({ x: 100, y: 900 }, { x: 900, y: 800 }, 1)).toEqual(expected);
        expect(regionFromDrag({ x: 900, y: 800 }, { x: 100, y: 900 }, 1)).toEqual(expected);
    });

    it('converts CSS pixels to physical pixels with the pixel ratio and rounds', () => {
        expect(regionFromDrag({ x: 10.5, y: 20.25 }, { x: 110.5, y: 70.25 }, 1.5)).toEqual({
            x: 16,
            y: 30,
            width: 150,
            height: 75,
        });
    });

    it('accepts exactly the minimum size', () => {
        expect(
            regionFromDrag({ x: 0, y: 0 }, { x: MIN_REGION_WIDTH, y: MIN_REGION_HEIGHT }, 1),
        ).toEqual({ x: 0, y: 0, width: MIN_REGION_WIDTH, height: MIN_REGION_HEIGHT });
    });

    it('rejects areas that are too narrow, too short or empty', () => {
        expect(regionFromDrag({ x: 0, y: 0 }, { x: MIN_REGION_WIDTH - 1, y: 100 }, 1)).toBeNull();
        expect(regionFromDrag({ x: 0, y: 0 }, { x: 500, y: MIN_REGION_HEIGHT - 1 }, 1)).toBeNull();
        expect(regionFromDrag({ x: 50, y: 50 }, { x: 50, y: 50 }, 1)).toBeNull();
    });

    it('measures the minimum size in physical pixels', () => {
        // 20 x 8 CSS pixels at ratio 2 is 40 x 16 physical pixels, which is large enough.
        expect(regionFromDrag({ x: 0, y: 0 }, { x: 20, y: 8 }, 2)).toEqual({
            x: 0,
            y: 0,
            width: 40,
            height: 16,
        });
    });
});
