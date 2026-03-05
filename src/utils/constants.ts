export const BRAIN_KEYS = {
    IDENTITY: 'identity',
    SESSION_STATE: 'session:state',
    SESSION_LOG: 'session:log',
    DECISIONS: 'decisions',
    PATTERNS: 'patterns',
    FILE_MAP: 'file_map',
    KNOWN_ISSUES: 'known_issues',
    TASKS: 'tasks',
    CHECKPOINTS: 'checkpoints',
    TEAM_STATE: 'team:state',
    SCORING: 'scoring',
    META: 'meta'
} as const;

export const MEMORY_TAXONOMY = {
    SEMANTIC: 'semantic',       // What the project IS (identity, architecture)
    EPISODIC: 'episodic',       // What HAPPENED (session logs, decisions)
    PROCEDURAL: 'procedural',   // HOW to do things (patterns, conventions)
    WORKING: 'working',         // Current task state (session:state)
    DIAGNOSTIC: 'diagnostic'    // Issues, health, conflicts (known_issues)
} as const;

export const TAXONOMY_MAP: Record<string, string> = {
    [BRAIN_KEYS.IDENTITY]: MEMORY_TAXONOMY.SEMANTIC,
    [BRAIN_KEYS.SESSION_STATE]: MEMORY_TAXONOMY.WORKING,
    [BRAIN_KEYS.SESSION_LOG]: MEMORY_TAXONOMY.EPISODIC,
    [BRAIN_KEYS.DECISIONS]: MEMORY_TAXONOMY.EPISODIC,
    [BRAIN_KEYS.PATTERNS]: MEMORY_TAXONOMY.PROCEDURAL,
    [BRAIN_KEYS.FILE_MAP]: MEMORY_TAXONOMY.SEMANTIC,
    [BRAIN_KEYS.KNOWN_ISSUES]: MEMORY_TAXONOMY.DIAGNOSTIC,
    [BRAIN_KEYS.TASKS]: MEMORY_TAXONOMY.WORKING,
};

export const MAX_KEY_SIZE_BYTES = 4096;
export const MAX_BRAIN_SIZE_KB = 512;
export const MAX_SESSION_LOG_ENTRIES = 30;
export const MAX_TOKEN_BUDGET = 4000;

export interface BrainMeta {
    projectId: string;
    createdAt: string;
    lastAccessed: string;
    totalSessions: number;
    brainVersion: string;
    teamId?: string;
    sizeBytes: number;
}

export interface SessionState {
    last_updated: string;
    session_number: number;
    current_task: string;
    progress: string[];
    blockers: string[];
    next_steps: string[];
    modified_files: string[];
    important_context: string;
}

export interface SessionLogEntry {
    session: number;
    date: string;
    summary: string;
    files_changed: string[];
    score?: SessionScore;
}

export interface SessionScore {
    tasks_completed: number;
    tasks_started: number;
    bugs_found: number;
    bugs_fixed: number;
    decisions_made: number;
    files_modified: number;
    brain_updates: number;
}

export interface HealthReport {
    status: 'healthy' | 'warning' | 'critical';
    timestamp: string;
    checks: HealthCheck[];
    totalSizeBytes: number;
    recommendations: string[];
}

export interface HealthCheck {
    key: string;
    exists: boolean;
    sizeBytes: number;
    valid: boolean;
    taxonomy: string;
    lastUpdated?: string;
    issues: string[];
}

export interface ConflictReport {
    key: string;
    brainValue: string;
    actualValue: string;
    recommendation: string;
    autoResolvable: boolean;
}