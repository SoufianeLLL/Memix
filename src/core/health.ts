import { BrainManager } from './brain';
import { BRAIN_KEYS, HealthReport, HealthCheck, TAXONOMY_MAP, MAX_KEY_SIZE_BYTES } from '../utils/constants';
import * as redisClient from './redis-client';

export class HealthMonitor {
    constructor(private brain: BrainManager) { }

    async runFullCheck(): Promise<HealthReport> {
        const checks: HealthCheck[] = [];
        const recommendations: string[] = [];
        let totalSize = 0;

        const requiredKeys: readonly string[] = [
            BRAIN_KEYS.IDENTITY,
            BRAIN_KEYS.SESSION_STATE,
            BRAIN_KEYS.PATTERNS
        ];

        const allKeys = Object.values(BRAIN_KEYS).filter(k => k !== 'checkpoints' && k !== 'meta');

        let missingRequiredCount = 0;

        for (const key of allKeys) {
            const check = await this.checkKey(key);
            checks.push(check);
            totalSize += check.sizeBytes;

            if (requiredKeys.includes(key) && !check.exists) {
                missingRequiredCount++;
            }

            if (check.sizeBytes > MAX_KEY_SIZE_BYTES * 0.9) {
                recommendations.push(`Key "${key}" is at ${Math.round(check.sizeBytes / MAX_KEY_SIZE_BYTES * 100)}% capacity. Consider pruning.`);
            }

            if (!check.valid && check.exists) {
                recommendations.push(`Key "${key}" contains invalid data. Run Recover Corruption.`);
            }
        }

        if (missingRequiredCount > 0) {
            recommendations.push(`CRITICAL: Brain not fully initialized. Create or Initialize your brain.`);
        }

        // Staleness check
        const state = await this.brain.get(BRAIN_KEYS.SESSION_STATE);
        if (state?.last_updated) {
            const lastUpdate = new Date(state.last_updated);
            const hoursAgo = (Date.now() - lastUpdate.getTime()) / (1000 * 60 * 60);
            if (hoursAgo > 72) {
                recommendations.push(`Brain hasn't been updated in ${Math.round(hoursAgo)}h. State may be stale.`);
            }
        }

        // Session log size check
        const log = await this.brain.get(BRAIN_KEYS.SESSION_LOG);
        if (Array.isArray(log) && log.length > 30) {
            recommendations.push(`Session log has ${log.length} entries. Consider archiving old ones.`);
        }

        const hasErrors = checks.some(c => !c.valid && c.exists);
        const hasMissing = requiredKeys.some(k => !checks.find(c => c.key === k)?.exists);

        return {
            status: hasErrors ? 'critical' : hasMissing ? 'warning' : 'healthy',
            timestamp: new Date().toISOString(),
            checks,
            totalSizeBytes: totalSize,
            recommendations
        };
    }

    private async checkKey(key: string): Promise<HealthCheck> {
        const raw = await redisClient.brainGet(this.brain.getPrefix(), key);
        const issues: string[] = [];

        if (!raw) {
            return {
                key,
                exists: false,
                sizeBytes: 0,
                valid: true,
                taxonomy: TAXONOMY_MAP[key] || 'unknown',
                issues: ['Key does not exist']
            };
        }

        const sizeBytes = Buffer.byteLength(raw, 'utf8');
        let valid = true;

        // JSON validity
        try {
            const parsed = JSON.parse(raw);

            // Structure checks
            if (key === BRAIN_KEYS.SESSION_LOG && !Array.isArray(parsed)) {
                valid = false;
                issues.push('Expected array, got ' + typeof parsed);
            }
            if (key === BRAIN_KEYS.DECISIONS && !Array.isArray(parsed)) {
                valid = false;
                issues.push('Expected array, got ' + typeof parsed);
            }
        } catch {
            valid = false;
            issues.push('Invalid JSON');
        }

        if (sizeBytes > MAX_KEY_SIZE_BYTES) {
            issues.push(`Exceeds max size: ${sizeBytes}B > ${MAX_KEY_SIZE_BYTES}B`);
        }

        return {
            key,
            exists: true,
            sizeBytes,
            valid,
            taxonomy: TAXONOMY_MAP[key] || 'unknown',
            issues
        };
    }
}