import eslint from '@eslint/js';
import tseslint from 'typescript-eslint';

export default tseslint.config(
    { ignores: ['web-dist/', 'dist/', 'node_modules/', '.cache/', 'src-tauri/', 'eslint.config.js'] },
    eslint.configs.recommended,
    ...tseslint.configs.strictTypeChecked,
    {
        languageOptions: {
            parserOptions: {
                projectService: true,
                tsconfigRootDir: import.meta.dirname,
            },
        },
        rules: {
            'arrow-body-style': ['error', 'always'],
            curly: ['error', 'all'],
            '@typescript-eslint/no-explicit-any': 'error',
        },
    },
);
