import * as http from 'http';
import * as os from 'os';
import * as path from 'path';

const IS_WINDOWS = process.platform === 'win32';
const DAEMON_TCP_PORT = parseInt(process.env.MEMIX_PORT || '3456', 10);
let SOCKET_PATH = path.join(os.homedir(), '.memix', 'daemon.sock');
const API_PREFIX = '/api/v1';

let BASE_URL: string | null = null;

export interface RedisStats {
	used_bytes: number;
	max_bytes: number | null;
}

function getRequestOptions(method: string, requestPath: string): http.RequestOptions {
	if (BASE_URL) {
		const url = new URL(requestPath, BASE_URL);
		return {
			hostname: url.hostname,
			port: url.port ? Number(url.port) : 80,
			path: url.pathname + url.search,
			method
		};
	}
	// Windows: TCP fallback since Unix sockets aren't available
	if (IS_WINDOWS) {
		return {
			hostname: '127.0.0.1',
			port: DAEMON_TCP_PORT,
			path: requestPath,
			method,
		};
	}
	return {
		socketPath: SOCKET_PATH,
		path: requestPath,
		method
	};
}

function readResponseBody(res: http.IncomingMessage): Promise<string> {
	return new Promise((resolve) => {
		let data = '';
		res.on('data', chunk => data += chunk);
		res.on('end', () => resolve(data));
		res.on('error', () => resolve(data));
	});
}

async function buildDaemonError(res: http.IncomingMessage, requestPath: string): Promise<Error> {
	const body = await readResponseBody(res);
	const status = res.statusCode ?? 0;
	const trimmed = (body || '').trim();
	const suffix = trimmed ? `\n${trimmed}` : '';
	return new Error(`Daemon returned status: ${status} for ${requestPath}${suffix}`);
}

export enum MemoryKind {
	Fact = 'fact',
	Decision = 'decision',
	Warning = 'warning',
	Pattern = 'pattern',
	Context = 'context'
}

export enum MemorySource {
	UserManual = 'user_manual',
	AgentExtracted = 'agent_extracted',
	FileWatcher = 'file_watcher',
	GitArchaeology = 'git_archaeology'
}

export interface MemoryEntry {
	id: string;
	project_id: string;
	kind: MemoryKind;
	content: string;
	tags: string[];
	source: MemorySource;
	superseded_by: string | null;
	contradicts: string[];
	parent_id?: string | null;
	caused_by?: string[];
	enables?: string[];
	created_at: string;
	updated_at: string;
	access_count: number;
	last_accessed_at: string | null;
}

export interface ObserverDna {
	indexed_files: number;
	functions_indexed: number;
	architecture: string;
	complexity_score: number;
	dominant_patterns: string[];
	hot_zones: string[];
	stable_zones: string[];
	dependency_depth: number;
	circular_risks: string[];
	type_coverage: number;
	error_handling: string;
	test_coverage_estimate: number;
	active_development_areas: string[];
	stale_areas: string[];
	explainability_summary: string;
	language_breakdown: Record<string, number>;
	rules_source: string | null;
	applied_rule_ids: string[];
}

export interface ObserverDnaOtelAttribute {
	key: string;
	value: string;
}

export interface ObserverDnaOtelEvent {
	name: string;
	attributes: ObserverDnaOtelAttribute[];
}

export interface ObserverDnaOtelExport {
	schema_url: string;
	resource_attributes: ObserverDnaOtelAttribute[];
	events: ObserverDnaOtelEvent[];
}

export interface ObserverIntentSnapshot {
	active_file: string;
	intent_type: string;
	confidence: number;
	related_files: string[];
	preloaded_memory_ids: string[];
	token_weight: number;
	updated_at_ms: number;
	rationale: string[];
}

export interface ObserverGitTouchPoint {
	commit_id: string;
	author: string;
	summary: string;
	touched_at_unix: number;
}

export interface ObserverGitFileInsight {
	file_path: string;
	churn_commits: number;
	last_touch: ObserverGitTouchPoint | null;
}

export interface ObserverGitInsights {
	available: boolean;
	repo_root: string | null;
	hot_files: ObserverGitFileInsight[];
	stable_files: ObserverGitFileInsight[];
	recent_authors: string[];
	summary: string[];
}

export interface AgentConfig {
	name: string;
	trigger: string | { Interval?: { seconds: number } } | Record<string, unknown>;
	scope: string;
	action_description: string;
	output_key: string;
	cooldown_ms: number;
	source_path?: string | null;
}

export interface AgentNotification {
	title: string;
	message: string;
}

export interface AgentReport {
	agent_name: string;
	entry_id: string;
	output_key: string;
	severity: 'Info' | 'Warning' | 'Critical' | string;
	notifications: AgentNotification[];
	data: Record<string, unknown>;
	generated_at: string;
}

export interface AgentConfigResponse {
	source_path: string | null;
	configs: AgentConfig[];
}

export interface AgentReportsResponse {
	reports: AgentReport[];
}

export interface CompiledContextSection {
	id: string;
	kind: string;
	priority: number;
	tokens: number;
	content: string;
}

export interface CompiledContextMetrics {
	relevant_files: number;
	skeletons_built: number;
	deduplicated_files: number;
	history_sections: number;
	rules_sections: number;
	ranked_sections: number;
	fitted_sections: number;
}

export interface CompiledContext {
	budget: number;
	total_tokens: number;
	explainability_summary: string;
	selected_sections: CompiledContextSection[];
	omitted_section_ids: string[];
	metrics: CompiledContextMetrics;
}

export interface ProactiveRiskWarning {
	file: string;
	risk_score: number;
	dependents: number;
	past_breaks: string[];
	known_issues: string[];
	stable_days_signal: boolean;
	recommendation: string;
}

export interface BlastRadiusAffectedFile {
	path: string;
	depth: number;
	via: string;
}

export interface BlastRadius {
	source: string;
	affected_count: number;
	affected_files: BlastRadiusAffectedFile[];
	critical_path: string[];
	max_depth: number;
}

export interface ProactiveRiskResponse {
	warning: ProactiveRiskWarning | null;
	blast_radius: BlastRadius | null;
}

export interface ImportanceResponse {
	top_files: Array<[string, number]>;
	scc_groups: string[][];
	circular_files: string[];
	node_count: number;
	cycle_count: number;
	topological_order_length: number;
	betweenness: Record<string, number>;
	pagerank: Record<string, number>;
}

export interface ResolvedCallEdge {
	callee_file: string;
	callee_symbol: string;
	callee_line: number;
	is_method: boolean;
}

export interface CallerSite {
	caller_file: string;
	caller_symbol: string;
	call_line: number;
	is_method: boolean;
}

export interface SymbolCausalContext {
	symbol: string;
	calls: ResolvedCallEdge[];
	called_by: CallerSite[];
}

export interface FileCausalContext {
	file: string;
	symbols: SymbolCausalContext[];
	total_outgoing_edges: number;
	total_incoming_edges: number;
}

export interface PromptContextSection {
	section_name: string;
	tokens: number;
}

export interface PromptOptimizationSuggestion {
	task_type: string;
	always_include: string[];
	consider_excluding: string[];
	recommended_budget: number;
}

export interface TaskModelPerformance {
	first_try_rate: number;
	avg_tokens: number;
	runs: number;
}

export interface ModelPerformanceReport {
	model_performance: Record<string, Record<string, TaskModelPerformance>>;
}

export interface DeveloperProfile {
	universal_patterns: string[];
	preferred_stack: string[];
	code_style: string[];
}

export interface HierarchyResolution {
	entry_id: string;
	resolved_from: string[];
	value: unknown;
}

export interface LicenseStatusResponse {
	available: boolean;
	active: boolean;
	tier?: 'solo' | 'pro' | null;
	email?: string | null;
	seats?: number | null;
	expires_at?: number | null;
	mode?: string | null;
	message?: string | null;
	grace_until?: number | null;
}

export interface LicenseInitiateResponse {
	license_exists: boolean;
	token?: string | null;
	checkout_url?: string | null;
	key?: string | null;
	message?: string | null;
}

export interface LicensePendingResponse {
	ready: boolean;
	key?: string | null;
	message?: string | null;
}

export type LicenseBillingInterval = 'monthly' | 'yearly';

export interface TokenStatsResponse {
	session: {
		ai_tokens_consumed: number;
		context_tokens_compiled: number;
		estimated_tokens_saved: number;
		files_skeleton_indexed: number;
		embedding_cache_hits: number;
		embedding_cache_misses: number;
	};
	lifetime: {
		ai_tokens_consumed: number;
		context_tokens_compiled: number;
		estimated_tokens_saved: number;
		files_skeleton_indexed: number;
	};
	cache_efficiency_pct: number;
	compression_ratio: number;
}

export class MemoryClient {
	static setBaseUrl(url: string | null) {
		BASE_URL = url;
	}

	static setSocketPath(socketPath: string) {
		SOCKET_PATH = socketPath;
	}

	static async redisPing(redisUrl: string): Promise<void> {
		return new Promise((resolve, reject) => {
			const payload = JSON.stringify({ redis_url: redisUrl });
			const requestPath = `${API_PREFIX}/redis/ping`;
			const options: http.RequestOptions = {
				...getRequestOptions('POST', requestPath),
				headers: {
					'Content-Type': 'application/json',
					'Content-Length': Buffer.byteLength(payload)
				}
			};
			const req = http.request(options, (res) => {
				readResponseBody(res).then((data) => {
					if (res.statusCode !== 200) {
						buildDaemonError(res, requestPath).then(reject, reject);
						return;
					}
					try {
						const parsed = JSON.parse(data || '{}');
						if (parsed && parsed.ok === true) resolve();
						else reject(new Error('Redis ping failed'));
					} catch (e) {
						reject(e);
					}
				}, reject);
			});
			req.on('error', reject);
			req.write(payload);
			req.end();
		});
	}

	static async initiateLicense(email: string): Promise<LicenseInitiateResponse> {
		return new Promise((resolve, reject) => {
			const payload = JSON.stringify({ email });
			const requestPath = `${API_PREFIX}/license/initiate`;
			const options: http.RequestOptions = {
				...getRequestOptions('POST', requestPath),
				timeout: 15000,
				headers: {
					'Content-Type': 'application/json',
					'Content-Length': Buffer.byteLength(payload)
				}
			};
			const req = http.request(options, (res) => {
				readResponseBody(res).then((data) => {
					if ((res.statusCode ?? 500) >= 400) {
						try {
							const json = JSON.parse(data || '{}');
							reject(new Error(json.message || 'Failed to initiate license'));
						} catch {
							reject(new Error('Failed to initiate license'));
						}
						return;
					}
					try {
						resolve(JSON.parse(data || '{}'));
					} catch (e) {
						reject(e);
					}
				}, reject);
			});
			req.on('error', reject);
			req.on('timeout', () => {
				req.destroy();
				reject(new Error('License server request timed out'));
			});
			req.write(payload);
			req.end();
		});
	}

	static async getPendingLicense(token: string): Promise<LicensePendingResponse> {
		return new Promise((resolve, reject) => {
			const requestPath = `${API_PREFIX}/license/pending/${encodeURIComponent(token)}`;
			const options: http.RequestOptions = {
				...getRequestOptions('GET', requestPath),
				timeout: 10000,
			};
			const req = http.request(options, (res) => {
				readResponseBody(res).then((data) => {
					if ((res.statusCode ?? 500) >= 400) {
						try {
							const json = JSON.parse(data || '{}');
							reject(new Error(json.message || 'Failed to check license status'));
						} catch {
							reject(new Error('Failed to check license status'));
						}
						return;
					}
					try {
						resolve(JSON.parse(data || '{}'));
					} catch (e) {
						reject(e);
					}
				}, reject);
			});
			req.on('error', reject);
			req.on('timeout', () => {
				req.destroy();
				reject(new Error('License status check timed out'));
			});
			req.end();
		});
	}

	static async getSkeletonStats(projectId: string): Promise<{ project_id: string; fsi_count: number; fusi_count: number; total: number; size_bytes: number; capacity: number } | null> {
		return new Promise((resolve, reject) => {
			const sanitizedProjectId = encodeURIComponent(projectId);
			const requestPath = `${API_PREFIX}/skeleton/stats/${sanitizedProjectId}`;
			const req = http.request(getRequestOptions('GET', requestPath), (res) => {
				if (res.statusCode !== 200) {
					resolve(null);
					return;
				}
				readResponseBody(res).then((data) => {
					try {
						resolve(JSON.parse(data || '{}'));
					} catch (e) {
						resolve(null);
					}
				}, reject);
			});
			req.on('error', reject);
			req.end();
		});
	}

	static async getBrainDbSize(projectId: string): Promise<{ size_bytes: number }> {
		return new Promise((resolve, reject) => {
			const requestPath = `${API_PREFIX}/brain/size/${encodeURIComponent(projectId)}`;
			const options = getRequestOptions('GET', requestPath);
			const req = http.request(options, (res) => {
				if (res.statusCode !== 200) {
					buildDaemonError(res, requestPath).then(reject, reject);
					return;
				}
				readResponseBody(res).then((data) => {
					try {
						resolve(JSON.parse(data || '{}'));
					} catch (e) {
						reject(e);
					}
				}, reject);
			});
			req.on('error', reject);
			req.end();
		});
	}

	static async getRedisStats(): Promise<RedisStats> {
		return new Promise((resolve, reject) => {
			const requestPath = `${API_PREFIX}/redis/stats`;
			const options = getRequestOptions('GET', requestPath);
			const req = http.request(options, (res) => {
				if (res.statusCode !== 200) {
					buildDaemonError(res, requestPath).then(reject, reject);
					return;
				}
				readResponseBody(res).then((data) => {
					try {
						resolve(JSON.parse(data || '{}'));
					} catch (e) {
						reject(e);
					}
				}, reject);
			});
			req.on('error', reject);
			req.end();
		});
	}

	static async exportBrainMirror(projectId: string): Promise<{ written: number }> {
		return new Promise((resolve, reject) => {
			const requestPath = `${API_PREFIX}/brain/export/${encodeURIComponent(projectId)}`;
			const options = getRequestOptions('POST', requestPath);
			const req = http.request(options, (res) => {
				if (res.statusCode !== 200) {
					buildDaemonError(res, requestPath).then(reject, reject);
					return;
				}
				readResponseBody(res).then((data) => {
					try {
						resolve(JSON.parse(data || '{}'));
					} catch (e) {
						reject(e);
					}
				}, reject);
			});
			req.on('error', reject);
			req.end();
		});
	}

	static async importBrainMirror(projectId: string): Promise<{ imported: number }> {
		return new Promise((resolve, reject) => {
			const requestPath = `${API_PREFIX}/brain/import/${encodeURIComponent(projectId)}`;
			const options = getRequestOptions('POST', requestPath);
			const req = http.request(options, (res) => {
				if (res.statusCode !== 200) {
					buildDaemonError(res, requestPath).then(reject, reject);
					return;
				}
				readResponseBody(res).then((data) => {
					try {
						resolve(JSON.parse(data || '{}'));
					} catch (e) {
						reject(e);
					}
				}, reject);
			});
			req.on('error', reject);
			req.end();
		});
	}

	static async migrateProject(projectId: string): Promise<{ migrated_entries: number; schema_version: number }> {
		return new Promise((resolve, reject) => {
			const requestPath = `${API_PREFIX}/brain/migrate/${encodeURIComponent(projectId)}`;
			const options = getRequestOptions('POST', requestPath);
			const req = http.request(options, (res) => {
				if (res.statusCode !== 200) {
					buildDaemonError(res, requestPath).then(reject, reject);
					return;
				}
				readResponseBody(res).then((data) => {
					try {
						resolve(JSON.parse(data || '{}'));
					} catch (e) {
						reject(e);
					}
				}, reject);
			});
			req.on('error', reject);
			req.end();
		});
	}

	/**
	 * Retrieves memory from the Rust Daemon over localhost HTTP
	 */
	static async getMemory(projectId: string, workspaceRoot?: string): Promise<MemoryEntry[]> {
		return new Promise((resolve, reject) => {
			let requestPath = `${API_PREFIX}/memory/${projectId}`;
			if (workspaceRoot) {
				requestPath += `?workspace_root=${encodeURIComponent(workspaceRoot)}`;
			}
			const options = getRequestOptions('GET', requestPath);
			const req = http.request(options, (res) => {
				if (res.statusCode !== 200) {
					buildDaemonError(res, requestPath).then(reject, reject);
					return;
				}

				readResponseBody(res).then((data) => {
					try {
						resolve(JSON.parse(data));
					} catch (e) {
						reject(e);
					}
				}, reject);
			});
			req.on('error', reject);
			req.end();
		});
	}

	/**
	 * Purges an entire project's memory via the Rust Daemon
	 */
	static async purgeProject(projectId: string, workspaceRoot?: string): Promise<void> {
		return new Promise((resolve, reject) => {
			let requestPath = `${API_PREFIX}/memory/${projectId}`;
			if (workspaceRoot) {
				requestPath += `?workspace_root=${encodeURIComponent(workspaceRoot)}`;
			}
			const options: http.RequestOptions = getRequestOptions('DELETE', requestPath);
			const req = http.request(options, (res) => {
				if (res.statusCode === 204) {
					resolve();
				} else {
					buildDaemonError(res, requestPath).then(reject, reject);
				}
			});
			req.on('error', reject);
			req.end();
		});
	}

	/**
	 * Deletes a single memory entry by ID via the Rust Daemon
	 */
	static async deleteMemory(projectId: string, entryId: string): Promise<void> {
		return new Promise((resolve, reject) => {
			const requestPath = `${API_PREFIX}/memory/${projectId}/${encodeURIComponent(entryId)}`;
			const options: http.RequestOptions = getRequestOptions('DELETE', requestPath);
			const req = http.request(options, (res) => {
				if (res.statusCode === 200) {
					resolve();
				} else {
					buildDaemonError(res, requestPath).then(reject, reject);
				}
			});
			req.on('error', reject);
			req.end();
		});
	}

	/**
	 * Upserts a single memory entry via the Rust Daemon over localhost HTTP
	 */
	static async upsertMemory(projectId: string, entry: MemoryEntry, workspaceRoot?: string): Promise<void> {
		return new Promise((resolve, reject) => {
			const data = JSON.stringify(entry);
			let requestPath = `${API_PREFIX}/memory/${projectId}`;
			if (workspaceRoot) {
				requestPath += `?workspace_root=${encodeURIComponent(workspaceRoot)}`;
			}
			const options: http.RequestOptions = {
				...getRequestOptions('POST', requestPath),
				headers: {
					'Content-Type': 'application/json',
					'Content-Length': Buffer.byteLength(data)
				}
			};
			const req = http.request(options, (res) => {
				if (res.statusCode === 201 || res.statusCode === 200) {
					resolve();
				} else {
					buildDaemonError(res, requestPath).then(reject, reject);
				}
			});

			req.on('error', reject);
			req.write(data);
			req.end();
		});
	}

	/**
	 * Searches memory semantically via the Rust Daemon
	 */
	static async searchMemory(projectId: string, query: string): Promise<MemoryEntry[]> {
		return new Promise((resolve, reject) => {
			const encodedQuery = encodeURIComponent(query);
			const requestPath = `${API_PREFIX}/memory/${projectId}/search?q=${encodedQuery}`;
			const options = getRequestOptions('GET', requestPath);
			const req = http.request(options, (res) => {
				if (res.statusCode !== 200) {
					buildDaemonError(res, requestPath).then(reject, reject);
					return;
				}

				readResponseBody(res).then((data) => {
					try {
						resolve(JSON.parse(data));
					} catch (e) {
						reject(e);
					}
				}, reject);
			});
			req.on('error', reject);
			req.end();
		});
	}

	/**
	 * Link a memory to the newer memory that supersedes it
	 */
	static async linkSupersede(projectId: string, entryId: string, supersededById: string): Promise<void> {
		return new Promise((resolve, reject) => {
			const payload = JSON.stringify({ superseded_by_id: supersededById });
			const requestPath = `${API_PREFIX}/memory/${projectId}/${encodeURIComponent(entryId)}/supersede`;
			const options: http.RequestOptions = {
				...getRequestOptions('POST', requestPath),
				headers: {
					'Content-Type': 'application/json',
					'Content-Length': Buffer.byteLength(payload)
				}
			};
			const req = http.request(options, (res) => {
				if (res.statusCode !== 200) {
					buildDaemonError(res, requestPath).then(reject, reject);
					return;
				}
				resolve();
			});
			req.on('error', reject);
			req.write(payload);
			req.end();
		});
	}

	/**
	 * Add contradiction relationship between two memory entries
	 */
	static async addContradiction(projectId: string, entryId: string, contradictsId: string): Promise<void> {
		return new Promise((resolve, reject) => {
			const payload = JSON.stringify({ contradicts_id: contradictsId });
			const requestPath = `${API_PREFIX}/memory/${projectId}/${encodeURIComponent(entryId)}/contradictions`;
			const options: http.RequestOptions = {
				...getRequestOptions('POST', requestPath),
				headers: {
					'Content-Type': 'application/json',
					'Content-Length': Buffer.byteLength(payload)
				}
			};
			const req = http.request(options, (res) => {
				if (res.statusCode !== 200) {
					buildDaemonError(res, requestPath).then(reject, reject);
					return;
				}
				resolve();
			});
			req.on('error', reject);
			req.write(payload);
			req.end();
		});
	}

	/**
	 * Resolve/remove contradiction relationship between two entries
	 */
	static async resolveContradiction(projectId: string, entryId: string, contradictsId: string): Promise<void> {
		return new Promise((resolve, reject) => {
			const requestPath = `${API_PREFIX}/memory/${projectId}/${encodeURIComponent(entryId)}/contradictions/${encodeURIComponent(contradictsId)}`;
			const options: http.RequestOptions = getRequestOptions('DELETE', requestPath);
			const req = http.request(options, (res) => {
				if (res.statusCode !== 200) {
					buildDaemonError(res, requestPath).then(reject, reject);
					return;
				}
				resolve();
			});
			req.on('error', reject);
			req.end();
		});
	}

	/**
	 * Retrieve a local memory reasoning chain centered on a root entry
	 */
	static async getReasoningChain(projectId: string, entryId: string, depth?: number): Promise<{ root_id: string; count: number; nodes: MemoryEntry[]; edges: Array<{ from: string; to: string; relation: string }> }> {
		return new Promise((resolve, reject) => {
			const query = new URLSearchParams();
			if (typeof depth === 'number' && Number.isFinite(depth)) {
				query.set('depth', String(Math.max(1, Math.floor(depth))));
			}
			const suffix = query.toString() ? `?${query.toString()}` : '';
			const requestPath = `${API_PREFIX}/memory/${projectId}/${encodeURIComponent(entryId)}/reasoning-chain${suffix}`;
			const options = getRequestOptions('GET', requestPath);
			const req = http.request(options, (res) => {
				if (res.statusCode !== 200) {
					buildDaemonError(res, requestPath).then(reject, reject);
					return;
				}
				readResponseBody(res).then((data) => {
					try {
						const parsed = JSON.parse(data || '{}');
						resolve(parsed);
					} catch (e) {
						reject(e);
					}
				}, reject);
			});
			req.on('error', reject);
			req.end();
		});
	}

	/**
	 * Generate rules files via the Rust Daemon
	 */
	static async generateRules(
		projectId: string,
		redisUrl: string,
		ide: string,
		workspaceRoot: string
	): Promise<{ success: boolean; message: string }> {
		return new Promise((resolve, reject) => {
			const payload = JSON.stringify({
				project_id: projectId,
				redis_url: redisUrl,
				ide: ide,
				workspace_root: workspaceRoot
			});

			const requestPath = `${API_PREFIX}/rules/generate`;
			const options: http.RequestOptions = {
				...getRequestOptions('POST', requestPath),
				headers: {
					'Content-Type': 'application/json',
					'Content-Length': Buffer.byteLength(payload)
				}
			};

			const req = http.request(options, (res) => {
				readResponseBody(res).then((data) => {
					if (res.statusCode !== 200) {
						buildDaemonError(res, requestPath).then(reject, reject);
						return;
					}
					try {
						const result = JSON.parse(data);
						if (result.success) {
							resolve({ success: true, message: result.message });
						} else {
							reject(new Error(result.message || 'Failed to generate rules'));
						}
					} catch (e) {
						reject(e);
					}
				}, reject);
			});

			req.on('error', reject);
			req.write(payload);
			req.end();
		});
	}

	/**
	 * Get impact analysis for a file via the Rust Daemon
	 */
	static async getImpact(file: string): Promise<any> {
		return new Promise((resolve, reject) => {
			const requestPath = `${API_PREFIX}/autonomous/impact/${encodeURIComponent(file)}`;
			const options = getRequestOptions('GET', requestPath);
			const req = http.request(options, (res) => {
				if (res.statusCode !== 200) {
					buildDaemonError(res, requestPath).then(reject, reject);
					return;
				}
				readResponseBody(res).then((data) => {
					try {
						resolve(JSON.parse(data));
					} catch (e) {
						reject(e);
					}
				}, reject);
			});
			req.on('error', reject);
			req.end();
		});
	}

	/**
	 * Fetch session timeline records from daemon flight recorder
	 */
	static async getSessionTimeline(limit?: number, sinceMs?: number): Promise<{ count: number; items: any[] }> {
		return new Promise((resolve, reject) => {
			const query = new URLSearchParams();
			if (typeof limit === 'number' && Number.isFinite(limit)) {
				query.set('limit', String(Math.max(1, Math.floor(limit))));
			}
			if (typeof sinceMs === 'number' && Number.isFinite(sinceMs)) {
				query.set('since_ms', String(Math.floor(sinceMs)));
			}
			const suffix = query.toString() ? `?${query.toString()}` : '';
			const requestPath = `${API_PREFIX}/session/timeline${suffix}`;
			const options = getRequestOptions('GET', requestPath);

			const req = http.request(options, (res) => {
				if (res.statusCode !== 200) {
					buildDaemonError(res, requestPath).then(reject, reject);
					return;
				}
				readResponseBody(res).then((data) => {
					try {
						const parsed = JSON.parse(data);
						resolve({
							count: typeof parsed.count === 'number' ? parsed.count : 0,
							items: Array.isArray(parsed.items) ? parsed.items : []
						});
					} catch (e) {
						reject(e);
					}
				}, reject);
			});

			req.on('error', reject);
			req.end();
		});
	}

	static async getObserverDna(): Promise<ObserverDna> {
		return new Promise((resolve, reject) => {
			const requestPath = `${API_PREFIX}/observer/dna`;
			const options = getRequestOptions('GET', requestPath);
			const req = http.request(options, (res) => {
				if (res.statusCode !== 200) {
					buildDaemonError(res, requestPath).then(reject, reject);
					return;
				}
				readResponseBody(res).then((data) => {
					try {
						resolve(JSON.parse(data || '{}'));
					} catch (e) {
						reject(e);
					}
				}, reject);
			});
			req.on('error', reject);
			req.end();
		});
	}

	static async getObserverDnaOtel(): Promise<ObserverDnaOtelExport> {
		return new Promise((resolve, reject) => {
			const requestPath = `${API_PREFIX}/observer/dna/otel`;
			const options = getRequestOptions('GET', requestPath);
			const req = http.request(options, (res) => {
				if (res.statusCode !== 200) {
					buildDaemonError(res, requestPath).then(reject, reject);
					return;
				}
				readResponseBody(res).then((data) => {
					try {
						resolve(JSON.parse(data || '{}'));
					} catch (e) {
						reject(e);
					}
				}, reject);
			});
			req.on('error', reject);
			req.end();
		});
	}

	static async getObserverIntent(): Promise<ObserverIntentSnapshot | null> {
		return new Promise((resolve, reject) => {
			const requestPath = `${API_PREFIX}/observer/intent`;
			const options = getRequestOptions('GET', requestPath);
			const req = http.request(options, (res) => {
				if (res.statusCode !== 200) {
					buildDaemonError(res, requestPath).then(reject, reject);
					return;
				}
				readResponseBody(res).then((data) => {
					try {
						const parsed = JSON.parse(data || '{}');
						resolve(parsed && parsed.intent ? parsed.intent : null);
					} catch (e) {
						reject(e);
					}
				}, reject);
			});
			req.on('error', reject);
			req.end();
		});
	}

	static async getObserverGit(): Promise<ObserverGitInsights> {
		return new Promise((resolve, reject) => {
			const requestPath = `${API_PREFIX}/observer/git`;
			const options = getRequestOptions('GET', requestPath);
			const req = http.request(options, (res) => {
				if (res.statusCode !== 200) {
					buildDaemonError(res, requestPath).then(reject, reject);
					return;
				}
				readResponseBody(res).then((data) => {
					try {
						resolve(JSON.parse(data || '{}'));
					} catch (e) {
						reject(e);
					}
				}, reject);
			});
			req.on('error', reject);
			req.end();
		});
	}

	static async getAgentConfigs(): Promise<AgentConfigResponse> {
		return new Promise((resolve, reject) => {
			const requestPath = `${API_PREFIX}/agents/config`;
			const options = getRequestOptions('GET', requestPath);
			const req = http.request(options, (res) => {
				if (res.statusCode !== 200) {
					buildDaemonError(res, requestPath).then(reject, reject);
					return;
				}
				readResponseBody(res).then((data) => {
					try {
						resolve(JSON.parse(data || '{}'));
					} catch (e) {
						reject(e);
					}
				}, reject);
			});
			req.on('error', reject);
			req.end();
		});
	}

	static async getAgentReports(): Promise<AgentReportsResponse> {
		return new Promise((resolve, reject) => {
			const requestPath = `${API_PREFIX}/agents/reports`;
			const options = getRequestOptions('GET', requestPath);
			const req = http.request(options, (res) => {
				if (res.statusCode !== 200) {
					buildDaemonError(res, requestPath).then(reject, reject);
					return;
				}
				readResponseBody(res).then((data) => {
					try {
						resolve(JSON.parse(data || '{}'));
					} catch (e) {
						reject(e);
					}
				}, reject);
			});
			req.on('error', reject);
			req.end();
		});
	}

	static async compileContext(projectId: string, activeFile: string, tokenBudget: number, taskType?: string): Promise<CompiledContext> {
		return new Promise((resolve, reject) => {
			const payload = JSON.stringify({
				project_id: projectId,
				active_file: activeFile,
				token_budget: tokenBudget,
				task_type: taskType || null
			});
			const requestPath = `${API_PREFIX}/context/compile`;
			const options: http.RequestOptions = {
				...getRequestOptions('POST', requestPath),
				headers: {
					'Content-Type': 'application/json',
					'Content-Length': Buffer.byteLength(payload)
				}
			};
			const req = http.request(options, (res) => {
				if (res.statusCode !== 200) {
					buildDaemonError(res, requestPath).then(reject, reject);
					return;
				}
				readResponseBody(res).then((data) => {
					try {
						resolve(JSON.parse(data || '{}'));
					} catch (e) {
						reject(e);
					}
				}, reject);
			});
			req.on('error', reject);
			req.write(payload);
			req.end();
		});
	}

	static async getProactiveRisk(projectId: string, file: string): Promise<ProactiveRiskResponse> {
		return new Promise((resolve, reject) => {
			const query = new URLSearchParams({ project_id: projectId, file });
			const requestPath = `${API_PREFIX}/proactive/risk?${query.toString()}`;
			const options = getRequestOptions('GET', requestPath);
			const req = http.request(options, (res) => {
				if (res.statusCode !== 200) {
					buildDaemonError(res, requestPath).then(reject, reject);
					return;
				}
				readResponseBody(res).then((data) => {
					try {
						resolve(JSON.parse(data || '{}'));
					} catch (e) {
						reject(e);
					}
				}, reject);
			});
			req.on('error', reject);
			req.end();
		});
	}

	static async getImportance(topN = 15): Promise<ImportanceResponse> {
		return new Promise((resolve, reject) => {
			const query = new URLSearchParams({ top_n: String(topN) });
			const requestPath = `${API_PREFIX}/importance?${query.toString()}`;
			const options = getRequestOptions('GET', requestPath);
			const req = http.request(options, (res) => {
				if (res.statusCode !== 200) {
					buildDaemonError(res, requestPath).then(reject, reject);
					return;
				}
				readResponseBody(res).then((data) => {
					try {
						resolve(JSON.parse(data || '{}'));
					} catch (e) {
						reject(e);
					}
				}, reject);
			});
			req.on('error', reject);
			req.end();
		});
	}

	static async getBlastRadius(file: string, maxDepth?: number): Promise<BlastRadius> {
		return new Promise((resolve, reject) => {
			const query = new URLSearchParams({ file });
			if (typeof maxDepth === 'number') {
				query.set('max_depth', String(maxDepth));
			}
			const requestPath = `${API_PREFIX}/blast-radius?${query.toString()}`;
			const options = getRequestOptions('GET', requestPath);
			const req = http.request(options, (res) => {
				if (res.statusCode !== 200) {
					buildDaemonError(res, requestPath).then(reject, reject);
					return;
				}
				readResponseBody(res).then((data) => {
					try {
						resolve(JSON.parse(data || '{}'));
					} catch (e) {
						reject(e);
					}
				}, reject);
			});
			req.on('error', reject);
			req.end();
		});
	}

	static async getCausalChain(file: string): Promise<FileCausalContext> {
		return new Promise((resolve, reject) => {
			const query = new URLSearchParams({ file });
			const requestPath = `${API_PREFIX}/observer/call-graph?${query.toString()}`;
			const options = getRequestOptions('GET', requestPath);
			const req = http.request(options, (res) => {
				if (res.statusCode !== 200) {
					buildDaemonError(res, requestPath).then(reject, reject);
					return;
				}
				readResponseBody(res).then((data) => {
					try {
						resolve(JSON.parse(data || '{}'));
					} catch (e) {
						reject(e);
					}
				}, reject);
			});
			req.on('error', reject);
			req.end();
		});
	}

	static async getPromptOptimization(projectId: string, taskType: string): Promise<PromptOptimizationSuggestion> {
		return new Promise((resolve, reject) => {
			const query = new URLSearchParams({ task_type: taskType });
			const requestPath = `${API_PREFIX}/learning/prompts/${encodeURIComponent(projectId)}/optimize?${query.toString()}`;
			const options = getRequestOptions('GET', requestPath);
			const req = http.request(options, (res) => {
				if (res.statusCode !== 200) {
					buildDaemonError(res, requestPath).then(reject, reject);
					return;
				}
				readResponseBody(res).then((data) => {
					try {
						resolve(JSON.parse(data || '{}'));
					} catch (e) {
						reject(e);
					}
				}, reject);
			});
			req.on('error', reject);
			req.end();
		});
	}

	static async getModelPerformance(projectId: string): Promise<ModelPerformanceReport> {
		return new Promise((resolve, reject) => {
			const requestPath = `${API_PREFIX}/learning/model-performance/${encodeURIComponent(projectId)}`;
			const options = getRequestOptions('GET', requestPath);
			const req = http.request(options, (res) => {
				if (res.statusCode !== 200) {
					buildDaemonError(res, requestPath).then(reject, reject);
					return;
				}
				readResponseBody(res).then((data) => {
					try {
						resolve(JSON.parse(data || '{}'));
					} catch (e) {
						reject(e);
					}
				}, reject);
			});
			req.on('error', reject);
			req.end();
		});
	}

	static async getDeveloperProfile(): Promise<DeveloperProfile> {
		return new Promise((resolve, reject) => {
			const requestPath = `${API_PREFIX}/learning/developer-profile`;
			const options = getRequestOptions('GET', requestPath);
			const req = http.request(options, (res) => {
				if (res.statusCode !== 200) {
					buildDaemonError(res, requestPath).then(reject, reject);
					return;
				}
				readResponseBody(res).then((data) => {
					try {
						resolve(JSON.parse(data || '{}'));
					} catch (e) {
						reject(e);
					}
				}, reject);
			});
			req.on('error', reject);
			req.end();
		});
	}

	static async resolveHierarchy(layers: string[], entryId: string, merge = true): Promise<HierarchyResolution> {
		return new Promise((resolve, reject) => {
			const payload = JSON.stringify({ layers, entry_id: entryId, merge });
			const requestPath = `${API_PREFIX}/brain/hierarchy/resolve`;
			const options: http.RequestOptions = {
				...getRequestOptions('POST', requestPath),
				headers: {
					'Content-Type': 'application/json',
					'Content-Length': Buffer.byteLength(payload)
				}
			};
			const req = http.request(options, (res) => {
				if (res.statusCode !== 200) {
					buildDaemonError(res, requestPath).then(reject, reject);
					return;
				}
				readResponseBody(res).then((data) => {
					try {
						resolve(JSON.parse(data || '{}'));
					} catch (e) {
						reject(e);
					}
				}, reject);
			});
			req.on('error', reject);
			req.write(payload);
			req.end();
		});
	}

	/**
	 * Count tokens exactly via the Rust Daemon
	 */
	static async countTokens(text: string): Promise<{ tokens: number; chars: number }> {
		return new Promise((resolve, reject) => {
			const payload = JSON.stringify({ text });
			const requestPath = `${API_PREFIX}/tokens/count`;
			const options: http.RequestOptions = {
				...getRequestOptions('POST', requestPath),
				headers: {
					'Content-Type': 'application/json',
					'Content-Length': Buffer.byteLength(payload)
				}
			};
			const req = http.request(options, (res) => {
				readResponseBody(res).then((data) => {
					if (res.statusCode !== 200) {
						buildDaemonError(res, requestPath).then(reject, reject);
						return;
					}
					try {
						const result = JSON.parse(data);
						resolve({ tokens: result.tokens, chars: result.chars });
					} catch (e) {
						reject(e);
					}
				}, reject);
			});
			req.on('error', reject);
			req.write(payload);
			req.end();
		});
	}

	/**
	 * Synchronize CRDT Team Brain via Rust Daemon
	 */
	static async teamSync(projectId: string): Promise<{ success: boolean; message: string }> {
		return new Promise((resolve, reject) => {
			const payload = JSON.stringify({ project_id: projectId });
			const requestPath = `${API_PREFIX}/team/sync`;
			const options: http.RequestOptions = {
				...getRequestOptions('POST', requestPath),
				headers: {
					'Content-Type': 'application/json',
					'Content-Length': Buffer.byteLength(payload)
				}
			};
			const req = http.request(options, (res) => {
				readResponseBody(res).then((data) => {
					if (res.statusCode !== 200) {
						buildDaemonError(res, requestPath).then(reject, reject);
						return;
					}
					try {
						const result = JSON.parse(data);
						if (result.success) {
							resolve({ success: true, message: result.message });
						} else {
							reject(new Error(result.message || 'Team sync failed'));
						}
					} catch (e) {
						reject(e);
					}
				}, reject);
			});
			req.on('error', reject);
			req.write(payload);
			req.end();
		});
	}

	static async activateLicense(key: string, deviceId?: string): Promise<LicenseStatusResponse> {
		return new Promise((resolve, reject) => {
			const payload = JSON.stringify({ key, device_id: deviceId ?? null });
			const requestPath = `${API_PREFIX}/license/activate`;
			const options: http.RequestOptions = {
				...getRequestOptions('POST', requestPath),
				timeout: 10000,
				headers: {
					'Content-Type': 'application/json',
					'Content-Length': Buffer.byteLength(payload)
				}
			};
			const req = http.request(options, (res) => {
				readResponseBody(res).then((data) => {
					if ((res.statusCode ?? 500) >= 400) {
						try {
							const json = JSON.parse(data || '{}');
							const msg = json.message || '';
							// Map technical errors to user-friendly messages
							if (msg.includes('invalid license encoding') || msg.includes('Invalid')) {
								reject(new Error('Invalid license key. Please check and try again.'));
							} else if (msg.includes('invalid license signature')) {
								reject(new Error('Invalid license key. Please check and try again.'));
							} else if (msg.includes('expired')) {
								reject(new Error('License key has expired.'));
							} else if (msg.includes('revoked')) {
								reject(new Error('License key has been revoked.'));
							} else if (msg.includes('unavailable')) {
								reject(new Error('License validation is unavailable. Please try again later.'));
							} else {
								reject(new Error(msg || 'License activation failed.'));
							}
						} catch {
							reject(new Error('License activation failed.'));
						}
						return;
					}
					try {
						resolve(JSON.parse(data || '{}'));
					} catch (e) {
						reject(e);
					}
				}, reject);
			});
			req.on('error', reject);
			req.on('timeout', () => {
				req.destroy();
				reject(new Error('License activation timed out'));
			});
			req.write(payload);
			req.end();
		});
	}

	static async controlStatus(projectId?: string, workspaceRoot?: string): Promise<any> {
		return new Promise((resolve, reject) => {
			let requestPath = `${API_PREFIX}/control/status`;
			if (projectId) {
				requestPath += `?project_id=${encodeURIComponent(projectId)}`;
				if (workspaceRoot) {
					requestPath += `&workspace_root=${encodeURIComponent(workspaceRoot)}`;
				}
			}
			const req = http.request(getRequestOptions('GET', requestPath), (res) => {
				readResponseBody(res).then((data) => {
					if ((res.statusCode ?? 500) >= 400) {
						buildDaemonError(res, requestPath).then(reject, reject);
						return;
					}
					try {
						resolve(JSON.parse(data || '{}'));
					} catch (e) {
						reject(e);
					}
				}, reject);
			});
			req.on('error', reject);
			req.end();
		});
	}

	static async controlResume(projectId?: string): Promise<void> {
		return new Promise((resolve, reject) => {
			let requestPath = `${API_PREFIX}/control/resume`;
			if (projectId) {
				requestPath += `?project_id=${encodeURIComponent(projectId)}`;
			}
			const req = http.request(getRequestOptions('POST', requestPath), (res) => {
				readResponseBody(res).then((data) => {
					if ((res.statusCode ?? 500) >= 400) {
						buildDaemonError(res, requestPath).then(reject, reject);
						return;
					}
					try {
						resolve(JSON.parse(data || '{}'));
					} catch (e) {
						reject(e);
					}
				}, reject);
			});
			req.on('error', reject);
			req.end();
		});
	}

	static async controlPause(projectId?: string): Promise<void> {
		return new Promise((resolve, reject) => {
			let requestPath = `${API_PREFIX}/control/pause`;
			if (projectId) {
				requestPath += `?project_id=${encodeURIComponent(projectId)}`;
			}
			const req = http.request(getRequestOptions('POST', requestPath), (res) => {
				readResponseBody(res).then((data) => {
					if ((res.statusCode ?? 500) >= 400) {
						buildDaemonError(res, requestPath).then(reject, reject);
						return;
					}
					try {
						resolve(JSON.parse(data || '{}'));
					} catch (e) {
						reject(e);
					}
				}, reject);
			});
			req.on('error', reject);
			req.end();
		});
	}

	static async getLicenseStatus(deviceId?: string): Promise<LicenseStatusResponse> {
		return new Promise((resolve, reject) => {
			const suffix = deviceId ? `?device_id=${encodeURIComponent(deviceId)}` : '';
			const requestPath = `${API_PREFIX}/license/status${suffix}`;
			const req = http.request(getRequestOptions('GET', requestPath), (res) => {
				readResponseBody(res).then((data) => {
					if ((res.statusCode ?? 500) >= 400) {
						buildDaemonError(res, requestPath).then(reject, reject);
						return;
					}
					try {
						resolve(JSON.parse(data || '{}'));
					} catch (e) {
						reject(e);
					}
				}, reject);
			});
			req.on('error', reject);
			req.end();
		});
	}

	static async getPatternReport(): Promise<any> {
		return new Promise((resolve, reject) => {
			const requestPath = `${API_PREFIX}/observer/patterns`;
			const req = http.request(getRequestOptions('GET', requestPath), (res) => {
				readResponseBody(res).then((data) => {
					if ((res.statusCode ?? 500) >= 400) {
						buildDaemonError(res, requestPath).then(reject, reject);
						return;
					}
					try {
						resolve(JSON.parse(data || '{}'));
					} catch (e) {
						reject(e);
					}
				}, reject);
			});
			req.on('error', reject);
			req.end();
		});
	}

	static async getTokenStats(projectId?: string): Promise<TokenStatsResponse> {
		return new Promise((resolve, reject) => {
			const query = projectId ? `?project_id=${encodeURIComponent(projectId)}` : '';
			const requestPath = `${API_PREFIX}/tokens/stats${query}`;
			const req = http.request(getRequestOptions('GET', requestPath), (res) => {
				readResponseBody(res).then((data) => {
					if ((res.statusCode ?? 500) >= 400) {
						buildDaemonError(res, requestPath).then(reject, reject);
						return;
					}
					try {
						resolve(JSON.parse(data || '{}'));
					} catch (e) {
						reject(e);
					}
				}, reject);
			});
			req.on('error', reject);
			req.end();
		});
	}

	static async recordAiTokenUse(tokens: number, taskType?: string): Promise<void> {
		return new Promise((resolve, reject) => {
			const payload = JSON.stringify({ tokens, task_type: taskType });
			const requestPath = `${API_PREFIX}/tokens/record`;
			const options: http.RequestOptions = {
				...getRequestOptions('POST', requestPath),
				headers: {
					'Content-Type': 'application/json',
					'Content-Length': Buffer.byteLength(payload)
				}
			};
			const req = http.request(options, (res) => {
				if (res.statusCode !== 200) {
					console.warn(`Failed to record AI token use: ${res.statusCode}`);
					resolve(); // don't block
					return;
				}
				resolve();
			});
			req.on('error', reject);
			req.write(payload);
			req.end();
		});
	}

	// ─── Context Orchestrator ────────────────────────────────────────────────
    static async orchestrate(req: {
        prompt: string;
        project_id: string;
        active_file: string;
        context_budget?: number;
        task_type?: string;
        max_depth?: number;
    }): Promise<{
        enhanced_prompt: string;
        sections_used: number;
        compiled_tokens: number;
        naive_estimate: number;
        compression_ratio: number;
        relevant_files: string[];
        compilation_summary: string;
    }> {
        return new Promise((resolve, reject) => {
            const payload = JSON.stringify(req);
            const requestPath = `${API_PREFIX}/orchestrate`;
            const options: http.RequestOptions = {
                ...getRequestOptions('POST', requestPath),
                headers: {
                    'Content-Type': 'application/json',
                    'Content-Length': Buffer.byteLength(payload),
                },
            };
            const httpReq = http.request(options, (res) => {
                readResponseBody(res).then((data) => {
                    if ((res.statusCode ?? 500) >= 400) {
                        buildDaemonError(res, requestPath).then(reject, reject);
                        return;
                    }
                    try {
                        resolve(JSON.parse(data || '{}'));
                    } catch (e) {
                        reject(e);
                    }
                }, reject);
            });
            httpReq.on('error', reject);
            httpReq.write(payload);
            httpReq.end();
        });
    }

    // ─── Skeleton Index Management ──────────────────────────────────────────
    static async purgeSkeleton(projectId: string): Promise<{ success: boolean; entries_purged: number }> {
        return new Promise((resolve, reject) => {
            const requestPath = `${API_PREFIX}/skeleton/purge/${projectId}`;
            const options: http.RequestOptions = {
                ...getRequestOptions('POST', requestPath),
            };
            const httpReq = http.request(options, (res) => {
                readResponseBody(res).then((data) => {
                    if ((res.statusCode ?? 500) >= 400) {
                        buildDaemonError(res, requestPath).then(reject, reject);
                        return;
                    }
                    try {
                        resolve(JSON.parse(data || '{}'));
                    } catch (e) {
                        reject(e);
                    }
                }, reject);
            });
            httpReq.on('error', reject);
            httpReq.end();
        });
    }

    // ─── Workspace Registry (Multi-Tenant) ────────────────────────────────────
    static async registerWorkspace(projectId: string, workspaceRoot: string): Promise<{
        registered: boolean;
        is_new: boolean;
        registry: { workspaces: any[]; active_project_id: string | null };
    }> {
        return new Promise((resolve, reject) => {
            const payload = JSON.stringify({ project_id: projectId, workspace_root: workspaceRoot });
            const requestPath = `${API_PREFIX}/workspace/register`;
            const options: http.RequestOptions = {
                ...getRequestOptions('POST', requestPath),
                headers: {
                    'Content-Type': 'application/json',
                    'Content-Length': Buffer.byteLength(payload),
                },
            };
            const httpReq = http.request(options, (res) => {
                readResponseBody(res).then((data) => {
                    if ((res.statusCode ?? 500) >= 400) {
                        buildDaemonError(res, requestPath).then(reject, reject);
                        return;
                    }
                    try {
                        resolve(JSON.parse(data || '{}'));
                    } catch (e) {
                        reject(e);
                    }
                }, reject);
            });
            httpReq.on('error', reject);
            httpReq.write(payload);
            httpReq.end();
        });
    }

    static async unregisterWorkspace(projectId: string): Promise<{
        unregistered: boolean;
        registry: { workspaces: any[]; active_project_id: string | null };
    }> {
        return new Promise((resolve, reject) => {
            const payload = JSON.stringify({ project_id: projectId });
            const requestPath = `${API_PREFIX}/workspace/unregister`;
            const options: http.RequestOptions = {
                ...getRequestOptions('POST', requestPath),
                headers: {
                    'Content-Type': 'application/json',
                    'Content-Length': Buffer.byteLength(payload),
                },
            };
            const httpReq = http.request(options, (res) => {
                readResponseBody(res).then((data) => {
                    if ((res.statusCode ?? 500) >= 400) {
                        buildDaemonError(res, requestPath).then(reject, reject);
                        return;
                    }
                    try {
                        resolve(JSON.parse(data || '{}'));
                    } catch (e) {
                        reject(e);
                    }
                }, reject);
            });
            httpReq.on('error', reject);
            httpReq.write(payload);
            httpReq.end();
        });
    }

    static async activateWorkspace(projectId: string): Promise<{
        activated: boolean;
        registry: { workspaces: any[]; active_project_id: string | null };
    }> {
        return new Promise((resolve, reject) => {
            const payload = JSON.stringify({ project_id: projectId });
            const requestPath = `${API_PREFIX}/workspace/activate`;
            const options: http.RequestOptions = {
                ...getRequestOptions('POST', requestPath),
                headers: {
                    'Content-Type': 'application/json',
                    'Content-Length': Buffer.byteLength(payload),
                },
            };
            const httpReq = http.request(options, (res) => {
                readResponseBody(res).then((data) => {
                    if ((res.statusCode ?? 500) >= 400) {
                        buildDaemonError(res, requestPath).then(reject, reject);
                        return;
                    }
                    try {
                        resolve(JSON.parse(data || '{}'));
                    } catch (e) {
                        reject(e);
                    }
                }, reject);
            });
            httpReq.on('error', reject);
            httpReq.write(payload);
            httpReq.end();
        });
    }

    static async listWorkspaces(): Promise<{
        workspaces: Array<{ project_id: string; workspace_root: string; last_active_at: number; indexing_complete: boolean }>;
        active_project_id: string | null;
        active_workspace_root: string | null;
    }> {
        return new Promise((resolve, reject) => {
            const requestPath = `${API_PREFIX}/workspace/list`;
            const options: http.RequestOptions = {
                ...getRequestOptions('GET', requestPath),
            };
            const httpReq = http.request(options, (res) => {
                readResponseBody(res).then((data) => {
                    if ((res.statusCode ?? 500) >= 400) {
                        buildDaemonError(res, requestPath).then(reject, reject);
                        return;
                    }
                    try {
                        resolve(JSON.parse(data || '{}'));
                    } catch (e) {
                        reject(e);
                    }
                }, reject);
            });
            httpReq.on('error', reject);
            httpReq.end();
        });
    }
}
