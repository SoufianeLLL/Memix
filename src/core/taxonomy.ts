/**
 * Memory Taxonomy Manager
 *
 * Classifies brain keys into cognitive memory categories,
 * assigns priority scores, and defines retention policies.
 * This is the intelligence layer on top of the raw TAXONOMY_MAP.
 */

import { BRAIN_KEYS, MEMORY_TAXONOMY, TAXONOMY_MAP } from '../utils/constants';

// Re-export for backward compatibility
export { MEMORY_TAXONOMY, TAXONOMY_MAP };

// --- Retention Policies ---

export type RetentionPolicy = 'permanent' | 'session' | 'rolling' | 'ephemeral';

export interface TaxonomyEntry {
    category: string;
    description: string;
    priority: number;         // 1 (highest) – 5 (lowest)
    retention: RetentionPolicy;
    maxAgeDays: number | null; // null = never expires
    compactable: boolean;      // can be summarized / trimmed
}

const TAXONOMY_REGISTRY: Record<string, TaxonomyEntry> = {
    [BRAIN_KEYS.IDENTITY]: {
        category: MEMORY_TAXONOMY.SEMANTIC,
        description: 'What the project IS — name, purpose, tech stack, architecture',
        priority: 1,
        retention: 'permanent',
        maxAgeDays: null,
        compactable: false,
    },
    [BRAIN_KEYS.SESSION_STATE]: {
        category: MEMORY_TAXONOMY.WORKING,
        description: 'Current work snapshot — the most critical real-time key',
        priority: 1,
        retention: 'session',
        maxAgeDays: null,
        compactable: true,
    },
    [BRAIN_KEYS.PATTERNS]: {
        category: MEMORY_TAXONOMY.PROCEDURAL,
        description: 'Coding conventions, naming rules, user preferences',
        priority: 2,
        retention: 'permanent',
        maxAgeDays: null,
        compactable: false,
    },
    [BRAIN_KEYS.FILE_MAP]: {
        category: MEMORY_TAXONOMY.SEMANTIC,
        description: 'Purpose and dependency info for key project files',
        priority: 3,
        retention: 'permanent',
        maxAgeDays: null,
        compactable: true,
    },
    [BRAIN_KEYS.DECISIONS]: {
        category: MEMORY_TAXONOMY.EPISODIC,
        description: 'Architecture / design decisions with rationale',
        priority: 3,
        retention: 'rolling',
        maxAgeDays: 90,
        compactable: true,
    },
    [BRAIN_KEYS.KNOWN_ISSUES]: {
        category: MEMORY_TAXONOMY.DIAGNOSTIC,
        description: 'Open bugs, tech debt, resolved issues',
        priority: 3,
        retention: 'rolling',
        maxAgeDays: 60,
        compactable: true,
    },
    [BRAIN_KEYS.SESSION_LOG]: {
        category: MEMORY_TAXONOMY.EPISODIC,
        description: 'Historical record of past sessions',
        priority: 4,
        retention: 'rolling',
        maxAgeDays: 120,
        compactable: true,
    },
    [BRAIN_KEYS.SCORING]: {
        category: MEMORY_TAXONOMY.EPISODIC,
        description: 'Session productivity scores',
        priority: 5,
        retention: 'rolling',
        maxAgeDays: 180,
        compactable: true,
    },
    [BRAIN_KEYS.TEAM_STATE]: {
        category: MEMORY_TAXONOMY.WORKING,
        description: 'Shared team brain sync state',
        priority: 4,
        retention: 'session',
        maxAgeDays: null,
        compactable: false,
    },
};

// --- Taxonomy Manager ---

export class MemoryTaxonomyManager {

    /**
     * Get the full taxonomy entry for a brain key.
     */
    static getEntry(key: string): TaxonomyEntry | null {
        return TAXONOMY_REGISTRY[key] ?? null;
    }

    /**
     * Get the memory category for a brain key.
     */
    static getCategoryForKey(key: string): string {
        return TAXONOMY_REGISTRY[key]?.category ?? TAXONOMY_MAP[key] ?? 'unknown';
    }

    /**
     * Get priority score (1 = highest, 5 = lowest).
     */
    static getPriorityScore(key: string): number {
        return TAXONOMY_REGISTRY[key]?.priority ?? 5;
    }

    /**
     * Get retention policy for a brain key.
     */
    static getRetentionPolicy(key: string): RetentionPolicy {
        return TAXONOMY_REGISTRY[key]?.retention ?? 'rolling';
    }

    /**
     * Check if a key's data has exceeded its max age.
     */
    static isExpired(key: string, lastUpdated: Date): boolean {
        const entry = TAXONOMY_REGISTRY[key];
        if (!entry || entry.maxAgeDays === null) { return false; }
        const ageDays = (Date.now() - lastUpdated.getTime()) / (1000 * 60 * 60 * 24);
        return ageDays > entry.maxAgeDays;
    }

    /**
     * Whether a key's data can be summarized / trimmed during pruning.
     */
    static isCompactable(key: string): boolean {
        return TAXONOMY_REGISTRY[key]?.compactable ?? true;
    }

    /**
     * Return all brain keys sorted by priority (highest first).
     */
    static getKeysByPriority(): string[] {
        return Object.entries(TAXONOMY_REGISTRY)
            .sort(([, a], [, b]) => a.priority - b.priority)
            .map(([key]) => key);
    }

    /**
     * Return all keys belonging to a given taxonomy category.
     */
    static getKeysByCategory(category: string): string[] {
        return Object.entries(TAXONOMY_REGISTRY)
            .filter(([, entry]) => entry.category === category)
            .map(([key]) => key);
    }

    /**
     * Return the boot-sequence keys (priority 1-2, always loaded first).
     */
    static getBootKeys(): string[] {
        return Object.entries(TAXONOMY_REGISTRY)
            .filter(([, entry]) => entry.priority <= 2)
            .map(([key]) => key);
    }

    /**
     * Get a human-readable summary of all taxonomy entries.
     */
    static getSummary(): Record<string, { keys: string[]; description: string }> {
        const result: Record<string, { keys: string[]; description: string }> = {};
        for (const cat of Object.values(MEMORY_TAXONOMY)) {
            const keys = this.getKeysByCategory(cat);
            const descriptions: Record<string, string> = {
                [MEMORY_TAXONOMY.SEMANTIC]: 'What the project IS (identity, architecture)',
                [MEMORY_TAXONOMY.EPISODIC]: 'What HAPPENED (session logs, decisions)',
                [MEMORY_TAXONOMY.PROCEDURAL]: 'HOW to do things (patterns, conventions)',
                [MEMORY_TAXONOMY.WORKING]: 'Current task state (session:state)',
                [MEMORY_TAXONOMY.DIAGNOSTIC]: 'Issues, health, conflicts (known_issues)',
            };
            result[cat] = { keys, description: descriptions[cat] || cat };
        }
        return result;
    }
}
