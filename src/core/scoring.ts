import { BrainManager } from './brain';
import { BRAIN_KEYS, SessionScore } from '../utils/constants';

export class SessionScorer {
    private score: SessionScore = {
        tasks_completed: 0,
        tasks_started: 0,
        bugs_found: 0,
        bugs_fixed: 0,
        decisions_made: 0,
        files_modified: 0,
        brain_updates: 0
    };

    constructor(private brain: BrainManager) { }

    increment(field: keyof SessionScore, amount: number = 1) {
        this.score[field] += amount;
    }

    getScore(): SessionScore {
        return { ...this.score };
    }

    async saveScore(sessionNumber: number): Promise<void> {
        // Append to scoring history
        let history = await this.brain.get(BRAIN_KEYS.SCORING) || [];
        if (!Array.isArray(history)) { history = []; }

        history.push({
            session: sessionNumber,
            date: new Date().toISOString(),
            ...this.score
        });

        // Keep last 50 sessions
        if (history.length > 50) {
            history = history.slice(-50);
        }

        await this.brain.set(BRAIN_KEYS.SCORING, history);
    }

    async getAverages(): Promise<Record<string, number>> {
        const history = await this.brain.get(BRAIN_KEYS.SCORING) || [];
        if (!Array.isArray(history) || history.length === 0) {
            return {};
        }

        const sums: Record<string, number> = {};
        const fields = Object.keys(this.score);

        for (const field of fields) {
            sums[field] = history.reduce(
                (sum: number, entry: any) => sum + (entry[field] || 0), 0
            ) / history.length;
        }

        return sums;
    }

    reset() {
        for (const key of Object.keys(this.score) as (keyof SessionScore)[]) {
            this.score[key] = 0;
        }
    }
}