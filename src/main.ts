import './style.css';
import { tauriBackend } from './ipc/backend';
import { createApp } from './ui/app';

const root = document.getElementById('app');

if (root !== null) {
    void createApp(root, tauriBackend);
}
