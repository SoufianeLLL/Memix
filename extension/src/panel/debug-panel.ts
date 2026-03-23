import * as fs from 'fs';
import * as path from 'path';
import * as vscode from 'vscode';
import { BrainManager } from '../core/brain';
import { HealthMonitor } from '../core/health';
import { ConflictHandler } from '../core/conflict';
import { BRAIN_KEYS, BRAIN_KEY_SPECS, TAXONOMY_MAP } from '../utils/constants';
import { createPromptPack, PromptPackVariant } from '../utils/prompt-pack';
import { detectIDE, getRulesConfig } from '../ide/detector';
import { MemoryClient } from '../client';
import { DaemonManager } from '../daemon';
import { DaemonReadinessState } from '../daemon-runtime';

const panelOutputChannel = vscode.window.createOutputChannel('Memix Panel');

export class DebugPanelProvider implements vscode.WebviewViewProvider {
	public static readonly viewType = 'memix.debugPanel';
	private lastContextCompileMs: number = 0;
	// Tracks which file was active during the last context compilation.
	// When the active file changes, the throttle resets immediately so the
	// developer always gets fresh context for the file they just switched to.
	private lastCompiledFile: string = '';

	private _view?: vscode.WebviewView;
	private brain: BrainManager | null;
	private health: HealthMonitor | null;
	private conflicts: ConflictHandler | null;
	private promptPackVariant: 'Small' | 'Standard' | 'Deep' = 'Standard';
	private daemonState: DaemonReadinessState = {
		kind: 'downloading',
		title: 'Preparing Memix Daemon',
		description: 'Checking daemon availability before Memix becomes available.',
	};

	constructor(
		private extensionUri: vscode.Uri,
		brain: BrainManager | null,
		private workspaceRoot: string | null
	) {
		this.brain = brain;
		this.health = brain ? new HealthMonitor(brain) : null;
		this.conflicts = (brain && workspaceRoot) ? new ConflictHandler(brain, workspaceRoot) : null;
	}

	setBrain(brain: BrainManager) {
		this.brain = brain;
		this.health = new HealthMonitor(brain);
		if (this.workspaceRoot) {
			this.conflicts = new ConflictHandler(brain, this.workspaceRoot);
		}
	}

	setDaemonState(state: DaemonReadinessState) {
		this.daemonState = state;
		if (!this._view) {
			return;
		}
		this._view.webview.postMessage({
			command: 'daemonState',
			data: state,
		});
	}

	private async refreshSettings() {
		if (!this._view) { return; }
		try {
			const settings = await DaemonManager.getSettings();
			this._view.webview.postMessage({ command: 'settingsData', data: settings });
		} catch (error) {
			console.error('Failed to load daemon settings', error);
		}
	}

	private postLoading(text: string) {
		this._view?.webview.postMessage({ command: 'showLoading', text });
	}

	private hideLoading() {
		this._view?.webview.postMessage({ command: 'hideLoading' });
	}

	private escapeWebviewHtml(value: string) {
		return value
			.replace(/&/g, '&amp;')
			.replace(/</g, '&lt;')
			.replace(/>/g, '&gt;')
			.replace(/"/g, '&quot;')
			.replace(/'/g, '&#39;');
	}

	private async openCenteredPayloadView(title: string, payload: string, subtitle?: string, notice?: string) {
		try {
			let language = 'plaintext';
			if (payload.trim().startsWith('{') || payload.trim().startsWith('[')) {
				try {
					JSON.parse(payload);
					language = 'json';
				} catch (e) {
					// Fall back to plaintext
				}
			}

			const document = await vscode.workspace.openTextDocument({
				content: payload,
				language
			});
			await vscode.window.showTextDocument(document, { preview: false });
		} catch (error) {
			vscode.window.showErrorMessage(`Failed to open payload: ${error instanceof Error ? error.message : String(error)}`);
		}
	}

	private async runPanelCommand<T>(loadingText: string, action: () => Promise<T>, options?: { refreshAfter?: boolean }) {
		const refreshAfter = options?.refreshAfter !== false;
		this.postLoading(loadingText);
		try {
			const result = await action();
			if (refreshAfter) {
				await this.sendUpdate();
			} else {
				this.hideLoading();
			}
			return result;
		} catch (error) {
			this.hideLoading();
			throw error;
		}
	}

	resolveWebviewView(webviewView: vscode.WebviewView) {
		this._view = webviewView;

		webviewView.webview.options = {
			enableScripts: true,
			localResourceRoots: [this.extensionUri]
		};

		webviewView.webview.html = this.getHtml();
		this.setDaemonState(this.daemonState);

		webviewView.webview.onDidReceiveMessage(async (msg) => {
			switch (msg.command) {
				case 'openBrainKey':
					if (typeof msg.key === 'string' && msg.key) {
						// Open the brain mirror JSON file if it exists on disk,
						// otherwise show the raw value from Redis in an untitled document
						const mirrorPath = this.workspaceRoot
							? path.join(this.workspaceRoot, '.memix', 'brain', `${msg.key}.json`)
							: null;
						if (mirrorPath && fs.existsSync(mirrorPath)) {
							const uri = vscode.Uri.file(mirrorPath);
							await vscode.window.showTextDocument(uri, { preview: false });
						} else if (this.brain) {
							const value = await this.brain.get(msg.key);
							if (value !== undefined && value !== null) {
								const content = typeof value === 'string'
									? value
									: JSON.stringify(value, null, 2);
								const doc = await vscode.workspace.openTextDocument({
									content,
									language: 'json'
								});
								await vscode.window.showTextDocument(doc, { preview: false });
							}
						}
					}
					break;
				case 'scanPatterns':
					await this.runPanelCommand('Scanning patterns...', async () => {
						await this.sendPatternUpdate();
					}, { refreshAfter: false });
					break;
				case 'refresh':
					if (!msg?.silent) {
						this.postLoading('Refreshing data...');
					}
					await this.sendUpdate({ includeAdvanced: msg?.includeAdvanced !== false });
					break;
				case 'connectRedis':
					await this.runPanelCommand('Opening Redis connect...', async () => {
						await vscode.commands.executeCommand('memix.connect');
					});
					break;
				case 'initBrain':
					this.postLoading('Initializing brain...');
					try {
						await vscode.commands.executeCommand('memix.init');
						await this.sendUpdate({ includeAdvanced: false });
					} catch (e) {
						this.hideLoading();
						vscode.window.showErrorMessage(`Failed to initialize brain: ${e}`);
					}
					break;
				case 'clearBrain':
					if (!this.brain) { return; }
					const confirm = await vscode.window.showWarningMessage(
						'Clear entire brain? This cannot be undone.',
						'Yes, clear', 'Cancel'
					);
					if (confirm === 'Yes, clear') {
						await this.runPanelCommand('Clearing brain...', async () => {
							await this.brain!.clearAll();
						});
					}
					break;
				case 'copyText':
					if (typeof msg.text === 'string') {
						await vscode.env.clipboard.writeText(msg.text);
						if (typeof msg.notice === 'string' && msg.notice) {
							vscode.window.showInformationMessage(msg.notice);
						}
					}
					break;
				case 'openCenteredPayload':
					if (typeof msg.payload === 'string' && typeof msg.title === 'string') {
						this.openCenteredPayloadView(
							msg.title,
							msg.payload,
							typeof msg.subtitle === 'string' ? msg.subtitle : '',
							typeof msg.notice === 'string' ? msg.notice : 'Copied details to clipboard'
						);
					}
					break;
				case 'copyPromptPack':
					if (typeof msg.text === 'string') {
						await vscode.env.clipboard.writeText(msg.text);
						vscode.window.showInformationMessage('Memix Prompt Pack copied to clipboard');
					}
					break;
				case 'setPromptPackVariant':
					if (msg && (msg.variant === 'Small' || msg.variant === 'Standard' || msg.variant === 'Deep')) {
						await this.runPanelCommand('Updating Prompt Pack...', async () => {
							this.promptPackVariant = msg.variant;
						});
					}
					break;
				case 'editRedisMaxOverride': {
					await this.runPanelCommand('Updating Redis memory settings...', async () => {
						const cfg = vscode.workspace.getConfiguration();
						const current = cfg.get<number>('memix.redis.maxMemoryMbOverride') || 0;
						const picked = await vscode.window.showQuickPick(
							[
								{ label: 'Auto-detect', description: 'Use Redis maxmemory when available (0)', value: 0 },
								{ label: '30 MB', description: 'Redis Cloud free tier', value: 30 },
								{ label: '50 MB', description: '', value: 50 },
								{ label: 'Custom…', description: `Current: ${current} MB`, value: -1 }
							],
							{ placeHolder: 'Set Redis max memory override (MB)' }
						);
						if (!picked) return;
						let nextVal = picked.value;
						if (nextVal === -1) {
							const input = await vscode.window.showInputBox({
								prompt: 'Enter Redis max memory override in MB (0 = auto-detect)',
								value: String(current)
							});
							if (input === undefined) return;
							const n = Number(input);
							if (!Number.isFinite(n) || n < 0) {
								vscode.window.showErrorMessage('Invalid value. Please enter a number >= 0.');
								return;
							}
							nextVal = n;
						}
						await cfg.update('memix.redis.maxMemoryMbOverride', nextVal, vscode.ConfigurationTarget.Workspace);
					});
					break;
				}

				case 'exportBrain':
					await this.runPanelCommand('Prompting export...', async () => {
						await vscode.commands.executeCommand('memix.exportBrain');
					});
					break;
				case 'importBrain':
					await this.runPanelCommand('Prompting import...', async () => {
						await vscode.commands.executeCommand('memix.importBrain');
					});
					break;
				case 'teamSync':
					await this.runPanelCommand('Initiating team sync...', async () => {
						await vscode.commands.executeCommand('memix.teamSync');
					});
					break;
				case 'prune':
					await this.runPanelCommand('Pruning stale data...', async () => {
						await vscode.commands.executeCommand('memix.prune');
					});
					break;
				case 'recoverBrain':
					await this.runPanelCommand('Recovering brain...', async () => {
						await vscode.commands.executeCommand('memix.recoverBrain');
					});
					break;
				case 'fixMissingKeys':
					if (!this.brain) { return; }
					await this.runPanelCommand('Creating missing baseline keys...', async () => {
						await this.brain!.init();
						vscode.window.showInformationMessage('Memix baseline keys restored for this workspace.');
					});
					break;
				case 'healthCheck':
					if (!this.health) { return; }
					await this.runPanelCommand('Running health check...', async () => {
						const report = await this.health!.runFullCheck();
						webviewView.webview.postMessage({ command: 'healthReport', data: report });
					}, { refreshAfter: false });
					break;
				case 'detectConflicts':
					if (!this.conflicts) { return; }
					await this.runPanelCommand('Detecting conflicts...', async () => {
						const conflictList = await this.conflicts!.detectConflicts();
						webviewView.webview.postMessage({ command: 'conflicts', data: conflictList });
					}, { refreshAfter: false });
					break;
				case 'pauseBrain':
					try {
						this.postLoading('Pausing brain...');
						await DaemonManager.pause();
						// Update main view first (shows empty-state overlay)
						await this.sendUpdate({ includeAdvanced: false });
						// Then refresh settings view on top so toggles are correct
						await this.refreshSettings();
					} catch (e) {
						this.hideLoading();
						vscode.window.showErrorMessage(`Failed to pause brain: ${e}`);
					}
					break;
				case 'resumeBrain':
					try {
						this.postLoading('Waking brain up...');
						await DaemonManager.resume();
						// Update main view first (hides empty-state overlay)
						await this.sendUpdate({ includeAdvanced: false });
						// Then refresh settings view on top so toggles + feature cards are correct
						await this.refreshSettings();
					} catch (e) {
						this.hideLoading();
						vscode.window.showErrorMessage(`Failed to resume brain: ${e}`);
					}
					break;
				case 'refreshSettings':
					await this.refreshSettings();
					break;
				case 'openBrainKey': {
					if (typeof msg.key === 'string' && this.workspaceRoot) {
						const path = require('path');
						const brainDir = path.join(this.workspaceRoot, '.memix', 'brain');
						// Normalize key to filename: session:state → session_state.json
						const fileName = msg.key.replace(/:/g, '_') + '.json';
						const filePath = path.join(brainDir, fileName);
						try {
							const uri = vscode.Uri.file(filePath);
							const doc = await vscode.workspace.openTextDocument(uri);
							await vscode.window.showTextDocument(doc, { preview: true });
						} catch {
							vscode.window.showWarningMessage(`Brain key file not found: ${fileName}`);
						}
					}
					break;
				}
				case 'showVersionInfo': {
					try {
						const path = require('path');
						const fs = require('fs');
						// Try multiple possible locations for versions.json
						const possiblePaths = [
							path.join(this.extensionUri.fsPath, '..', 'versions.json'),
							path.join(this.extensionUri.fsPath, 'versions.json'),
							path.join(this.extensionUri.fsPath, '..', '..', 'versions.json'),
						];
						let versionsPath: string | null = null;
						for (const p of possiblePaths) {
							if (fs.existsSync(p)) {
								versionsPath = p;
								break;
							}
						}
						let daemonVer = 'unknown';
						let extensionVer = 'unknown';
						let lastModified = '';
						if (versionsPath) {
							try {
								const raw = fs.readFileSync(versionsPath, 'utf8');
								const v = JSON.parse(raw);
								daemonVer = v.daemonVersion || 'unknown';
								extensionVer = v.extensionVersion || 'unknown';
								const stat = fs.statSync(versionsPath);
								const diffMs = Date.now() - new Date(stat.mtime).getTime();
								const days = Math.floor(diffMs / (1000 * 60 * 60 * 24));
								if (days === 0) { lastModified = 'today'; }
								else if (days === 1) { lastModified = '1 day ago'; }
								else if (days < 30) { lastModified = `${days} days ago`; }
								else if (days < 365) { lastModified = `${Math.floor(days / 30)} months ago`; }
								else { lastModified = `${Math.floor(days / 365)} years ago`; }
							} catch (e) {
								console.error('Failed to read versions.json:', e);
							}
						} else {
							console.warn('versions.json not found in any expected location');
						}
						// Fallback: try package.json for extension version
						if (extensionVer === 'unknown') {
							try {
								const pkgPath = path.join(this.extensionUri.fsPath, 'package.json');
								const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf8'));
								extensionVer = pkg.version || 'unknown';
							} catch { /* ignore */ }
						}
						const detail = `Daemon: v${daemonVer}\nExtension: v${extensionVer}${lastModified ? `\nLast updated: ${lastModified}` : ''}`;
						vscode.window.showInformationMessage(`Memix Version Info`, { modal: true, detail });
					} catch (e) {
						vscode.window.showErrorMessage(`Failed to read version info: ${e}`);
					}
					break;
				}

			}
		});

		// Defer initial load so the webview script has time to register its listener
		setTimeout(() => {
			webviewView.webview.postMessage({ command: 'showLoading', text: 'Loading brain...' });
			this.sendUpdate({ includeAdvanced: false });
		}, 500);
	}

	async runHealthCheck() {
		if (!this._view || !this.health) return;
		const report = await this.health.runFullCheck();
		this._view.webview.postMessage({ command: 'healthReport', data: report });
	}

	async runConflictDetection() {
		if (!this._view || !this.conflicts) return;
		const conflictList = await this.conflicts.detectConflicts();
		this._view.webview.postMessage({ command: 'conflicts', data: conflictList });
	}

	getWebView() {
		return this._view?.webview;
	}

	private formatTimelineEvent(
		eventType: string,
		payload: Record<string, unknown>,
		workspaceRoot: string | null
	): string {
		// Strip workspace root from any path value for display
		const displayPath = (p: unknown): string => {
			if (typeof p !== 'string') { return String(p || '—'); }
			if (workspaceRoot && p.startsWith(workspaceRoot)) {
				return p.slice(workspaceRoot.length).replace(/^[/\\]/, '');
			}
			return p;
		};

		switch (eventType) {
			case 'AstMutation': {
				const file = displayPath(payload.file);
				const nodes = Number(payload.nodes_changed ?? 0);
				if (nodes === 0) {
					return `Opened — ${file}`;
				}
				if (nodes === 1) {
					return `1 node changed — ${file}`;
				}
				return `${nodes} nodes changed — ${file}`;
			}
			case 'IntentDetected': {
				const intentRaw = String(payload.intent_type || 'unknown');
				// Map internal snake_case names to human-readable labels
				const intentLabels: Record<string, string> = {
					scaffolding: 'Building new code',
					refactoring: 'Refactoring',
					bug_fixing: 'Fixing a bug',
					api_design: 'Designing an API',
					testing: 'Writing tests',
					configuration: 'Editing config',
					exploration: 'Browsing code',
				};
				return `Intent detected — ${intentLabels[intentRaw] ?? intentRaw}`;
			}
			case 'MemoryAccessed': {
				const id = String(payload.memory_id || '—');
				return `Memory read — ${id}`;
			}
			case 'ScorePenalty': {
				const reason = String(payload.reason || 'unknown reason');
				const severity = Number(payload.severity ?? 0);
				const level = severity >= 8 ? 'high' : severity >= 4 ? 'medium' : 'low';
				return `Score penalty (${level}) — ${reason}`;
			}
			default: {
				// For any future event types, produce a readable sentence
				// rather than raw JSON. Summarise up to 3 key-value pairs.
				const pairs = Object.entries(payload).slice(0, 3);
				if (pairs.length === 0) {
					return eventType.replace(/([A-Z])/g, ' $1').trim();
				}
				const summary = pairs
					.map(([k, v]) => `${k}: ${displayPath(v)}`)
					.join(', ');
				return `${eventType.replace(/([A-Z])/g, ' $1').trim()} — ${summary}`;
			}
		}
	}

	async sendUpdate(options?: { includeAdvanced?: boolean }) {
		if (!this._view) { return; }

		if (this.daemonState.kind !== 'ready') {
			this._view.webview.postMessage({
				command: 'daemonState',
				data: this.daemonState,
			});
			return;
		}

		if (!this.brain) {
			this._view.webview.postMessage({
				command: 'error',
				data: 'No workspace open'
			});
			return;
		}

		try {
			const includeAdvanced = options?.includeAdvanced !== false;
			const allData = await this.brain.getAll();
			const projectId = this.brain.getProjectId();
			const activeFile = vscode.window.activeTextEditor?.document?.uri?.fsPath || '';
			const keys: Record<string, number> = {};
			let totalBytes = 0;
			for (const [k, v] of Object.entries(allData)) {
				const strValue = typeof v === 'string' ? v : JSON.stringify(v);
				const size = Buffer.byteLength(strValue || '', 'utf8');
				keys[k] = size;
				totalBytes += size;
			}
			const sizeInfo = { totalBytes, keys };
			const healthReport = this.health!.runFullCheckFromSnapshot(allData);

			const requiredKeys = [BRAIN_KEYS.IDENTITY, BRAIN_KEYS.SESSION_STATE, BRAIN_KEYS.PATTERNS];
			const missingRequiredKeys = requiredKeys.filter(k => !(k in sizeInfo.keys));
			const isInitialized = missingRequiredKeys.length === 0;

			let redisUsedBytes = sizeInfo.totalBytes;
			let redisMaxBytes = sizeInfo.totalBytes * 2;
			let redisMaxEstimated = false;
			const redisMaxOverrideMb = vscode.workspace.getConfiguration().get<number>('memix.redis.maxMemoryMbOverride') || 0;
			if (redisMaxOverrideMb > 0) {
				redisMaxBytes = redisMaxOverrideMb * 1024 * 1024;
				redisMaxEstimated = false;
			}
			try {
				const stats = await MemoryClient.getRedisStats();
				if (stats && typeof stats.used_bytes === 'number') {
					redisUsedBytes = stats.used_bytes;
				}
				if (redisMaxOverrideMb > 0) {
					// override is authoritative
				} else if (stats && typeof stats.max_bytes === 'number' && stats.max_bytes > 0) {
					redisMaxBytes = stats.max_bytes;
				} else if (stats && typeof stats.used_bytes === 'number' && stats.used_bytes > 0) {
					// Some providers don't expose maxmemory via CONFIG/INFO; we still want a meaningful bar.
					redisMaxBytes = 30 * 1024 * 1024;
					redisMaxEstimated = true;
				}
			} catch {
				// ignore and keep fallback
			}
			if (!redisMaxBytes || redisMaxBytes <= 0) {
				redisMaxBytes = 1;
			}

			const canonicalKeyList = Object.keys(BRAIN_KEY_SPECS);
			const allKeysSorted = Array.from(new Set([...canonicalKeyList, ...Object.keys(sizeInfo.keys)])).sort();
			const keyCoverage = allKeysSorted.map((k) => {
				const spec = BRAIN_KEY_SPECS[k];
				const exists = k in sizeInfo.keys;
				const tier = spec?.tier || 'system';
				const state = exists
					? 'ok'
					: tier === 'required'
						? 'missing_required'
						: tier === 'recommended'
							? 'missing_recommended'
							: tier === 'generated'
								? 'not_generated'
								: 'optional';
				return {
					key: k,
					label: spec?.label || k,
					exists,
					sizeBytes: sizeInfo.keys[k] || 0,
					taxonomy: TAXONOMY_MAP[k] || '—',
					tier,
					description: spec?.description || '',
					fixStrategy: spec?.fixStrategy || 'manual',
					state
				};
			});
			const missingRequiredCoverage = keyCoverage.filter((entry) => entry.state === 'missing_required');
			const missingRecommendedCoverage = keyCoverage.filter((entry) => entry.state === 'missing_recommended');
			const generatedMissingCoverage = keyCoverage.filter((entry) => entry.state === 'not_generated');

			const promptPackData = createPromptPack(allData, this.promptPackVariant as PromptPackVariant);
			const promptPack = promptPackData.text;
			let promptPackTokens: number | null = null;
			try {
				promptPackTokens = (await MemoryClient.countTokens(promptPack)).tokens;
			} catch {
				promptPackTokens = null;
			}

			let isPaused = false;
			try {
				const healthResp = await DaemonManager.ping();
				if (healthResp?.status === 'paused') {
					isPaused = true;
				}
			} catch (e) {
				// daemon might be down
			}

			panelOutputChannel.appendLine(
				`[Panel] keys=${Object.keys(sizeInfo.keys).length} ` +
				`initialized=${isInitialized} ` +
				`paused=${isPaused} ` +
				`missing=[${missingRequiredKeys.join(', ')}] ` +
				`received=[${Object.keys(sizeInfo.keys).join(', ')}]`
			);

			const lastUpdatedRaw = allData[BRAIN_KEYS.SESSION_STATE]?.last_updated;
			let stalenessHours: number | null = null;
			if (typeof lastUpdatedRaw === 'string' && lastUpdatedRaw) {
				const t = new Date(lastUpdatedRaw).getTime();
				if (!Number.isNaN(t)) {
					stalenessHours = (Date.now() - t) / (1000 * 60 * 60);
				}
			}

			const sessionLog = allData[BRAIN_KEYS.SESSION_LOG];
			const sessionLogEntries = Array.isArray(sessionLog) ? sessionLog : [];
			const sessionLogPreview = sessionLogEntries
				.slice(-3)
				.reverse()
				.map((e: any) => {
					if (!e || typeof e !== 'object') return { date: '', summary: '' };
					return {
						date: typeof e.date === 'string' ? e.date : '',
						summary: typeof e.summary === 'string' ? e.summary : ''
					};
				});

			let sessionTimelineCount = 0;
			let sessionTimelinePreview: Array<{ timestamp: string; event: string }> = [];
			if (includeAdvanced) {
				try {
					const timeline = await MemoryClient.getSessionTimeline(20);
					sessionTimelineCount = timeline.count;
					sessionTimelinePreview = timeline.items.slice(-5).reverse().map((entry: any) => {
						const ts = entry && typeof entry.timestamp === 'string' ? entry.timestamp : '';
						const eventObj = entry && entry.event && typeof entry.event === 'object' ? entry.event : {};
						const eventType = Object.keys(eventObj)[0] || 'Unknown';
						const payload = eventObj[eventType] || {};
						return {
							timestamp: ts,
							event: this.formatTimelineEvent(eventType, payload, this.workspaceRoot)
						};
					});
				} catch { }
			}

			let observerDna: any = null;
			let observerDnaOtel: any = null;
			let observerIntent: any = null;
			let observerGit: any = null;
			let agentConfig: any = null;
			let agentReports: any = null;
			let compiledContext: any = null;
			let proactiveRisk: any = null;
			let importanceData: any = null;
			let blastRadius: any = null;
			let causalChain: any = null;
			let promptOptimization: any = null;
			let modelPerformance: any = null;
			let developerProfile: any = null;
			let hierarchyIdentity: any = null;
			let tokenStats: any = null;
			if (includeAdvanced) {
				const [dnaRes, dnaOtelRes, intentRes, gitRes, agentConfigRes, agentReportsRes, tokenStatsRes] = await Promise.allSettled([
					MemoryClient.getObserverDna(),
					MemoryClient.getObserverDnaOtel(),
					MemoryClient.getObserverIntent(),
					MemoryClient.getObserverGit(),
					MemoryClient.getAgentConfigs(),
					MemoryClient.getAgentReports(),
					MemoryClient.getTokenStats(),
				]);
				observerDna = dnaRes.status === 'fulfilled' ? dnaRes.value : null;
				observerDnaOtel = dnaOtelRes.status === 'fulfilled' ? dnaOtelRes.value : null;
				observerIntent = intentRes.status === 'fulfilled' ? intentRes.value : null;
				observerGit = gitRes.status === 'fulfilled' ? gitRes.value : null;
				agentConfig = agentConfigRes.status === 'fulfilled' ? agentConfigRes.value : null;
				agentReports = agentReportsRes.status === 'fulfilled' ? agentReportsRes.value : null;
				tokenStats = tokenStatsRes.status === 'fulfilled' ? tokenStatsRes.value : null;
			}
			const inferredTaskType = (() => {
				const daemonIntent = String(observerIntent?.intent_type || '').toLowerCase();
				if (daemonIntent === 'bug_fixing') return 'bugfix';
				if (daemonIntent === 'refactoring') return 'refactor';
				if (daemonIntent === 'scaffolding' || daemonIntent === 'api_design') return 'new_feature';
				if (daemonIntent === 'exploration') return 'code_review';
				const task = String(allData[BRAIN_KEYS.SESSION_STATE]?.current_task || '').toLowerCase();
				if (task.includes('fix') || task.includes('bug')) return 'bugfix';
				if (task.includes('refactor')) return 'refactor';
				if (task.includes('feature') || task.includes('build') || task.includes('implement')) return 'new_feature';
				return 'code_review';
			})();
			const compileBudget = this.promptPackVariant === 'Small' ? 1200 : this.promptPackVariant === 'Deep' ? 4000 : 2400;
			if (includeAdvanced && activeFile) {
				// compileContext is throttled independently because it reads the full skeleton
				// index from Redis on every call — expensive on cloud tiers. The other calls
				// (risk, importance, blast radius, causal chain) are lightweight and always run.
				const CONTEXT_THROTTLE_MS = 30_000; // 30 seconds
				const now = Date.now();
				const activeFileChanged = activeFile !== this.lastCompiledFile;
				const throttleExpired = (now - this.lastContextCompileMs) > CONTEXT_THROTTLE_MS;

				// Run compileContext when: the active file changed (developer switched files)
				// OR the throttle window has expired since the last compilation.
				const shouldCompile = activeFileChanged || throttleExpired;

				if (shouldCompile) {
					this.lastContextCompileMs = now;
					this.lastCompiledFile = activeFile;
				}

				// The three structure/risk calls always run — they are cheap Redis reads
				// and their data (blast radius, causal chain, risk score) must stay live.
				const [proactiveRiskRes, importanceRes, blastRadiusRes, causalChainRes] = await Promise.allSettled([
					MemoryClient.getProactiveRisk(projectId, activeFile),
					MemoryClient.getImportance(10),
					MemoryClient.getBlastRadius(activeFile),
					MemoryClient.getCausalChain(activeFile),
				]);

				// Compile context only when allowed by the throttle
				if (shouldCompile) {
					try {
						compiledContext = await MemoryClient.compileContext(
							projectId, activeFile, compileBudget, inferredTaskType
						);
					} catch {
						compiledContext = null;
					}
				}
				// If throttled, compiledContext stays null — the panel keeps showing
				// the previously compiled context from the last postMessage update.

				const proactiveEnvelope = proactiveRiskRes.status === 'fulfilled' ? proactiveRiskRes.value : null;
				proactiveRisk = proactiveEnvelope?.warning || null;
				importanceData = importanceRes.status === 'fulfilled' ? importanceRes.value : null;
				blastRadius = blastRadiusRes.status === 'fulfilled'
					? blastRadiusRes.value
					: (proactiveEnvelope?.blast_radius || null);
				causalChain = causalChainRes.status === 'fulfilled' ? causalChainRes.value : null;
			}
			if (includeAdvanced) {
				const [promptOptimizationRes, modelPerformanceRes, developerProfileRes, hierarchyIdentityRes] = await Promise.allSettled([
					MemoryClient.getPromptOptimization(projectId, inferredTaskType),
					MemoryClient.getModelPerformance(projectId),
					MemoryClient.getDeveloperProfile(),
					MemoryClient.resolveHierarchy([projectId], BRAIN_KEYS.IDENTITY, true),
				]);
				promptOptimization = promptOptimizationRes.status === 'fulfilled' ? promptOptimizationRes.value : null;
				modelPerformance = modelPerformanceRes.status === 'fulfilled' ? modelPerformanceRes.value : null;
				developerProfile = developerProfileRes.status === 'fulfilled' ? developerProfileRes.value : null;
				hierarchyIdentity = hierarchyIdentityRes.status === 'fulfilled' ? hierarchyIdentityRes.value : null;
			}

			const ide = detectIDE();
			const rulesCfg = getRulesConfig(ide);

			const categories: Record<string, { keys: string[]; size: number }> = {};
			for (const [key, tax] of Object.entries(TAXONOMY_MAP)) {
				if (!categories[tax]) { categories[tax] = { keys: [], size: 0 }; }
				categories[tax].keys.push(key);
				categories[tax].size += sizeInfo.keys[key] || 0;
			}

			const tasksObject = allData[BRAIN_KEYS.TASKS] || {};
			const pendingTasks: Array<{ title: string }> = [];
			if (tasksObject && typeof tasksObject === 'object') {
				const currentListName = typeof tasksObject.current_list === 'string' ? tasksObject.current_list : null;
				const lists = Array.isArray(tasksObject.lists) ? tasksObject.lists : [];
				const currentList = currentListName
					? lists.find((l: any) => l && l.name === currentListName)
					: (lists.length > 0 ? lists[lists.length - 1] : null);
				const tasks = Array.isArray(currentList?.tasks) ? currentList.tasks : [];
				for (const t of tasks) {
					if (!t || typeof t !== 'object') continue;
					if (t.status && t.status !== 'completed') {
						pendingTasks.push({ title: t.title || t.task || 'Untitled task' });
					}
				}
			}

			vscode.commands.executeCommand('setContext', 'memix.isConnected', true);
			vscode.commands.executeCommand('setContext', 'memix.isInitialized', isInitialized);

			this._view.webview.postMessage({
				command: 'update',
				data: {
					connected: true,
					workspaceRoot: this.workspaceRoot ?? '',
					advancedDataLoaded: includeAdvanced,
					totalSizeBytes: sizeInfo.totalBytes,
					redisUsedBytes,
					redisMaxBytes,
					redisMaxEstimated,
					keys: sizeInfo.keys,
					receivedKeys: Object.keys(sizeInfo.keys),
					categories,
					health: healthReport.status,
					lastUpdated: lastUpdatedRaw || 'Never',
					sessionNumber: allData[BRAIN_KEYS.SESSION_STATE]?.session_number || 0,
					currentTask: allData[BRAIN_KEYS.SESSION_STATE]?.current_task || 'None',
					keyCount: Object.keys(sizeInfo.keys).length,
					promptPack,
					promptPackSectionCount: promptPackData.requestedSectionCount,
					promptPackRequestedSectionCount: promptPackData.requestedSectionCount,
					promptPackAvailableSectionCount: promptPackData.availableSectionCount,
					promptPackMissingSections: promptPackData.missingSections,
					promptPackObserverSectionCount: promptPackData.observerSectionCount,
					promptPackIncludedSectionTitles: promptPackData.includedSectionTitles,
					promptPackTokens,
					promptPackVariant: this.promptPackVariant,
					keyCoverage,
					keyCoverageSummary: {
						missingRequired: missingRequiredCoverage.length,
						missingRecommended: missingRecommendedCoverage.length,
						notGenerated: generatedMissingCoverage.length
					},
					recommendations: healthReport.recommendations,
					pendingTasks,
					isInitialized,
					isPaused,
					missingRequiredKeys,
					fixableMissingRequiredKeys: missingRequiredCoverage.map((entry) => entry.key),
					stalenessHours,
					sessionLogCount: sessionLogEntries.length,
					sessionLogPreview,
					activeFile,
					inferredTaskType,
					ide,
					rulesPath: (this.workspaceRoot ? (this.workspaceRoot + '/' + rulesCfg.rulesDir + '/' + rulesCfg.rulesFile) : (rulesCfg.rulesDir + '/' + rulesCfg.rulesFile)),
					rulesDir: rulesCfg.rulesDir,
					rulesFile: rulesCfg.rulesFile,
					metrics: {
						decisions: Array.isArray(allData[BRAIN_KEYS.DECISIONS]) ? allData[BRAIN_KEYS.DECISIONS].length : 0,
						facts: Object.keys(sizeInfo.keys).filter(k => k.startsWith('fact:') || k === BRAIN_KEYS.IDENTITY).length,
						patterns: Array.isArray(allData[BRAIN_KEYS.PATTERNS]?.architectural_rules) ? allData[BRAIN_KEYS.PATTERNS].architectural_rules.length : 0,
						warnings: Array.isArray(allData[BRAIN_KEYS.KNOWN_ISSUES]) ? allData[BRAIN_KEYS.KNOWN_ISSUES].length : 0
					},
					...(includeAdvanced ? {
						sessionTimelineCount,
						sessionTimelinePreview,
						observerDna,
						observerDnaOtel,
						observerIntent,
						observerGit,
						agentConfig,
						agentReports,
						importanceData,
						blastRadius,
						causalChain,
						compiledContext,
						proactiveRisk,
						promptOptimization,
						modelPerformance,
						developerProfile,
						hierarchyIdentity,
						tokenStats
					} : {})
				}
			});
		} catch (e) {
			vscode.commands.executeCommand('setContext', 'memix.isConnected', false);
			vscode.commands.executeCommand('setContext', 'memix.isInitialized', false);

			const msg = e instanceof Error ? e.message : String(e);
			this._view.webview.postMessage({
				command: 'state',
				data: {
					connected: false,
					isInitialized: false,
					isPaused: false,
					reason: msg
				}
			});
		}
	}

	async sendPatternUpdate(): Promise<void> {
		if (!this._view) { return; }
		try {
			// PatternEngine.analyze() is expensive — runs on demand, not on every refresh
			const report = await MemoryClient.getPatternReport();
			this._view.webview.postMessage({ command: 'patternReport', data: report });
		} catch {
			// Send empty report so the UI shows a clean "no data" state
			this._view.webview.postMessage({
				command: 'patternReport',
				data: {
					patterns: [],
					total_files_scanned: 0,
					total_functions_analyzed: 0,
					scan_duration_ms: 0
				}
			});
		}
	}

	private getTailwindCSS(): string {
		try {
			const fs = require('fs');
			const path = require('path');
			// app.css is compiled to media/app.css and included with the extension package
			const cssPath = path.join(this.extensionUri.fsPath, 'media', 'app.css');
			return fs.readFileSync(cssPath, 'utf8');
		} catch {
			return '';
		}
	}

	private getWebviewScript(): string {
		try {
			const fs = require('fs');
			const path = require('path');
			// panel.js is compiled/copied to media/panel.js at build time
			const jsPath = path.join(this.extensionUri.fsPath, 'media', 'panel.js');
			return fs.readFileSync(jsPath, 'utf8');
		} catch {
			return '/* panel.js not found */';
		}
	}

	private getHtml(): string {
		return /* html */`<!DOCTYPE html>
<html>
<head>
	<meta charset="UTF-8">
	<meta name="viewport" content="width=device-width, initial-scale=1.0">
	<style>
		${this.getTailwindCSS()}
	</style>
	<script>
		${this.getWebviewScript()}
	</script>
</head>
<body class="bg-[--vscode-sideBar-background] text-[--vscode-foreground] text-xs p-2">
	<div id="loading-overlay" class="fixed top-0 left-0 w-full h-full flex flex-col items-center justify-center hidden" hidden>
		<svg class="spinner mb-6" width="14" height="14" viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
			<path d="M8 2a6 6 0 100 12A6 6 0 008 2z" stroke="currentColor" strokeWidth="1.5" stroke-dasharray="27 10" strokeLinecap="round"/>
		</svg>
		<div id="loading-text" class="text-[--vscode-foreground]">Connecting to Memix...</div>
	</div>

	<div id="error-banner" class="text-danger mb-3 hidden"></div>
	<div id="hover-widget" class="hover-widget" hidden></div>
	<div id="payload-modal-backdrop" class="modal-backdrop" hidden>
		<div class="modal-shell" role="dialog" aria-modal="true" aria-labelledby="payload-modal-title">
			<div class="modal-header">
				<div>
					<div id="payload-modal-title" class="modal-title">Details</div>
					<button id="payload-modal-close" class="icon-btn" title="Close dialog">✕</button>
				</div>
				<div id="payload-modal-subtitle" class="modal-subtitle"></div>
			</div>
			<div id="payload-modal-body" class="modal-body"></div>
			<div class="modal-actions">
				<button id="payload-modal-copy" class="action-btn">Copy</button>
				<button id="payload-modal-done" class="action-btn">Done</button>
			</div>
		</div>
	</div>

	<div id="empty" class="empty open">
		<div class="empty-inner">
			<svg version="1.2" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 440 423" height="40"><path fill="currentColor" id="Shape 1" d="m40 314v-248l105 102 149-163h106v412l-106-103v-153l-74 80-74.6-72.51v145.51z"/></svg>
			<div id="empty-title" class="empty-title">Initialize Your Brain</div>
			<div id="empty-sub" class="empty-sub">To use Memix, connect your Redis and initialize your brain for this workspace.</div>
			<div style="display:flex;gap:8px;margin-top:10px;flex-direction:column;width:100%">
				<button id="btn-empty-action" class="action-btn">Initialize Brain</button>
				<button id="btn-empty-resume" class="action-btn">Wake Brain Up</button>
			</div>
		</div>
	</div>

	<div id="main" style="display:none">
		<div class="tabs">
			<button id="tab-overview" class="tab active">Overview</button>
			<button id="tab-advanced" class="tab">Advanced <span id="advanced-badge" class="badge" style="display:none">0</span></button>
			<button id="tab-settings" class="tab">Settings</button>
		</div>
		<div id="view-overview" class="view active">
			<div class="w-full py-8 px-3 border-b border-bottom">
				<h3 class="text-base font-semibold mb-2 w-full">Brain Status</h3>
				<div class="stat">
					<span>Health</span>
					<span id="health" class="stat-value">\u2014</span>
				</div>
				<div class="stat">
					<span>Memix Size</span>
					<span id="size" class="stat-value">\u2014</span>
				</div>
				<div class="stat" style="margin-top: 4px">
					<span>Redis Dataset <button id="redis-max-edit" class="icon-btn" title="Set Redis max memory override">✎</button></span>
					<span id="redis-size-text" class="stat-value">\u2014</span>
				</div>
				<div class="bar mb-2 mt-2"><div id="redis-size-bar" class="bar-fill" style="width:0%;background:#4ec9b0"></div></div>
				<div class="stat">
					<span>Keys</span>
					<span id="keyCount" class="stat-value">\u2014</span>
				</div>
				<div class="stat">
					<span>Session</span>
					<span id="session" class="stat-value">\u2014</span>
				</div>
				<div class="stat">
					<span>Last Updated</span>
					<span id="lastUpdated" class="stat-value">\u2014</span>
				</div>
				<div class="stat">
					<span>Current Task</span>
					<span id="currentTask" class="stat-value" style="max-width:140px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">\u2014</span>
				</div>
			</div>
			<div class="w-full py-8 px-3 border-b border-bottom">
				<h3 class="text-base font-semibold mb-2 w-full">Memory Categories</h3>
				<div id="categories"></div>
			</div>
			<div class="w-full py-8 px-3">
				<h3 class="text-base font-semibold mb-2 w-full">Warnings</h3>
				<div class="mt-2" id="warnings"><span>None</span></div>
			</div>
		</div>
		<div id="view-advanced" class="view">
			<div class="w-full py-8 px-3 border-b border-bottom">
				<h3 class="text-base font-semibold mb-2 w-full">Intelligence Metrics</h3>
				<div class="stat">
					<span>Decisions</span>
					<span id="metric-decisions" class="stat-value">0</span>
				</div>
				<div class="stat">
					<span>Core Facts</span>
					<span id="metric-facts" class="stat-value">0</span>
				</div>
				<div class="stat">
					<span>Patterns</span>
					<span id="metric-patterns" class="stat-value">0</span>
				</div>
				<div class="stat">
					<span>Anti-Patterns</span>
					<span id="metric-warnings" class="stat-value">0</span>
				</div>
			</div>
			<div class="w-full py-8 px-3 border-b border-bottom">
				<h3 class="text-base font-semibold mb-2 w-full">Token Intelligence</h3>
				<div class="stat">
					<span>Session AI Tokens</span>
					<span id="token-session-ai" class="stat-value">—</span>
				</div>
				<div class="stat">
					<span>Context Compiled</span>
					<span id="token-session-compiled" class="stat-value">—</span>
				</div>
				<div class="stat">
					<span>Tokens Saved</span>
					<span id="token-session-saved" class="stat-value">—</span>
				</div>
				<div class="stat">
					<span>Files Indexed</span>
					<span id="token-session-files" class="stat-value">—</span>
				</div>
				<div class="stat">
					<span>Context Compilations</span>
					<span id="token-session-compilations" class="stat-value">—</span>
				</div>
				<div class="stat">
					<span>Cache Efficiency</span>
					<span id="token-cache-efficiency" class="stat-value">—</span>
				</div>
				<div class="stat">
					<span>Compression Ratio</span>
					<span id="token-compression-ratio" class="stat-value">—</span>
				</div>
				<div class="mt-3 pt-3 border-t border-[--vscode-panel-border]" style="opacity:0.8">
					<div class="stat" style="font-size:11px">
						<span>Lifetime AI Tokens</span>
						<span id="token-lifetime-ai" class="stat-value">—</span>
					</div>
					<div class="stat" style="font-size:11px">
						<span>Lifetime Saved</span>
						<span id="token-lifetime-saved" class="stat-value">—</span>
					</div>
					<div class="stat" style="font-size:11px">
						<span>Sessions</span>
						<span id="token-lifetime-sessions" class="stat-value">—</span>
					</div>
				</div>
			</div>
			<div class="w-full py-8 px-3 border-b border-bottom">
				<h3 class="text-base font-semibold mb-2 w-full">Integrity & Freshness</h3>
				<div class="stat">
					<span>Required Keys</span>
					<span id="required-keys-status" class="stat-value">—</span>
				</div>
				<div class="mt-2 w-full flex items-start gap-x-2">
					<div class="relative w-4 h-2 before:absolute before:top-0 before:left-0 before:bottom-0 before:w-[1px] before:bg-select after:absolute after:bottom-0 after:right-0 after:left-0 after:h-[1px] after:bg-select"></div>
					<div id="missing-required-keys"></div>
				</div>
				<div class="w-full">
					<button id="fix-missing-keys" class="action-btn w-full" hidden>Restore baseline keys</button>
				</div>
				<div class="stat">
					<span>Staleness</span>
					<span id="staleness" class="stat-value">—</span>
				</div>
			</div>
			<div class="w-full py-8 px-3 border-b border-bottom">
				<h3 class="text-base font-semibold mb-2 w-full flex items-center justify-between">
					<span>Missing/Pending Tasks</span>
					<span class="text-sm font-normal" id="pending-tasks-count">0</span>
				</h3>
				<div id="pending-tasks-container">
					<span>No pending tasks</span>
				</div>
			</div>
			<div class="w-full py-8 px-3 border-b border-bottom">
				<h3 class="text-base font-semibold mb-2 w-full flex items-center justify-between">
					<span>Session Log</span>
					<span class="text-sm font-normal" id="session-log-count">0</span>
				</h3>
				<div id="session-log-preview">No entries</div>
			</div>
			<div class="w-full py-8 px-3 border-b border-bottom">
				<h3 class="text-base font-semibold mb-2 w-full flex items-center justify-between">
					<span>Daemon Timeline</span>
					<span class="text-sm font-normal" id="session-timeline-count">0</span>
				</h3>
				<div id="session-timeline-preview">No events</div>
			</div>
			<div class="w-full py-8 px-3 border-b border-bottom">
				<h3 class="text-base font-semibold mb-2 w-full">Observer Code DNA</h3>
				<div class="stat">
					<span>Architecture</span>
					<span id="observer-dna-architecture" class="stat-value">—</span>
				</div>
				<div class="stat">
					<span>Files</span>
					<span id="observer-dna-files" class="stat-value">—</span>
				</div>
				<div class="stat">
					<span>Symbols</span>
					<span id="observer-dna-symbols" class="stat-value">—</span>
				</div>
				<div class="stat">
					<span>Depth</span>
					<span id="observer-dna-depth" class="stat-value">—</span>
				</div>
				<div class="stat">
					<span>Complexity Score</span>
					<span id="observer-dna-complexity" class="stat-value">—</span>
				</div>
				<div class="stat">
					<span>Typed</span>
					<span id="observer-dna-typed" class="stat-value">—</span>
				</div>
				<div id="observer-dna-explainability" class="text-[11px]" style="color:var(--vscode-descriptionForeground)">No DNA snapshot</div>
				<div id="observer-dna-patterns" class="mt-5">
					<div id="pattern-stats" class="text-[11px] mb-2"></div>
					<div id="patterns-known"></div>
					<div id="patterns-framework"></div>
					<div id="patterns-emergent"></div>
					<div class="mt-4 w-full">
						<button id="btn-scan-patterns" class="action-btn w-full">Scan Patterns</button>
					</div>
				</div>
				<div id="observer-dna-languages" class="mt-2"></div>
				<div id="observer-dna-rules" class="mt-2"></div>
				<div id="observer-dna-hot-zones" class="mt-2"></div>
				<div id="observer-dna-stable-zones" class="mt-2"></div>
			</div>
			<div class="w-full py-8 px-3 border-b border-bottom">
				<h3 class="text-base font-semibold mb-2 w-full">Observer DNA OTel Export</h3>
				<div id="observer-dna-otel-summary" class="summary-row">No OTel export</div>
				<div class="flex items-center gap-x-2 mt-2">
					<button id="observer-dna-otel-open" class="action-btn w-full">View JSON</button>
					<button id="observer-dna-otel-copy" class="action-btn w-full">Copy OTel</button>
				</div>
			</div>
			<div class="w-full py-8 px-3 border-b border-bottom">
				<h3 class="text-base font-semibold mb-2 w-full">Predictive Intent</h3>
				<div class="stat">
					<span>Intent</span>
					<span id="observer-intent-type" class="stat-value">—</span>
				</div>
				<div class="stat">
					<span>Active File</span>
					<span id="observer-intent-active-file" class="stat-value" style="font-weight:normal;max-width:140px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">—</span>
				</div>
				<div id="observer-intent-related-files" style="margin-top:6px;color:var(--vscode-descriptionForeground)">No predictive snapshot</div>
				<div id="observer-intent-rationale" style="margin-top:6px"></div>
			</div>
			<div class="w-full py-8 px-3 border-b border-bottom">
				<h3 class="text-base font-semibold mb-2 w-full">Git Archaeology</h3>
				<div class="stat">
					<span>Repo Root</span>
					<span id="observer-git-repo" class="stat-value" style="font-weight:normal;max-width:140px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">—</span>
				</div>
				<div id="observer-git-authors" style="margin-top:6px;color:var(--vscode-descriptionForeground)">No archaeology snapshot</div>
				<div id="observer-git-hot-files" style="margin-top:6px"></div>
			</div>
			<div class="w-full py-8 px-3 border-b border-bottom">
				<h3 class="text-base font-semibold mb-2 w-full">Architecture X-Ray</h3>
				<div id="importance-summary" style="color:var(--vscode-descriptionForeground)">No structural graph data</div>
				<div id="importance-top-files" style="margin-top:6px"></div>
			</div>
			<div class="w-full py-8 px-3 border-b border-bottom">
				<h3 class="text-base font-semibold mb-2 w-full">Blast Radius</h3>
				<div id="blast-radius-summary" style="color:var(--vscode-descriptionForeground)">No blast radius available</div>
				<div id="blast-radius-details" style="margin-top:6px"></div>
			</div>
			<div class="w-full py-8 px-3 border-b border-bottom">
				<h3 class="text-base font-semibold mb-2 w-full">Causal Chain</h3>
				<div id="causal-chain-summary" style="color:var(--vscode-descriptionForeground)">No causal chain available</div>
				<div id="causal-chain-details" style="margin-top:6px"></div>
			</div>
			<div class="w-full py-8 px-3 border-b border-bottom">
				<h3 class="text-base font-semibold mb-2 w-full">Daemon Agents</h3>
				<div id="agent-config-summary" style="color:var(--vscode-descriptionForeground)">No agent runtime data</div>
				<div id="agent-reports-summary" style="margin-top:6px"></div>
			</div>
			<div class="w-full py-8 px-3 border-b border-bottom">
				<h3 class="text-base font-semibold mb-2 w-full">Compiled Context</h3>
				<div id="compiled-context-summary" style="color:var(--vscode-descriptionForeground)">No compiled context</div>
				<div id="compiled-context-sections" style="margin-top:6px"></div>
			</div>
			<div class="w-full py-8 px-3 border-b border-bottom">
				<h3 class="text-base font-semibold mb-2 w-full">Proactive Risk</h3>
				<div id="proactive-risk-summary" style="color:var(--vscode-descriptionForeground)">No risk signal</div>
				<div id="proactive-risk-details" style="margin-top:6px"></div>
			</div>
			<div class="w-full py-8 px-3 border-b border-bottom">
				<h3 class="text-base font-semibold mb-2 w-full">Learning Layer</h3>
				<div id="prompt-optimization-summary" style="color:var(--vscode-descriptionForeground)">No learning data</div>
				<div id="model-performance-summary" style="margin-top:6px"></div>
				<div id="developer-profile-summary" style="margin-top:6px"></div>
			</div>
			<div class="w-full py-8 px-3 border-b border-bottom">
				<h3 class="text-base font-semibold mb-2 w-full">Hierarchy Resolution</h3>
				<div id="hierarchy-resolution-summary" style="color:var(--vscode-descriptionForeground)">No hierarchy resolution</div>
				<div style="margin-top: 8px; width: 100%;">
					<button id="hierarchy-resolution-open" class="action-btn w-full">View JSON</button>
				</div>
			</div>
			<div class="w-full py-8 px-3 border-b border-bottom">
				<h3 class="text-base font-semibold mb-2 w-full">IDE Rules Output</h3>
				<div class="stat">
					<span>IDE</span>
					<span id="ide" class="stat-value">—</span>
				</div>
				<div class="stat">
					<span>Rules Path</span>
					<span id="rules-path" class="stat-value" title="" style="max-width:140px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">—</span>
				</div>
			</div>
			<div class="w-full py-8 px-3 border-b border-bottom">
				<h3 class="text-base font-semibold mb-2 w-full">Top Memory Vectors (Size)</h3>
				<div id="key-sizes" class="key-table"></div>
			</div>
			<div class="w-full py-8 px-3 border-b border-bottom">
				<h3 class="text-base font-semibold mb-2 w-full">Brain Key Coverage</h3>
				<div id="key-coverage" class="key-table"></div>
			</div>
			<div class="w-full py-8 px-3">
				<h3 class="text-base font-semibold mb-2 w-full">Prompt Pack</h3>
				<div id="prompt-pack-meta" class="mb-4">Tokens: —</div>
				<div class="flex items-center gap-x-2">
					<select id="prompt-pack-variant" class="w-full bg-select border pl-2 pr-4 py-1.5 border-0 focus:ring-0 focus:outline-none rounded-none" aria-label="Prompt Pack Variant">
						<option value="Small">Small</option>
						<option value="Standard" selected>Standard</option>
						<option value="Deep">Deep</option>
					</select>
					<button id="view-prompt-pack" class="w-full action-btn">View</button>
				</div>
				<div id="prompt-pack-summary" class="mt-4 summary-row">Prompt Pack unavailable</div>
			</div>
		</div>
		<div id="view-settings" class="view">
			<div class="w-full py-8 px-3 border-b border-bottom">
				<h3 class="text-base font-semibold mb-2 w-full">Global Control</h3>
				<div class="setting-row">
					<div class="setting-info">
						<div class="setting-title">Pause Brain</div>
						<div class="setting-desc">Suspends all memory ingestion, AST analysis, and background processing. Memory reads and AI chat remain available.</div>
					</div>
					<label class="switch">
						<input type="checkbox" id="toggle-brain-pause">
						<span class="slider"></span>
					</label>
				</div>
			</div>
                        <div class="w-full py-8 px-3 border-b border-bottom">
                            <h3 class="text-base font-semibold mb-2 w-full">Redis Connection</h3>
                            <div class="setting-row">
                                <div class="setting-info">
                                    <div class="setting-title">Change Redis URL</div>
                                    <div class="setting-desc">Switch to a different Redis instance. The current URL is saved securely in your OS keychain.</div>
                                </div>
                                <button id="btn-change-redis" class="action-btn" style="white-space:nowrap">Change</button>
                            </div>
                        </div>
                        <div class="w-full py-8 px-3 border-b border-bottom">
                            <h3 class="text-base font-semibold mb-2 w-full">Version</h3>
                            <div class="setting-row">
                                <div class="setting-info">
                                    <div class="setting-title">Current Version</div>
                                    <div class="setting-desc">Shows installed Memix daemon and extension versions.</div>
                                </div>
                                <button id="btn-version-info" class="action-btn" style="white-space:nowrap">View</button>
                            </div>
                        </div>
			<div class="w-full py-8 px-3" id="settings-config-info">
				<h3 class="text-base font-semibold mb-2 w-full">Config</h3>
				<div class="setting-row" style="border:none;padding:4px 0">
					<span id="settings-config-path" style="max-width:100%;overflow:hidden;text-overflow:ellipsis;white-space:nowrap" class="stat-value">—</span>
				</div>
			</div>
		</div>
	</div>
</body>
</html>`;
	}
}
