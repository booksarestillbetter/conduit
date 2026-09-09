// web/eslint.config.js
import js from '@eslint/js';
import tseslint from 'typescript-eslint';
import reactHooks from 'eslint-plugin-react-hooks';
import reactRefresh from 'eslint-plugin-react-refresh';
import globals from 'globals';

export default tseslint.config(
  { ignores: ['dist', 'node_modules'] },
  {
    extends: [js.configs.recommended, ...tseslint.configs.recommended],
    files: ['**/*.{ts,tsx}'],
    languageOptions: {
      ecmaVersion: 2022,
      globals: globals.browser,
    },
    plugins: {
      'react-hooks': reactHooks,
      'react-refresh': reactRefresh,
    },
    rules: {
      // Only the two classic, genuinely-correctness rules — eslint-plugin-react-hooks v7's
      // "recommended" preset now bundles the full React Compiler safety rule set (immutability,
      // purity, set-state-in-effect, etc.), which flags many long-standing, working patterns
      // already throughout this codebase as hard errors. Retrofitting a lint config onto an
      // existing app should establish a clean, useful baseline, not demand an unrelated
      // compiler-migration refactor before `npm run lint` can pass.
      'react-hooks/rules-of-hooks': 'error',
      'react-hooks/exhaustive-deps': 'warn',
      'react-refresh/only-export-components': ['warn', { allowConstantExport: true }],
      // Conduit's API layer types plenty of request/response shapes loosely (settings blobs,
      // session objects) on purpose — flag new `any` usage as a warning to catch drift, not
      // an error that would block an otherwise-correct change.
      '@typescript-eslint/no-explicit-any': 'warn',
      '@typescript-eslint/no-unused-vars': ['warn', { argsIgnorePattern: '^_', varsIgnorePattern: '^_' }],
    },
  }
);
