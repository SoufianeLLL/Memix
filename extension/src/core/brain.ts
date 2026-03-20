import * as vscode from 'vscode';
import { validateBrainWrite } from './validator';
import { BRAIN_KEYS, BrainMeta, MAX_BRAIN_SIZE_KB } from '../utils/constants';
import { MemoryClient, MemoryEntry, MemoryKind, MemorySource } from '../client';

export class BrainManager {
    private projectId: string;
    private writeLog: { key: string; timestamp: number; sizeBytes: number }[] = [];

    constructor(projectId: string) {
        this.projectId = projectId;
    }

    getProjectId(): string {
        return this.projectId;
    }

    getPrefix(): string {
        return `brain:${this.projectId}`;
    }

    // --- READ ---
    async get(key: string): Promise<any | null> {
        try {
            const memory = await MemoryClient.getMemory(this.projectId);
            const entry = memory.find(e => e.id === key);
            if (!entry) return null;

            try {
                return JSON.parse(entry.content);
            } catch {
                return entry.content;
            }
        } catch {
            return null;
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

        const isDecision = key === BRAIN_KEYS.DECISIONS;
        const entry: MemoryEntry = {
            id: key,
            project_id: this.projectId,
            kind: isDecision ? MemoryKind.Decision : MemoryKind.Context,
            content: validation.sanitized || serialized,
            tags: [key.split(':')[0]],
            source: MemorySource.UserManual,
            superseded_by: null,
            contradicts: [],
            created_at: new Date().toISOString(),
            updated_at: new Date().toISOString(),
            access_count: 0,
            last_accessed_at: null
        };

        try {
            await MemoryClient.upsertMemory(this.projectId, entry);

            const newValueSize = Buffer.byteLength(entry.content, 'utf8');
            this.writeLog.push({
                key,
                timestamp: Date.now(),
                sizeBytes: newValueSize
            });

            await this.updateMeta();
            return { success: true, errors: [] };
        } catch (err: any) {
            console.error(`BrainManager set error: ${err.message}`);
            return { success: false, errors: [err.message] };
        }
    }

    // --- DELETE ---
    async delete(key: string): Promise<void> {
        // MVP: Soft delete or ignoring delete for now as daemon upsert is prioritized
        // A dedicated DELETE endpoint would be needed in the daemon for this.
        await this.set(key, null);
    }

    // --- GET ALL ---
    async getAll(): Promise<Record<string, any>> {
        const parsed: Record<string, any> = {};
        const memory = await MemoryClient.getMemory(this.projectId);
        for (const entry of memory) {
            try {
                parsed[entry.id] = JSON.parse(entry.content);
            } catch {
                parsed[entry.id] = entry.content;
            }
        }
        return parsed;
    }

    // --- CLEAR ALL ---
    async clearAll(): Promise<void> {
        await MemoryClient.purgeProject(this.projectId);
    }

    // --- SIZE ---
    async getSize(): Promise<{ totalBytes: number; keys: Record<string, number> }> {
        const all = await this.getAll();
        const keys: Record<string, number> = {};
        let totalBytes = 0;

        for (const [k, v] of Object.entries(all)) {
            const strValue = typeof v === 'string' ? v : JSON.stringify(v);
            const size = Buffer.byteLength(strValue || '', 'utf8');
            keys[k] = size;
            totalBytes += size;
        }
        return { totalBytes, keys };
    }

    // --- EXISTS ---
    async exists(): Promise<boolean> {
        const all = await this.getAll();
        return (BRAIN_KEYS.IDENTITY in all) && (BRAIN_KEYS.SESSION_STATE in all) && (BRAIN_KEYS.PATTERNS in all);
    }

    // --- INIT ---
    async init(projectId?: string): Promise<void> {
        if (projectId) {
            this.projectId = projectId;
        }

        const hasIdentity = await this.get(BRAIN_KEYS.IDENTITY);
        if (!hasIdentity || hasIdentity === 'null') {
            await this.set(BRAIN_KEYS.IDENTITY, {
                name: this.projectId,
                purpose: 'Memix brain for project ' + this.projectId,
                tech_stack: [],
                core_objectives: [],
                boundaries: []
            });
        }

        const hasSession = await this.get(BRAIN_KEYS.SESSION_STATE);
        if (!hasSession || hasSession === 'null') {
            await this.set(BRAIN_KEYS.SESSION_STATE, {
                current_task: 'Initialized Memix',
                last_updated: new Date().toISOString(),
                session_number: 1
            });
        }

        const hasPatterns = await this.get(BRAIN_KEYS.PATTERNS);
        if (!hasPatterns || hasPatterns === 'null') {
            await this.set(BRAIN_KEYS.PATTERNS, {
                files_frequently_edited_together: [],
                architectural_rules: [],
                user_preferences: {}
            });
        }

        const hasTasks = await this.get(BRAIN_KEYS.TASKS);
        if (!hasTasks || hasTasks === 'null') {
            await this.set(BRAIN_KEYS.TASKS, {
                current_list: null,
                lists: []
            });
        }
    }

    // --- META ---
    private async updateMeta(): Promise<void> {
        const sizeInfo = await this.getSize();
        const existingMeta = await this.get(BRAIN_KEYS.META);

        const meta: BrainMeta = {
            projectId: this.projectId,
            createdAt: existingMeta?.createdAt || new Date().toISOString(),
            lastAccessed: new Date().toISOString(),
            totalSessions: existingMeta?.totalSessions || 0,
            brainVersion: '1.0.6-beta',
            sizeBytes: sizeInfo.totalBytes
        };

        const entry: MemoryEntry = {
            id: BRAIN_KEYS.META,
            project_id: this.projectId,
            kind: MemoryKind.Fact,
            content: JSON.stringify(meta),
            tags: ['meta', 'system'],
            source: MemorySource.UserManual,
            superseded_by: null,
            contradicts: [],
            created_at: existingMeta?.createdAt || new Date().toISOString(),
            updated_at: new Date().toISOString(),
            access_count: 0,
            last_accessed_at: null
        };
        await MemoryClient.upsertMemory(this.projectId, entry);
    }

    // --- CHECKPOINT ---
    async createCheckpoint(name: string): Promise<void> {
        const allData = await this.getAll();
        const checkpoint = {
            name,
            timestamp: new Date().toISOString(),
            data: allData
        };
        await this.set(`${BRAIN_KEYS.CHECKPOINTS}:${name}`, checkpoint);
        vscode.window.showInformationMessage(`Memix: Checkpoint "${name}" created via Daemon`);
    }

    async restoreCheckpoint(name: string): Promise<boolean> {
        const checkpoint = await this.get(`${BRAIN_KEYS.CHECKPOINTS}:${name}`);
        if (!checkpoint || !checkpoint.data) { return false; }

        for (const [key, value] of Object.entries(checkpoint.data as Record<string, string>)) {
            await this.set(key, value);
        }
        return true;
    }

    getWriteLog() {
        return this.writeLog;
    }
}