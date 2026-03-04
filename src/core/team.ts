import { BrainManager } from './brain';
import { BRAIN_KEYS } from '../utils/constants';
import * as redisClient from './redis-client';

export class TeamSync {
    private teamPrefix: string;

    constructor(
        private brain: BrainManager,
        private teamId: string,
        private projectId: string,
        private memberId: string
    ) {
        this.teamPrefix = `team:${teamId}:${projectId}`;
    }

    /**
     * Push local brain keys that should be shared to team namespace
     */
    async pushToTeam(): Promise<string[]> {
        const sharedKeys = [
            BRAIN_KEYS.IDENTITY,
            BRAIN_KEYS.PATTERNS,
            BRAIN_KEYS.DECISIONS,
            BRAIN_KEYS.FILE_MAP,
            BRAIN_KEYS.KNOWN_ISSUES
        ];

        const pushed: string[] = [];

        for (const key of sharedKeys) {
            const data = await this.brain.get(key);
            if (data) {
                await redisClient.brainSet(this.teamPrefix, key, JSON.stringify(data));
                pushed.push(key);
            }
        }

        // Record who pushed and when
        await redisClient.brainSet(this.teamPrefix, 'last_push', JSON.stringify({
            member: this.memberId,
            timestamp: new Date().toISOString(),
            keys: pushed
        }));

        return pushed;
    }

    /**
     * Pull shared team brain into local brain
     */
    async pullFromTeam(): Promise<string[]> {
        const sharedKeys = [
            BRAIN_KEYS.IDENTITY,
            BRAIN_KEYS.PATTERNS,
            BRAIN_KEYS.DECISIONS,
            BRAIN_KEYS.FILE_MAP,
            BRAIN_KEYS.KNOWN_ISSUES
        ];

        const pulled: string[] = [];

        for (const key of sharedKeys) {
            const teamData = await redisClient.brainGet(this.teamPrefix, key);
            if (teamData) {
                await this.brain.set(key, JSON.parse(teamData));
                pulled.push(key);
            }
        }

        return pulled;
    }

    /**
     * Merge team decisions with local (no duplicates)
     */
    async mergeDecisions(): Promise<number> {
        const local = await this.brain.get(BRAIN_KEYS.DECISIONS) || [];
        const teamRaw = await redisClient.brainGet(this.teamPrefix, BRAIN_KEYS.DECISIONS);
        const team = teamRaw ? JSON.parse(teamRaw) : [];

        const localDates = new Set(local.map((d: any) => d.date + d.decision));
        const newEntries = team.filter((d: any) => !localDates.has(d.date + d.decision));

        if (newEntries.length > 0) {
            const merged = [...local, ...newEntries];
            await this.brain.set(BRAIN_KEYS.DECISIONS, merged);
        }

        return newEntries.length;
    }
}