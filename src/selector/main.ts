import './selector.css';
import { tauriSelectorBackend } from '../ipc/selector';
import { createSelector } from './view';

const root = document.getElementById('selector');

if (root !== null) {
    createSelector(root, tauriSelectorBackend, window.devicePixelRatio, (message) => {
        const hint = root.querySelector('.hint');
        if (hint !== null) {
            hint.textContent = message;
        }
    });
}
