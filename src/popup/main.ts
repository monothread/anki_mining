import './popup.css';
import { listen } from '@tauri-apps/api/event';
import type { HoverResult } from '../types/hover';
import { renderHoverResult } from './view';

const root = document.getElementById('popup');

if (root !== null) {
    void listen<HoverResult>('hover-result', (message) => {
        renderHoverResult(root, message.payload);
    });
}
