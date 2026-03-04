import { BrainManager } from './brain';
import { BRAIN_KEYS, MAX_SESSION_LOG_ENTRIES, MAX_KEY_SIZE_BYTES } from '../utils/constants';
import * as vscode from 'vscode';

export class BrainPruner {
    constructor(private brain: BrainManager) { }

    async prune(): Promise<string[]> {
        const actions: string[] = [];

        // 1. Prune session log
        const log = await this.brain.get(BRAIN_KEYS.SESSION_LOG);
        if (Array.isArray(log) && log.length > MAX_SESSION_LOG_ENTRIES) {
            const trimmed = log.slice(-MAX_SESSION_LOG_ENTRIES);
            await this.brain.set(BRAIN_KEYS.SESSION_LOG, trimmed);
            actions.push(`Session log pruned: ${log.length} → ${trimmed.length} entries`);
        }

        // 2. Prune old known issues (keep last 20, remove old FIXED)
        const issues = await this.brain.get(BRAIN_KEYS.KNOWN_ISSUES);
        if (Array.isArray(issues)) {
            const open = issues.filter((i: any) => i.status !== 'FIXED');
            const fixed = issues.filter((i: any) => i.status === 'FIXED');
            const recentFixed = fixed.slice(-5); // keep last 5 fixed
            const pruned = [...open, ...recentFixed];
            if (pruned.length < issues.length) {
                await this.brain.set(BRAIN_KEYS.KNOWN_ISSUES, pruned);
                actions.push(`Known issues pruned: ${issues.length} → ${pruned.length}`);
            }
        }

        // 3. Trim decisions if too large
        const decisions = await this.brain.get(BRAIN_KEYS.DECISIONS);
        if (Array.isArray(decisions)) {
            const serialized = JSON.stringify(decisions);
            if (Buffer.byteLength(serialized) > MAX_KEY_SIZE_BYTES) {
                // Keep most recent half
                const trimmed = decisions.slice(-Math.ceil(decisions.length / 2));
                await this.brain.set(BRAIN_KEYS.DECISIONS, trimmed);
                actions.push(`Decisions pruned: ${decisions.length} → ${trimmed.length}`);
            }
        }

        // 4. Compact session state progress array
        const state = await this.brain.get(BRAIN_KEYS.SESSION_STATE);
        if (state?.progress && Array.isArray(state.progress) && state.progress.length > 10) {
            state.progress = state.progress.slice(-5);
            await this.brain.set(BRAIN_KEYS.SESSION_STATE, state);
            actions.push('Session state progress compacted to last 5 entries');
        }

        if (actions.length === 0) {
            actions.push('Brain is clean, nothing to prune');
        }

        return actions;
    }

    async recoverCorruption(): Promise<string[]> {
        const actions: string[] = [];
        const allKeys = Object.values(BRAIN_KEYS);

        for (const key of allKeys) {
            const raw = await this.brain.get(key);
            if (raw === null) { continue; }

            // If it's supposed to be JSON but isn't parseable
            if (typeof raw === 'string') {
                try {
                    JSON.parse(raw);
                } catch {
                    // Attempt recovery
                    if (key === BRAIN_KEYS.SESSION_LOG || key === BRAIN_KEYS.DECISIONS ||
                        key === BRAIN_KEYS.KNOWN_ISSUES) {
                        // These should be arrays — reset to empty
                        await this.brain.set(key, []);
                        actions.push(`${key}: corrupted array → reset to []`);
                    } else if (key === BRAIN_KEYS.FILE_MAP || key === BRAIN_KEYS.PATTERNS ||
                        key === BRAIN_KEYS.IDENTITY) {
                        // These should be objects — reset to empty
                        await this.brain.set(key, {});
                        actions.push(`${key}: corrupted object → reset to {}`);
                    } else if (key === BRAIN_KEYS.SESSION_STATE) {
                        // Critical — reset with template
                        await this.brain.set(key, {
                            last_updated: new Date().toISOString(),
                            session_number: 0,
                            current_task: 'RECOVERED — previous state was corrupted',
                            progress: [],
                            blockers: [],
                            next_steps: ['Review brain state after recovery'],
                            modified_files: [],
                            important_context: 'Brain was recovered from corruption'
                        });
                        actions.push(`${key}: corrupted → reset with recovery template`);
                    }
                }
            }
        }

        if (actions.length === 0) {
            actions.push('No corruption detected');
        }

        return actions;
    }
}