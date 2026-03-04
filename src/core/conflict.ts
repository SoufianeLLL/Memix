import * as vscode from 'vscode';
import * as path from 'path';
import * as fs from 'fs';
import { BrainManager } from './brain';
import { BRAIN_KEYS, ConflictReport } from '../utils/constants';

export class ConflictHandler {
    constructor(
        private brain: BrainManager,
        private workspaceRoot: string
    ) { }

    async detectConflicts(): Promise<ConflictReport[]> {
        const conflicts: ConflictReport[] = [];

        // 1. Check file_map against actual filesystem
        const fileMap = await this.brain.get(BRAIN_KEYS.FILE_MAP);
        if (fileMap && typeof fileMap === 'object') {
            for (const filePath of Object.keys(fileMap)) {
                const fullPath = path.join(this.workspaceRoot, filePath);
                if (!fs.existsSync(fullPath)) {
                    conflicts.push({
                        key: BRAIN_KEYS.FILE_MAP,
                        brainValue: `"${filePath}" exists in brain`,
                        actualValue: 'File does not exist on disk',
                        recommendation: `Remove "${filePath}" from file_map`,
                        autoResolvable: true
                    });
                }
            }
        }

        // 2. Check tech_stack against package.json
        const identity = await this.brain.get(BRAIN_KEYS.IDENTITY);
        const pkgPath = path.join(this.workspaceRoot, 'package.json');
        if (identity?.tech_stack && fs.existsSync(pkgPath)) {
            try {
                const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf8'));
                const allDeps = {
                    ...pkg.dependencies,
                    ...pkg.devDependencies
                };
                const depNames = Object.keys(allDeps);

                for (const tech of identity.tech_stack) {
                    const techLower = tech.toLowerCase().replace(/\s+\d+.*/, '');
                    const found = depNames.some(d => d.toLowerCase().includes(techLower));
                    if (!found && !['typescript', 'javascript', 'html', 'css'].includes(techLower)) {
                        conflicts.push({
                            key: BRAIN_KEYS.IDENTITY,
                            brainValue: `tech_stack includes "${tech}"`,
                            actualValue: `"${tech}" not found in package.json dependencies`,
                            recommendation: `Verify if "${tech}" is still in use`,
                            autoResolvable: false
                        });
                    }
                }
            } catch { /* skip if package.json unreadable */ }
        }

        return conflicts;
    }

    async autoResolve(conflicts: ConflictReport[]): Promise<string[]> {
        const resolved: string[] = [];

        for (const conflict of conflicts.filter(c => c.autoResolvable)) {
            if (conflict.key === BRAIN_KEYS.FILE_MAP) {
                const fileMap = await this.brain.get(BRAIN_KEYS.FILE_MAP);
                if (fileMap) {
                    // Extract file path from brain value
                    const match = conflict.brainValue.match(/"([^"]+)"/);
                    if (match) {
                        delete fileMap[match[1]];
                        await this.brain.set(BRAIN_KEYS.FILE_MAP, fileMap);
                        resolved.push(`Removed missing file "${match[1]}" from file_map`);
                    }
                }
            }
        }

        return resolved;
    }
}