import { BrainManager } from './brain';
import { BRAIN_KEYS, MEMORY_TAXONOMY, TAXONOMY_MAP } from '../utils/constants';
import { getTokenCount, trimToTokenBudget } from './tokenizer';
import * as vscode from 'vscode';

interface LoadTier {
    priority: number;
    keys: string[];
    label: string;
    required: boolean;
}

const LOAD_TIERS: LoadTier[] = [
    {
        priority: 1,
        keys: [BRAIN_KEYS.SESSION_STATE],
        label: 'Working Memory',
        required: true
    },
    {
        priority: 2,
        keys: [BRAIN_KEYS.IDENTITY, BRAIN_KEYS.PATTERNS],
        label: 'Core Identity',
        required: true
    },
    {
        priority: 3,
        keys: [BRAIN_KEYS.FILE_MAP, BRAIN_KEYS.KNOWN_ISSUES],
        label: 'Project State',
        required: false
    },
    {
        priority: 4,
        keys: [BRAIN_KEYS.DECISIONS, BRAIN_KEYS.SESSION_LOG],
        label: 'History',
        required: false
    }
];

export class MemoryOrchestrator {
    constructor(private brain: BrainManager) { }

    /**
     * Load brain context in priority order, respecting token budget.
     * Returns the context string to inject into rules file.
     */
    async loadWithBudget(tokenBudget?: number): Promise<string> {
        const budget = tokenBudget ||
            vscode.workspace.getConfiguration('memix').get<number>('maxTokenBudget') || 4000;

        let usedTokens = 0;
        const sections: string[] = [];

        for (const tier of LOAD_TIERS) {
            if (usedTokens >= budget && !tier.required) {
                sections.push(`\n<!-- Tier ${tier.priority} (${tier.label}) skipped: token budget reached -->`);
                continue;
            }

            for (const key of tier.keys) {
                const data = await this.brain.get(key);
                if (!data) { continue; }

                const serialized = JSON.stringify(data, null, 2);
                const tokens = getTokenCount(serialized);

                if (usedTokens + tokens > budget && !tier.required) {
                    // Trim this entry to fit remaining budget
                    const remaining = budget - usedTokens;
                    const trimmed = trimToTokenBudget(serialized, remaining);
                    sections.push(this.formatSection(key, trimmed, true));
                    usedTokens = budget;
                    break;
                }

                sections.push(this.formatSection(key, serialized, false));
                usedTokens += tokens;
            }
        }

        const header = `<!-- MEMIX BRAIN CONTEXT | Tokens: ${usedTokens}/${budget} | ${new Date().toISOString()} -->`;
        return header + '\n' + sections.join('\n');
    }

    /**
     * Determine which keys need updating based on what happened
     */
    async determineUpdates(event: OrchestratorEvent): Promise<string[]> {
        const keysToUpdate: string[] = [];

        switch (event.type) {
            case 'task_completed':
                keysToUpdate.push(BRAIN_KEYS.SESSION_STATE);
                break;
            case 'file_modified':
                keysToUpdate.push(BRAIN_KEYS.SESSION_STATE, BRAIN_KEYS.FILE_MAP);
                break;
            case 'decision_made':
                keysToUpdate.push(BRAIN_KEYS.DECISIONS, BRAIN_KEYS.SESSION_STATE);
                break;
            case 'bug_found':
                keysToUpdate.push(BRAIN_KEYS.KNOWN_ISSUES, BRAIN_KEYS.SESSION_STATE);
                break;
            case 'pattern_learned':
                keysToUpdate.push(BRAIN_KEYS.PATTERNS);
                break;
            case 'session_end':
                keysToUpdate.push(
                    BRAIN_KEYS.SESSION_STATE,
                    BRAIN_KEYS.SESSION_LOG,
                    BRAIN_KEYS.SCORING
                );
                break;
        }

        return keysToUpdate;
    }

    /**
     * Get memory by taxonomy category
     */
    async getByTaxonomy(category: string): Promise<Record<string, any>> {
        const result: Record<string, any> = {};
        for (const [key, tax] of Object.entries(TAXONOMY_MAP)) {
            if (tax === category) {
                const data = await this.brain.get(key);
                if (data) { result[key] = data; }
            }
        }
        return result;
    }

    private formatSection(key: string, content: string, trimmed: boolean): string {
        const taxonomy = TAXONOMY_MAP[key] || 'unknown';
        const trimLabel = trimmed ? ' [TRIMMED]' : '';
        return `\n### ${key} (${taxonomy})${trimLabel}\n\`\`\`json\n${content}\n\`\`\``;
    }
}

export interface OrchestratorEvent {
    type: 'task_completed' | 'file_modified' | 'decision_made' |
    'bug_found' | 'pattern_learned' | 'session_end';
    data?: any;
}