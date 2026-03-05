import * as vscode from 'vscode';
import * as fs from 'fs';
import * as path from 'path';
import { BrainManager } from './brain';
import { BRAIN_KEYS } from '../utils/constants';

/**
 * Maps brain Redis keys to local filenames.
 * The AI reads/writes these local files; BrainSync handles Redis sync.
 */
const KEY_FILE_MAP: Record<string, string> = {
    [BRAIN_KEYS.IDENTITY]: 'identity.json',
    [BRAIN_KEYS.SESSION_STATE]: 'session_state.json',
    [BRAIN_KEYS.SESSION_LOG]: 'session_log.json',
    [BRAIN_KEYS.DECISIONS]: 'decisions.json',
    [BRAIN_KEYS.PATTERNS]: 'patterns.json',
    [BRAIN_KEYS.FILE_MAP]: 'file_map.json',
    [BRAIN_KEYS.KNOWN_ISSUES]: 'known_issues.json',
    [BRAIN_KEYS.TASKS]: 'tasks.json',
};

/**
 * BrainSync mirrors brain data between Redis and local `.memix/brain/` files.
 * 
 * Flow:
 * 1. Extension connects to Redis → pullFromRedis() downloads all keys to local files
 * 2. AI reads/writes local JSON files during conversations
 * 3. FileSystemWatcher detects changes → pushFileToRedis() syncs back through BrainManager
 * 4. All writes go through BrainManager.set() which includes validation + secret detection
 */
export class BrainSync {
    private brain: BrainManager;
    private brainDir: string;
    private watcher: vscode.FileSystemWatcher | null = null;
    private isSyncing = false;  // Prevents circular sync loops

    constructor(brain: BrainManager, workspaceRoot: string) {
        this.brain = brain;
        this.brainDir = path.join(workspaceRoot, '.memix', 'brain');
    }

    /**
     * Returns the absolute path to the .memix/brain directory.
     */
    getBrainDir(): string {
        return this.brainDir;
    }

    /**
     * Pull all brain keys from Redis and write them to local JSON files.
     * Call this after connecting to Redis or after brain.init().
     */
    async pullFromRedis(): Promise<void> {
        // Ensure directory exists
        if (!fs.existsSync(this.brainDir)) {
            fs.mkdirSync(this.brainDir, { recursive: true });
        }

        this.isSyncing = true;
        try {
            const allData = await this.brain.getAll();

            for (const [brainKey, filename] of Object.entries(KEY_FILE_MAP)) {
                const filePath = path.join(this.brainDir, filename);
                const value = allData[brainKey];

                if (value !== undefined && value !== null) {
                    const json = typeof value === 'string' ? value : JSON.stringify(value, null, 2);
                    // Pretty-print if it's valid JSON
                    let formatted: string;
                    try {
                        formatted = JSON.stringify(JSON.parse(json), null, 2);
                    } catch {
                        formatted = json;
                    }
                    fs.writeFileSync(filePath, formatted, 'utf8');
                } else {
                    // Write empty placeholder so the AI knows the key exists
                    if (!fs.existsSync(filePath)) {
                        fs.writeFileSync(filePath, '{}', 'utf8');
                    }
                }
            }
        } finally {
            // Small delay to let the watcher ignore our own writes
            setTimeout(() => { this.isSyncing = false; }, 500);
        }
    }

    /**
     * Push a single local file back to Redis through BrainManager.set()
     * (includes validation, secret detection, and size checks).
     */
    async pushFileToRedis(filename: string): Promise<void> {
        // Find the brain key for this filename
        const brainKey = Object.entries(KEY_FILE_MAP)
            .find(([, f]) => f === filename)?.[0];

        if (!brainKey) {
            return; // Not a recognized brain file, ignore
        }

        const filePath = path.join(this.brainDir, filename);
        if (!fs.existsSync(filePath)) { return; }

        try {
            const raw = fs.readFileSync(filePath, 'utf8').trim();
            if (!raw) { return; }

            // Parse to validate it's proper JSON
            const parsed = JSON.parse(raw);

            // Write through BrainManager (validation + secret detection)
            const result = await this.brain.set(brainKey, parsed);
            if (!result.success) {
                vscode.window.showWarningMessage(
                    `Memix Sync: Failed to save ${filename} → ${result.errors.join(', ')}`
                );
            }
        } catch (err: any) {
            // JSON parse error — don't sync malformed files
            vscode.window.showWarningMessage(
                `Memix Sync: ${filename} contains invalid JSON. Fix the file and save again.`
            );
        }
    }

    /**
     * Start watching .memix/brain/ for file changes.
     * Any save by the AI triggers a push to Redis.
     */
    startWatcher(): void {
        if (this.watcher) { return; } // Already watching

        const pattern = new vscode.RelativePattern(this.brainDir, '*.json');
        this.watcher = vscode.workspace.createFileSystemWatcher(pattern);

        this.watcher.onDidChange((uri) => {
            if (this.isSyncing) { return; } // Ignore our own writes
            const filename = path.basename(uri.fsPath);
            this.pushFileToRedis(filename);
        });

        this.watcher.onDidCreate((uri) => {
            if (this.isSyncing) { return; }
            const filename = path.basename(uri.fsPath);
            this.pushFileToRedis(filename);
        });
    }

    /**
     * Stop watching and clean up.
     */
    stopWatcher(): void {
        if (this.watcher) {
            this.watcher.dispose();
            this.watcher = null;
        }
    }
}
