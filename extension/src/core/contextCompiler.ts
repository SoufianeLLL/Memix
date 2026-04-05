/**
 * Smart Context Compiler - Exports brain context within character budget.
 * 
 * This helps users export their brain to a format that fits within
 * their LLM's context window (estimated, not exact token count).
 */

import { BrainManager } from './brain';
import { BRAIN_KEYS } from '../utils/constants';

export interface CompiledContext {
    content: string;
    totalChars: number;
    includedKeys: string[];
    truncatedKeys: string[];
    skippedKeys: string[];
}

export interface ContextCompilerOptions {
    /** Maximum characters to include (default: 10000) */
    maxChars?: number;
    /** Include markdown formatting (default: true) */
    markdown?: boolean;
    /** Priority order for keys (higher priority = included first) */
    priorityOrder?: string[];
}

/**
 * Compiles brain content into a compact, prioritized format
 * that fits within a character budget.
 */
export class ContextCompiler {
    private brain: BrainManager;
    
    constructor(brain: BrainManager) {
        this.brain = brain;
    }
    
    /**
     * Compile brain content within character budget.
     */
    async compile(options: ContextCompilerOptions = {}): Promise<CompiledContext> {
        const {
            maxChars = 10000,
            markdown = true,
            priorityOrder = this.getDefaultPriorityOrder()
        } = options;
        
        const all = await this.brain.getAll();
        
        const includedKeys: string[] = [];
        const truncatedKeys: string[] = [];
        const skippedKeys: string[] = [];
        
        let compiled = '';
        let remaining = maxChars;
        
        // Add header
        if (markdown) {
            const header = '# Memix Brain Context\n\n';
            compiled += header;
            remaining -= header.length;
        }
        
        // Process keys in priority order
        for (const key of priorityOrder) {
            const value = all[key];
            if (!value) {
                continue;
            }
            
            const section = this.formatSection(key, value, markdown);
            
            if (section.length <= remaining) {
                // Full section fits
                compiled += section + '\n';
                remaining -= section.length + 1;
                includedKeys.push(key);
            } else if (remaining > 100) {
                // Partial section fits - truncate
                const truncated = this.truncateSection(section, remaining - 50, markdown);
                compiled += truncated + '\n';
                truncatedKeys.push(key);
                remaining = 0;
                break;
            } else {
                // No room left
                skippedKeys.push(key);
            }
        }
        
        // Add remaining keys that weren't in priority order
        const processedKeys = new Set([...includedKeys, ...truncatedKeys, ...skippedKeys]);
        for (const [key, value] of Object.entries(all)) {
            if (processedKeys.has(key) || !value) {
                continue;
            }
            
            if (remaining < 100) {
                skippedKeys.push(key);
                continue;
            }
            
            const section = this.formatSection(key, value, markdown);
            if (section.length <= remaining) {
                compiled += section + '\n';
                remaining -= section.length + 1;
                includedKeys.push(key);
            } else {
                skippedKeys.push(key);
            }
        }
        
        // Add footer with stats
        if (markdown && remaining > 50) {
            const footer = `\n---\n*Compiled by Memix. ${includedKeys.length} keys included, ${truncatedKeys.length} truncated, ${skippedKeys.length} skipped.*\n`;
            if (footer.length <= remaining) {
                compiled += footer;
            }
        }
        
        return {
            content: compiled,
            totalChars: compiled.length,
            includedKeys,
            truncatedKeys,
            skippedKeys
        };
    }
    
    /**
     * Compile with automatic budget estimation.
     * Estimates character budget based on typical LLM context windows.
     */
    async compileForModel(model: 'claude' | 'gpt4' | 'gpt35' | 'gemini'): Promise<CompiledContext> {
        const budgets: Record<string, number> = {
            'claude': 15000,      // Claude has 200k tokens, ~150k chars
            'gpt4': 6000,         // GPT-4 has 128k tokens, ~96k chars
            'gpt35': 3000,        // GPT-3.5 has 16k tokens, ~12k chars
            'gemini': 20000       // Gemini has 1M tokens, ~750k chars
        };
        
        return this.compile({ maxChars: budgets[model] || 10000 });
    }
    
    /**
     * Get default priority order for brain keys.
     * Most important keys first.
     */
    private getDefaultPriorityOrder(): string[] {
        return [
            BRAIN_KEYS.IDENTITY,       // Project identity - critical
            BRAIN_KEYS.PATTERNS,       // Coding patterns - very useful
            BRAIN_KEYS.DECISIONS,      // Architecture decisions - important
            BRAIN_KEYS.KNOWN_ISSUES,   // Known issues - prevents mistakes
            BRAIN_KEYS.FILE_MAP,       // File map - useful context
            BRAIN_KEYS.SESSION_STATE,  // Current state - situational
            BRAIN_KEYS.TASKS,          // Tasks - situational
            BRAIN_KEYS.SESSION_LOG,    // History - optional
            BRAIN_KEYS.META            // Metadata - least important
        ];
    }
    
    /**
     * Format a section for output.
     */
    private formatSection(key: string, value: any, markdown: boolean): string {
        const displayName = this.getDisplayName(key);
        const content = typeof value === 'string' ? value : JSON.stringify(value, null, 2);
        
        if (markdown) {
            return `## ${displayName}\n\n${content}\n`;
        }
        return `[${displayName}]\n${content}\n`;
    }
    
    /**
     * Truncate a section to fit within budget.
     */
    private truncateSection(section: string, maxChars: number, markdown: boolean): string {
        if (section.length <= maxChars) {
            return section;
        }
        
        // Find a good truncation point (end of line or sentence)
        let truncateAt = maxChars;
        const lastNewline = section.lastIndexOf('\n', maxChars);
        const lastPeriod = section.lastIndexOf('. ', maxChars);
        
        if (lastNewline > maxChars * 0.7) {
            truncateAt = lastNewline;
        } else if (lastPeriod > maxChars * 0.7) {
            truncateAt = lastPeriod + 1;
        }
        
        const truncated = section.slice(0, truncateAt);
        const suffix = markdown ? '\n\n*[truncated]*' : '\n[truncated]';
        
        return truncated + suffix;
    }
    
    /**
     * Get human-readable display name for a key.
     */
    private getDisplayName(key: string): string {
        const names: Record<string, string> = {
            [BRAIN_KEYS.IDENTITY]: 'Project Identity',
            [BRAIN_KEYS.SESSION_STATE]: 'Current Session',
            [BRAIN_KEYS.PATTERNS]: 'Coding Patterns',
            [BRAIN_KEYS.TASKS]: 'Tasks',
            [BRAIN_KEYS.DECISIONS]: 'Architecture Decisions',
            [BRAIN_KEYS.FILE_MAP]: 'File Map',
            [BRAIN_KEYS.KNOWN_ISSUES]: 'Known Issues',
            [BRAIN_KEYS.SESSION_LOG]: 'Session History',
            [BRAIN_KEYS.META]: 'Metadata'
        };
        
        return names[key] || key.replace(/brain:/i, '').replace(/_/g, ' ');
    }
    
    /**
     * Estimate character count for a value.
     */
    estimateSize(value: any): number {
        if (typeof value === 'string') {
            return value.length;
        }
        return JSON.stringify(value).length;
    }
    
    /**
     * Get total brain size estimate.
     */
    async getTotalSize(): Promise<number> {
        const all = await this.brain.getAll();
        return Object.values(all)
            .filter(v => v)
            .reduce((sum, v) => sum + this.estimateSize(v), 0);
    }
}
