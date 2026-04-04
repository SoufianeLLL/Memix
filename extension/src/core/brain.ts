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

    /// Update projectId when switching workspaces (multi-tenant)
    setProjectId(projectId: string): void {
        this.projectId = projectId;
        this.writeLog = []; // Reset write log for new workspace
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
    // Returns which keys were written and which already existed.
    // Does ONE read to determine state, then writes all missing keys
    // in parallel, then updates meta ONCE at the end.
    async init(projectId?: string): Promise<{ written: string[]; skipped: string[] }> {
        if (projectId) {
            this.projectId = projectId;
        }

        // Single read — determines exactly what exists and what is missing.
        // All subsequent decisions are made from this snapshot with zero extra reads.
        const existing = await this.getAll();

        const isAbsent = (key: string): boolean => {
            const v = existing[key];
            return v === null || v === undefined || v === 'null' ||
                (typeof v === 'string' && v.trim() === '') ||
                (typeof v === 'string' && v.trim() === 'null');
        };

        const now = new Date().toISOString();

        // All brain keys with their default values.
        // Add new required keys here — init handles them automatically.
        const defaults: Array<{ key: string; value: any; kind: MemoryKind }> = [
            {
                key: BRAIN_KEYS.IDENTITY,
                kind: MemoryKind.Context,
                value: {
                    name: this.projectId,
                    purpose: 'Memix brain for project ' + this.projectId,
                    tech_stack: [],
                    core_objectives: [],
                    boundaries: []
                }
            },
            {
                key: BRAIN_KEYS.SESSION_STATE,
                kind: MemoryKind.Context,
                value: {
                    current_task: 'Initialized Memix',
                    last_updated: now,
                    session_number: 1
                }
            },
            {
                key: BRAIN_KEYS.PATTERNS,
                kind: MemoryKind.Context,
                value: {
                    files_frequently_edited_together: [],
                    architectural_rules: [],
                    user_preferences: {}
                }
            },
            {
                key: BRAIN_KEYS.TASKS,
                kind: MemoryKind.Context,
                value: { current_list: null, lists: [] }
            },
            {
                key: BRAIN_KEYS.DECISIONS,
                kind: MemoryKind.Decision,
                value: []
            },
            {
                key: BRAIN_KEYS.FILE_MAP,
                kind: MemoryKind.Context,
                value: {}
            },
            {
                key: BRAIN_KEYS.KNOWN_ISSUES,
                kind: MemoryKind.Context,
                value: []
            },
            {
                key: BRAIN_KEYS.SESSION_LOG,
                kind: MemoryKind.Context,
                value: []
            },
        ];

        const toWrite = defaults.filter(d => isAbsent(d.key));
        const skipped = defaults.filter(d => !isAbsent(d.key)).map(d => d.key);

        if (toWrite.length === 0) {
            return { written: [], skipped };
        }

        // Build entries directly — bypass set() to avoid per-write updateMeta() calls.
        // Validation is intentionally skipped for init defaults because they are
        // structurally correct by construction.
        const entries: MemoryEntry[] = toWrite.map(({ key, value, kind }) => ({
            id: key,
            project_id: this.projectId,
            kind,
            content: JSON.stringify(value),
            tags: [key.split(':')[0]],
            source: MemorySource.UserManual,
            superseded_by: null,
            contradicts: [],
            created_at: now,
            updated_at: now,
            access_count: 0,
            last_accessed_at: null
        }));

        // Write all missing entries in parallel — N HTTP calls to the daemon,
        // which are all fast Unix socket operations. No sequential bottleneck.
        const results = await Promise.allSettled(
            entries.map(entry => MemoryClient.upsertMemory(this.projectId, entry))
        );

        const failures = results
            .map((r, i) => ({ r, key: toWrite[i].key }))
            .filter(({ r }) => r.status === 'rejected');

        if (failures.length > 0) {
            const detail = failures
                .map(({ r, key }) => `${key}: ${(r as PromiseRejectedResult).reason?.message ?? 'unknown'}`)
                .join(', ');
            throw new Error(`Brain init: ${failures.length} key(s) failed to write — ${detail}`);
        }

        // Meta written once — after all entries are confirmed written.
        await this.updateMeta();

        return { written: toWrite.map(d => d.key), skipped };
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
            brainVersion: '1.8.0',
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