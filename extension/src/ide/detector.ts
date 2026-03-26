import * as vscode from 'vscode';

export type IDEType = 'cursor' | 'windsurf' | 'claude-code' | 'antigravity' | 'vscode' | 'unknown';

export interface IDERulesConfig {
    ide: IDEType;
    rulesDir: string;        // relative to workspace root
    rulesFile: string;        // primary rules filename
    guardFile: string;        // guard rules filename
    supportsMultipleFiles: boolean;
}

export function detectIDE(): IDEType {
    const appName = vscode.env.appName?.toLowerCase() || '';
    const appHost = vscode.env.appHost?.toLowerCase() || '';

    if (appName.includes('cursor')) { return 'cursor'; }
    if (appName.includes('windsurf')) { return 'windsurf'; }
    if (appName.includes('antigravity')) { return 'antigravity'; }
    if (appName.includes('claude')) { return 'claude-code'; }

    // Check for extensions that indicate IDE
    const extensions = vscode.extensions.all.map(e => e.id.toLowerCase());
    if (extensions.some(e => e.includes('cursor'))) { return 'cursor'; }
    if (extensions.some(e => e.includes('codeium') && e.includes('windsurf'))) { return 'windsurf'; }

    return 'vscode';
}

export function getRulesConfig(ide: IDEType): IDERulesConfig {
    switch (ide) {
        case 'cursor':
            return {
                ide,
                rulesDir: '.cursor/rules',
                rulesFile: 'memix.mdc',
                guardFile: 'memix-guard.mdc',
                supportsMultipleFiles: true
            };
        case 'windsurf':
            return {
                ide,
                rulesDir: '.windsurf/rules',
                rulesFile: 'memix.md',
                guardFile: 'memix-guard.md',
                supportsMultipleFiles: true
            };
        case 'claude-code':
            return {
                ide,
                rulesDir: '.',
                rulesFile: 'CLAUDE.md',
                guardFile: 'CLAUDE-GUARD.md',
                supportsMultipleFiles: true
            };
        case 'antigravity':
            return {
                ide,
                rulesDir: '.agents/rules',
                rulesFile: 'memix.md',
                guardFile: 'memix-guard.md',
                supportsMultipleFiles: true
            };
        default:
            return {
                ide,
                rulesDir: '.',
                rulesFile: '.memix-rules.md',
                guardFile: '.memix-guard.md',
                supportsMultipleFiles: true
            };
    }
}