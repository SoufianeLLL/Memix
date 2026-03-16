import { BrainManager } from './brain';

export interface Conflict {
    file: string;
    type: string;
    description: string;
    severity: 'low' | 'medium' | 'high' | 'critical';
}

export class ConflictHandler {
    constructor(
        private brain: BrainManager,
        private workspaceRoot: string
    ) {}

    async detectConflicts(): Promise<Conflict[]> {
        // Use daemon's conflict detection endpoint if available
        // For now, return empty - daemon's /autonomous/conflicts endpoint handles this
        return [];
    }

    async getConflictingFiles(): Promise<string[]> {
        return [];
    }
}
