import * as vscode from 'vscode';
import * as redisClient from './redis-client';
import { validateBrainWrite } from './validator';
import { BRAIN_KEYS, BrainMeta, MAX_BRAIN_SIZE_KB } from '../utils/constants';

export class BrainManager {
    private prefix: string;
    private writeLog: { key: string; timestamp: number; sizeBytes: number }[] = [];

    constructor(private projectId: string) {
        this.prefix = `brain:${projectId}`;
    }

    getPrefix(): string {
        return this.prefix;
    }

    // --- READ ---
    async get(key: string): Promise<any | null> {
        const raw = await redisClient.brainGet(this.prefix, key);
        if (!raw) { return null; }
        try {
            return JSON.parse(raw);
        } catch {
            return raw;
        }
    }

    // --- WRITE (with validation) ---
    async set(key: string, value: any): Promise<{ success: boolean; errors: string[] }> {
        const serialized = typeof value === 'string' ? value : JSON.stringify(value);

        // Validate before writing
        const validation = validateBrainWrite(key, serialized);

        if (!validation.valid) {
            vscode.window.showWarningMessage(
                `Memix: Write blocked for ${key}: ${validation.errors.join(', ')}`
            );
            return { success: false, errors: validation.errors };
        }

        if (validation.warnings.length > 0) {
            validation.warnings.forEach(w =>
                vscode.window.showInformationMessage(`Memix: ${w}`)
            );
        }

        // Check total brain size before writing
        const currentSize = await redisClient.brainSize(this.prefix);
        const newValueSize = Buffer.byteLength(validation.sanitized || serialized, 'utf8');
        const maxBytes = (vscode.workspace.getConfiguration('memix')
            .get<number>('maxBrainSizeKB') || MAX_BRAIN_SIZE_KB) * 1024;

        if (currentSize + newValueSize > maxBytes) {
            vscode.window.showWarningMessage(
                `Memix: Brain approaching size limit (${Math.round(currentSize / 1024)}KB / ${Math.round(maxBytes / 1024)}KB). Consider pruning.`
            );
        }

        await redisClient.brainSet(this.prefix, key, validation.sanitized || serialized);

        // Log the write
        this.writeLog.push({
            key,
            timestamp: Date.now(),
            sizeBytes: newValueSize
        });

        // Update meta
        await this.updateMeta();

        return { success: true, errors: [] };
    }

    // --- DELETE ---
    async delete(key: string): Promise<void> {
        await redisClient.brainDel(this.prefix, key);
    }

    // --- GET ALL ---
    async getAll(): Promise<Record<string, any>> {
        const raw = await redisClient.brainGetAll(this.prefix);
        const parsed: Record<string, any> = {};
        for (const [k, v] of Object.entries(raw)) {
            try { parsed[k] = JSON.parse(v); }
            catch { parsed[k] = v; }
        }
        return parsed;
    }

    // --- CLEAR ALL ---
    async clearAll(): Promise<void> {
        const keys = await redisClient.brainKeys(this.prefix);
        for (const key of keys) {
            await redisClient.brainDel(this.prefix, key);
        }
    }

    // --- SIZE ---
    async getSize(): Promise<{ totalBytes: number; keys: Record<string, number> }> {
        const raw = await redisClient.brainGetAll(this.prefix);
        const keys: Record<string, number> = {};
        let totalBytes = 0;
        for (const [k, v] of Object.entries(raw)) {
            const size = Buffer.byteLength(v, 'utf8');
            keys[k] = size;
            totalBytes += size;
        }
        return { totalBytes, keys };
    }

    // --- EXISTS ---
    async exists(): Promise<boolean> {
        const identity = await redisClient.brainGet(this.prefix, BRAIN_KEYS.IDENTITY);
        return identity !== null;
    }

    // --- INIT ---
    async init(projectId?: string): Promise<void> {
        // Initialize required keys with valid empty state if they don't exist
        const hasIdentity = await this.get(BRAIN_KEYS.IDENTITY);
        if (!hasIdentity) {
            await this.set(BRAIN_KEYS.IDENTITY, {
                name: projectId || this.projectId,
                purpose: 'Memix brain for project ' + (projectId || this.projectId),
                tech_stack: [],
                core_objectives: [],
                boundaries: []
            });
        }

        const hasSession = await this.get(BRAIN_KEYS.SESSION_STATE);
        if (!hasSession) {
            await this.set(BRAIN_KEYS.SESSION_STATE, {
                current_task: 'Initialized Memix',
                last_updated: new Date().toISOString(),
                session_number: 1
            });
        }

        const hasPatterns = await this.get(BRAIN_KEYS.PATTERNS);
        if (!hasPatterns) {
            await this.set(BRAIN_KEYS.PATTERNS, {
                files_frequently_edited_together: [],
                architectural_rules: [],
                user_preferences: {}
            });
        }

        const hasTasks = await this.get(BRAIN_KEYS.TASKS);
        if (!hasTasks) {
            await this.set(BRAIN_KEYS.TASKS, {
                current_list: null,
                lists: []
            });
        }
    }

    // --- META ---
    private async updateMeta(): Promise<void> {
        const size = await redisClient.brainSize(this.prefix);
        const existingMeta = await this.get(BRAIN_KEYS.META);

        const meta: BrainMeta = {
            projectId: this.projectId,
            createdAt: existingMeta?.createdAt || new Date().toISOString(),
            lastAccessed: new Date().toISOString(),
            totalSessions: existingMeta?.totalSessions || 0,
            brainVersion: '1.0.0',
            sizeBytes: size
        };

        // Direct set without validation loop
        await redisClient.brainSet(
            this.prefix,
            BRAIN_KEYS.META,
            JSON.stringify(meta)
        );
    }

    // --- CHECKPOINT ---
    async createCheckpoint(name: string): Promise<void> {
        const allData = await redisClient.brainGetAll(this.prefix);
        const checkpoint = {
            name,
            timestamp: new Date().toISOString(),
            data: allData
        };
        await redisClient.brainSet(
            this.prefix,
            `${BRAIN_KEYS.CHECKPOINTS}:${name}`,
            JSON.stringify(checkpoint)
        );
        vscode.window.showInformationMessage(`Memix: Checkpoint "${name}" created`);
    }

    async restoreCheckpoint(name: string): Promise<boolean> {
        const raw = await redisClient.brainGet(
            this.prefix,
            `${BRAIN_KEYS.CHECKPOINTS}:${name}`
        );
        if (!raw) { return false; }

        const checkpoint = JSON.parse(raw);
        for (const [key, value] of Object.entries(checkpoint.data as Record<string, string>)) {
            const shortKey = key.replace(`${this.prefix}:`, '');
            await redisClient.brainSet(this.prefix, shortKey, value);
        }
        return true;
    }

    getWriteLog() {
        return this.writeLog;
    }
}