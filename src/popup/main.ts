import './popup.css';
import { tauriPopupBackend } from '../ipc/popup';
import { createPopup } from './popup';

const root = document.getElementById('popup');

if (root !== null) {
    void createPopup(root, tauriPopupBackend);
}
