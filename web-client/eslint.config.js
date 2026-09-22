import js from '@eslint/js'
import tseslint from '@typescript-eslint/eslint-plugin'
import tsparser from '@typescript-eslint/parser'
import reactHooks from 'eslint-plugin-react-hooks'
import react from 'eslint-plugin-react'
import jsxA11y from 'eslint-plugin-jsx-a11y'

export default [
  js.configs.recommended,
  {
    files: ['**/*.{ts,tsx}'],
    languageOptions: {
      parser: tsparser,
      parserOptions: {
        ecmaVersion: 'latest',
        sourceType: 'module',
        ecmaFeatures: { jsx: true },
      },
      globals: {
        // Vitest / Node globals
        console: 'readonly',
        globalThis: 'readonly',
        React: 'readonly',
        // Browser globals
        AbortController: 'readonly',
        AbortSignal: 'readonly',
        Blob: 'readonly',
        document: 'readonly',
        File: 'readonly',
        FileList: 'readonly',
        fetch: 'readonly',
        FormData: 'readonly',
        HTMLDivElement: 'readonly',
        HTMLInputElement: 'readonly',
        HTMLTextAreaElement: 'readonly',
        IntersectionObserver: 'readonly',
        KeyboardEvent: 'readonly',
        localStorage: 'readonly',
        navigator: 'readonly',
        RequestInit: 'readonly',
        ResizeObserver: 'readonly',
        Response: 'readonly',
        setInterval: 'readonly',
        clearInterval: 'readonly',
        TextDecoder: 'readonly',
        URL: 'readonly',
        window: 'readonly',
      },
    },
    plugins: {
      '@typescript-eslint': tseslint,
      'react-hooks': reactHooks,
      react,
      'jsx-a11y': jsxA11y,
    },
    rules: {
      // Disable base no-unused-vars so @typescript-eslint/no-unused-vars controls it
      'no-unused-vars': 'off',
      ...reactHooks.configs.recommended.rules,
      'react-hooks/set-state-in-effect': 'off',
      'react/jsx-uses-react': 'off',
      'react/react-in-jsx-scope': 'off',
      'react/prop-types': 'off',
      '@typescript-eslint/no-unused-vars': [
        'error',
        { argsIgnorePattern: '^_', varsIgnorePattern: '^_', caughtErrorsIgnorePattern: '^_' },
      ],
      '@typescript-eslint/no-explicit-any': 'warn',
      'react/display-name': 'off',
    },
    settings: {
      react: { version: 'detect' },
    },
  },
  {
    files: ['**/*.tsx'],
    rules: {
      'jsx-a11y/no-static-element-interactions': 'warn',
      'jsx-a11y/click-events-have-key-events': 'warn',
    },
  },
  {
    ignores: ['src/mocks/**', 'src/vite-env.d.ts', 'src/vitest.d.ts'],
  },
]
