import js from '@eslint/js';
import globals from 'globals';
import reactHooks from 'eslint-plugin-react-hooks';
import reactRefresh from 'eslint-plugin-react-refresh';
import tseslint from 'typescript-eslint';

export default tseslint.config(
  { ignores: ['dist', 'node_modules'] },
  {
    extends: [
      js.configs.recommended,
      ...tseslint.configs.strictTypeChecked,
      ...tseslint.configs.stylisticTypeChecked,
    ],
    files: ['**/*.{ts,tsx}'],
    languageOptions: {
      ecmaVersion: 2022,
      globals: globals.browser,
      parserOptions: {
        project: ['./tsconfig.json', './tsconfig.node.json'],
        tsconfigRootDir: import.meta.dirname,
      },
    },
    plugins: {
      'react-hooks': reactHooks,
      'react-refresh': reactRefresh,
    },
    rules: {
      ...reactHooks.configs.recommended.rules,
      'react-refresh/only-export-components': [
        'warn',
        { allowConstantExport: true },
      ],

      // Bans the recs-doc lists
      '@typescript-eslint/no-explicit-any': 'error',
      '@typescript-eslint/no-non-null-assertion': 'error',
      '@typescript-eslint/ban-ts-comment': [
        'error',
        {
          'ts-expect-error': 'allow-with-description',
          'ts-ignore': true,
          'ts-nocheck': true,
          'ts-check': false,
          minimumDescriptionLength: 10,
        },
      ],
      '@typescript-eslint/no-floating-promises': 'error',
      '@typescript-eslint/strict-boolean-expressions': 'error',
      'no-console': ['error', { allow: ['warn', 'error'] }],
      'no-warning-comments': [
        'error',
        { terms: ['todo', 'fixme', 'xxx', 'hack'], location: 'anywhere' },
      ],

      // Ban mock libraries from non-test paths
      'no-restricted-imports': [
        'error',
        {
          patterns: [
            {
              group: [
                '**/__mocks__/**',
                '**/*.mock',
                '**/*.mock.*',
                'vitest',
                '@testing-library/*',
              ],
              message:
                'mock libraries are only allowed in *.test.ts/*.spec.ts files',
            },
          ],
        },
      ],
    },
  },
  // Tests and test setup/config files can use mocks
  {
    files: [
      '**/*.test.{ts,tsx}',
      '**/*.spec.{ts,tsx}',
      'vitest.config.ts',
      'vitest.setup.ts',
    ],
    rules: {
      'no-restricted-imports': 'off',
      'no-console': 'off',
    },
  },
);
