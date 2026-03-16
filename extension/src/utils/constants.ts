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

export type BrainKeyTier = 'required' | 'recommended' | 'generated' | 'system';

export interface BrainKeySpec {
	label: string;
	tier: BrainKeyTier;
	description: string;
	fixStrategy: 'init' | 'generated' | 'manual';
}

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

export const BRAIN_KEY_SPECS: Record<string, BrainKeySpec> = {
	[BRAIN_KEYS.IDENTITY]: {
		label: 'Identity',
		tier: 'required',
		description: 'Project identity, purpose, and tech stack.',
		fixStrategy: 'init'
	},
	[BRAIN_KEYS.SESSION_STATE]: {
		label: 'Session State',
		tier: 'required',
		description: 'Current task, progress, blockers, and next steps.',
		fixStrategy: 'init'
	},
	[BRAIN_KEYS.PATTERNS]: {
		label: 'Patterns',
		tier: 'required',
		description: 'Project conventions, architecture rules, and preferences.',
		fixStrategy: 'init'
	},
	[BRAIN_KEYS.TASKS]: {
		label: 'Tasks',
		tier: 'recommended',
		description: 'Persistent task tracking for active workstreams.',
		fixStrategy: 'init'
	},
	[BRAIN_KEYS.DECISIONS]: {
		label: 'Decisions',
		tier: 'recommended',
		description: 'Key architectural and implementation decisions.',
		fixStrategy: 'manual'
	},
	[BRAIN_KEYS.FILE_MAP]: {
		label: 'File Map',
		tier: 'generated',
		description: 'Daemon-generated map of important files and their roles.',
		fixStrategy: 'generated'
	},
	[BRAIN_KEYS.KNOWN_ISSUES]: {
		label: 'Known Issues',
		tier: 'generated',
		description: 'Warnings, risks, and known technical issues.',
		fixStrategy: 'generated'
	},
	[BRAIN_KEYS.SESSION_LOG]: {
		label: 'Session Log',
		tier: 'generated',
		description: 'Historical session summaries appended over time.',
		fixStrategy: 'generated'
	},
	[BRAIN_KEYS.CHECKPOINTS]: {
		label: 'Checkpoints',
		tier: 'system',
		description: 'Optional recovery checkpoints created on demand.',
		fixStrategy: 'manual'
	},
	[BRAIN_KEYS.TEAM_STATE]: {
		label: 'Team State',
		tier: 'system',
		description: 'Shared/team synchronization metadata.',
		fixStrategy: 'manual'
	},
	[BRAIN_KEYS.SCORING]: {
		label: 'Scoring',
		tier: 'system',
		description: 'Optional scoring metadata for future intelligence features.',
		fixStrategy: 'manual'
	},
	[BRAIN_KEYS.META]: {
		label: 'Meta',
		tier: 'system',
		description: 'Brain metadata such as size and version.',
		fixStrategy: 'generated'
	}
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