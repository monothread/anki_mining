import type { Region } from '../types/region';

export interface Point {
    x: number;
    y: number;
}

/** Smallest area (in screen pixels) that can hold a line of text. */
export const MIN_REGION_WIDTH = 32;
export const MIN_REGION_HEIGHT = 12;

/**
 * Turns a drag between two points (in CSS pixels, any direction) into a screen region in
 * physical pixels. Returns null when the area is too small to hold text.
 */
export function regionFromDrag(start: Point, end: Point, pixelRatio: number): Region | null {
    const region: Region = {
        x: Math.round(Math.min(start.x, end.x) * pixelRatio),
        y: Math.round(Math.min(start.y, end.y) * pixelRatio),
        width: Math.round(Math.abs(end.x - start.x) * pixelRatio),
        height: Math.round(Math.abs(end.y - start.y) * pixelRatio),
    };
    if (region.width < MIN_REGION_WIDTH || region.height < MIN_REGION_HEIGHT) {
        return null;
    }
    return region;
}
