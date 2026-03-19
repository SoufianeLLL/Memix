/** @type {import('tailwindcss').Config} */
module.exports = {
  content: [
    "./src/panel/**/*.ts",
    "./src/panel/*.ts",
    "./src/webview/**/*.ts",
  ],
  theme: {
    extend: {
      colors: {
        // VS Code theme variables
        'vscode-editor-bg': 'var(--vscode-editor-background)',
        'vscode-editor-fg': 'var(--vscode-editor-foreground)',
        'vscode-sidebar-bg': 'var(--vscode-sideBar-background)',
        'vscode-button-bg': 'var(--vscode-button-background)',
        'vscode-button-hover': 'var(--vscode-button-hoverBackground)',
        'vscode-button-fg': 'var(--vscode-button-foreground)',
        'vscode-badge-bg': 'var(--vscode-badge-background)',
        'vscode-badge-fg': 'var(--vscode-badge-foreground)',
        'vscode-description': 'var(--vscode-descriptionForeground)',
        'vscode-panel-border': 'var(--vscode-panel-border)',
        'vscode-input-bg': 'var(--vscode-input-background)',
        'vscode-input-fg': 'var(--vscode-input-foreground)',
        'vscode-input-border': 'var(--vscode-input-border)',
        'success': '#4ec9b0',
        'warning': '#cca700',
        'danger': '#f44747',
      },
      fontFamily: {
        mono: ['ui-monospace', 'SFMono-Regular', 'Menlo', 'Monaco', 'Consolas', '"Liberation Mono"', '"Courier New"', 'monospace'],
      },
    },
  },
  plugins: [],
}