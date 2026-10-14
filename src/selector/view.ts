import type { SelectorBackend } from '../ipc/selector';
import { regionFromDrag } from './region';
import type { Point } from './region';

const TEMPLATE = `
<p class="hint">Drag over the area with the subtitles - Esc cancels</p>
<div id="box" class="box" hidden></div>
`;

function pointOf(event: MouseEvent): Point {
    return { x: event.clientX, y: event.clientY };
}

/** Lets the user drag a rectangle over the screen and reports it to the backend. */
export function createSelector(
    root: HTMLElement,
    backend: SelectorBackend,
    pixelRatio: number,
    onError: (message: string) => void,
): void {
    root.innerHTML = TEMPLATE;
    const box = root.querySelector<HTMLElement>('#box');
    if (box === null) {
        throw new Error('Missing element #box');
    }

    let start: Point | null = null;

    const draw = (from: Point, to: Point): void => {
        box.hidden = false;
        box.style.left = `${String(Math.min(from.x, to.x))}px`;
        box.style.top = `${String(Math.min(from.y, to.y))}px`;
        box.style.width = `${String(Math.abs(to.x - from.x))}px`;
        box.style.height = `${String(Math.abs(to.y - from.y))}px`;
    };

    const cancel = (): void => {
        start = null;
        box.hidden = true;
        void backend.cancel().catch((error: unknown) => {
            onError(String(error));
        });
    };

    root.addEventListener('mousedown', (event) => {
        if (event.button !== 0) {
            cancel();
            return;
        }
        start = pointOf(event);
        draw(start, start);
    });

    root.addEventListener('mousemove', (event) => {
        if (start !== null) {
            draw(start, pointOf(event));
        }
    });

    root.addEventListener('mouseup', (event) => {
        if (start === null) {
            return;
        }
        const region = regionFromDrag(start, pointOf(event), pixelRatio);
        start = null;
        box.hidden = true;
        if (region === null) {
            return;
        }
        void backend.select(region).catch((error: unknown) => {
            onError(String(error));
        });
    });

    document.addEventListener('keydown', (event) => {
        if (event.key === 'Escape') {
            cancel();
        }
    });
}
