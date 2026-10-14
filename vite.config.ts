import { defineConfig } from 'vite';

export default defineConfig({
    clearScreen: false,
    build: {
        outDir: 'web-dist',
        emptyOutDir: true,
        rollupOptions: {
            input: {
                main: 'index.html',
                selector: 'selector.html',
                popup: 'popup.html',
            },
        },
    },
    server: {
        port: 1420,
        strictPort: true,
    },
});
