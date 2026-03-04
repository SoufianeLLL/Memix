import * as vscode from 'vscode';
import * as fs from 'fs';
import * as path from 'path';
import { detectIDE, getRulesConfig, IDERulesConfig } from './detector';
import { getBrainTemplate, getGuardTemplate } from './templates';

export class RulesEngine {
    private config: IDERulesConfig;
    private workspaceRoot: string;

    constructor(workspaceRoot: string) {
        this.workspaceRoot = workspaceRoot;
        const ide = detectIDE();
        this.config = getRulesConfig(ide);
    }

    getConfig(): IDERulesConfig {
        return this.config;
    }

    async generateRules(projectId: string, redisUrl: string): Promise<void> {
        const rulesDir = path.join(this.workspaceRoot, this.config.rulesDir);

        // Create rules directory if needed
        if (!fs.existsSync(rulesDir)) {
            fs.mkdirSync(rulesDir, { recursive: true });
        }

        const brainContent = getBrainTemplate(projectId, redisUrl);
        const guardContent = getGuardTemplate(projectId);

        if (this.config.supportsMultipleFiles) {
            // Write both files
            const brainPath = path.join(rulesDir, this.config.rulesFile);
            const guardPath = path.join(rulesDir, this.config.guardFile);

            fs.writeFileSync(brainPath, brainContent, 'utf8');
            fs.writeFileSync(guardPath, guardContent, 'utf8');

            // Add companion link to brain file
            const link = `\n\n---\n## COMPANION: ${this.config.guardFile}\nYou MUST also read and obey ${this.config.guardFile}. Both files are ONE system.`;
            fs.appendFileSync(brainPath, link, 'utf8');

        } else {
            // Single file IDE — combine both
            const combined = brainContent + '\n\n---\n\n' + guardContent;
            const filePath = path.join(rulesDir, this.config.rulesFile);
            fs.writeFileSync(filePath, combined, 'utf8');
        }

        // Add rules files to .gitignore
        this.addToGitignore();

        vscode.window.showInformationMessage(
            `Memix: Rules generated for ${this.config.ide} in ${this.config.rulesDir}/`
        );
    }

    rulesExist(): boolean {
        const brainPath = path.join(
            this.workspaceRoot,
            this.config.rulesDir,
            this.config.rulesFile
        );
        return fs.existsSync(brainPath);
    }

    private addToGitignore(): void {
        const gitignorePath = path.join(this.workspaceRoot, '.gitignore');
        const entries = [
            `# Memix AI Brain Rules`,
            this.config.rulesDir === '.'
                ? this.config.rulesFile
                : `${this.config.rulesDir}/`,
        ];

        if (fs.existsSync(gitignorePath)) {
            const content = fs.readFileSync(gitignorePath, 'utf8');
            const toAdd = entries.filter(e => !content.includes(e));
            if (toAdd.length > 0) {
                fs.appendFileSync(gitignorePath, '\n' + toAdd.join('\n') + '\n');
            }
        } else {
            fs.writeFileSync(gitignorePath, entries.join('\n') + '\n');
        }
    }
}