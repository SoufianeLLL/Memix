import { BrainManager } from './brain';
import { BRAIN_KEYS, BRAIN_KEY_SPECS, HealthReport, HealthCheck, TAXONOMY_MAP, MAX_KEY_SIZE_BYTES } from '../utils/constants';

export class HealthMonitor {
    constructor(private brain: BrainManager) { }

    async runFullCheck(): Promise<HealthReport> {
        // Fetch once to avoid hammering the daemon with many getMemory() calls.
		const allData = await this.brain.getAll();
		return this.runFullCheckFromSnapshot(allData);
    }

	runFullCheckFromSnapshot(allData: Record<string, any>): HealthReport {
		const checks: HealthCheck[] = [];
		const recommendations: string[] = [];
		let totalSize = 0;

        const requiredKeys = Object.entries(BRAIN_KEY_SPECS)
			.filter(([, spec]) => spec.tier === 'required')
			.map(([key]) => key);

        const allKeys = Object.values(BRAIN_KEYS).filter(k => k !== 'checkpoints' && k !== 'meta');

        let missingRequiredCount = 0;

        for (const key of allKeys) {
            const check = this.checkKeyFromSnapshot(key, allData);
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
        const state = allData[BRAIN_KEYS.SESSION_STATE];
        if (state?.last_updated) {
            const lastUpdate = new Date(state.last_updated);
            const hoursAgo = (Date.now() - lastUpdate.getTime()) / (1000 * 60 * 60);
            if (hoursAgo > 72) {
                recommendations.push(`Brain hasn't been updated in ${Math.round(hoursAgo)}h. State may be stale.`);
            }
        }

        // Session log size check
        const log = allData[BRAIN_KEYS.SESSION_LOG];
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

	private checkKeyFromSnapshot(key: string, snapshot: Record<string, any>): HealthCheck {
		const issues: string[] = [];
		let valid = true;
		const parsed = snapshot[key];

		if (parsed === null || parsed === undefined) {
			return {
				key,
				exists: false,
				sizeBytes: 0,
				valid: true,
				taxonomy: TAXONOMY_MAP[key] || 'unknown',
				issues: ['Key does not exist']
			};
		}

		const rawString = typeof parsed === 'string' ? parsed : JSON.stringify(parsed);
		const sizeBytes = Buffer.byteLength(rawString || '', 'utf8');

		// Structure checks
		if (key === BRAIN_KEYS.SESSION_LOG && !Array.isArray(parsed)) {
			valid = false;
			issues.push('Expected array, got ' + typeof parsed);
		}
		if (key === BRAIN_KEYS.DECISIONS && !Array.isArray(parsed)) {
			valid = false;
			issues.push('Expected array, got ' + typeof parsed);
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