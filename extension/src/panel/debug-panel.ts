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

	private getCenteredPayloadHtml(title: string, payload: string, subtitle?: string) {
		const safeTitle = this.escapeWebviewHtml(title);
		const safeSubtitle = this.escapeWebviewHtml(subtitle || '');
		const safePayload = this.escapeWebviewHtml(payload);
		return /* html */`<!DOCTYPE html>
<html>
<head>
	<style>
		body {
			margin: 0;
			padding: 24px;
			font-family: var(--vscode-font-family);
			background: var(--vscode-editor-background);
			color: var(--vscode-foreground);
		}
		.shell {
			max-width: 980px;
			margin: 0 auto;
			border: 1px solid rgba(255, 255, 255, 0.12);
			border-radius: 14px;
			overflow: hidden;
			box-shadow: 0 24px 80px rgba(0, 0, 0, 0.28);
			background: color-mix(in srgb, var(--vscode-editor-background) 92%, black 8%);
		}
		.header {
			display: flex;
			justify-content: space-between;
			align-items: flex-start;
			gap: 12px;
			padding: 18px 20px 14px;
			border-bottom: 1px solid rgba(255, 255, 255, 0.08);
		}
		.title {
			font-size: 18px;
			font-weight: 700;
		}
		.subtitle {
			margin-top: 6px;
			font-size: 12px;
			line-height: 1.5;
			color: var(--vscode-descriptionForeground);
		}
		pre {
			margin: 0;
			padding: 20px;
			overflow: auto;
			font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;
			font-size: 12px;
			line-height: 1.55;
			white-space: pre-wrap;
			word-break: break-word;
		}
	</style>
</head>
<body>
	<div class="shell">
		<div class="header">
			<div>
				<div class="title">${safeTitle}</div>
				<div class="subtitle">${safeSubtitle}</div>
			</div>
			<button id="copy" class="action-btn">Copy</button>
		</div>
		<pre>${safePayload}</pre>
	</div>
	<script>
		const vscode = acquireVsCodeApi();
		document.getElementById('copy').addEventListener('click', function() {
			vscode.postMessage({ command: 'copyText' });
		});
	</script>
</body>
</html>`;
	}

	private openCenteredPayloadView(title: string, payload: string, subtitle?: string, notice?: string) {
		const panel = vscode.window.createWebviewPanel(
			'memix.payloadView',
			title,
			vscode.ViewColumn.Active,
			{ enableScripts: true, retainContextWhenHidden: false }
		);
		panel.webview.html = this.getCenteredPayloadHtml(title, payload, subtitle);
		panel.webview.onDidReceiveMessage(async (msg) => {
			if (msg?.command === 'copyText') {
				await vscode.env.clipboard.writeText(payload);
				if (notice) {
					vscode.window.showInformationMessage(notice);
				}
			}
		});
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
							event: `${eventType}: ${JSON.stringify(payload)}`
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
			let promptOptimization: any = null;
			let modelPerformance: any = null;
			let developerProfile: any = null;
			let hierarchyIdentity: any = null;
			if (includeAdvanced) {
				const [dnaRes, dnaOtelRes, intentRes, gitRes, agentConfigRes, agentReportsRes] = await Promise.allSettled([
					MemoryClient.getObserverDna(),
					MemoryClient.getObserverDnaOtel(),
					MemoryClient.getObserverIntent(),
					MemoryClient.getObserverGit(),
					MemoryClient.getAgentConfigs(),
					MemoryClient.getAgentReports(),
				]);
				observerDna = dnaRes.status === 'fulfilled' ? dnaRes.value : null;
				observerDnaOtel = dnaOtelRes.status === 'fulfilled' ? dnaOtelRes.value : null;
				observerIntent = intentRes.status === 'fulfilled' ? intentRes.value : null;
				observerGit = gitRes.status === 'fulfilled' ? gitRes.value : null;
				agentConfig = agentConfigRes.status === 'fulfilled' ? agentConfigRes.value : null;
				agentReports = agentReportsRes.status === 'fulfilled' ? agentReportsRes.value : null;
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
				const [compiledContextRes, proactiveRiskRes] = await Promise.allSettled([
					MemoryClient.compileContext(projectId, activeFile, compileBudget, inferredTaskType),
					MemoryClient.getProactiveRisk(projectId, activeFile),
				]);
				compiledContext = compiledContextRes.status === 'fulfilled' ? compiledContextRes.value : null;
				proactiveRisk = proactiveRiskRes.status === 'fulfilled' ? proactiveRiskRes.value : null;
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
						compiledContext,
						proactiveRisk,
						promptOptimization,
						modelPerformance,
						developerProfile,
						hierarchyIdentity
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

	private getHtml(): string {
		return /* html */`<!DOCTYPE html>
<html>
<head>
	<style>
		body {
			font-family: var(--vscode-font-family);
			color: var(--vscode-foreground);
			background: var(--vscode-sideBar-background);
			padding: 10px;
			font-size: 12px;
		}
		#loading-overlay {
			position: fixed;
			top: 0; left: 0; width: 100%; height: 100%;
			background: var(--vscode-editor-background);
			display: flex;
			flex-direction: column;
			align-items: center;
			justify-content: center;
			z-index: 9999;
			opacity: 0.85;
		}
		#loading-overlay[hidden] { display: none; }
		.spinner {
			animation: spin 1s linear infinite;
			margin-bottom: 12px;
			color: var(--vscode-textLink-foreground);
		}
		@keyframes spin { 100% { transform: rotate(360deg); } }
		.header {
			display: flex;
			justify-content: space-between;
			align-items: center;
			gap: 8px;
			margin-bottom: 10px;
		}

		:root {
			--memix-primary-color: var(--vscode-editor-foreground);
		}

		.tabs {
			display: flex;
			margin-bottom: 10px;
		}
		.tab {
			flex: 1;
			border: none;
			border-bottom: 2px solid color-mix(in srgb, var(--vscode-sideBar-background) 80%, var(--memix-primary-color) 10%);
			background: transparent;
			color: color-mix(in srgb, var(--memix-primary-color) 50%, var(--vscode-descriptionForeground) 30%);
			padding: 8px 12px;
			font-size: 13px;
			font-weight: 400;
			letter-spacing: 0.01em;
			transition: color 0.15s ease, border-color 0.15s ease;
			cursor: pointer;
		}
		.tab span {
			vertical-align: middle;
		}
		.tab.active {
			color: var(--memix-primary-color);
			border-bottom-color: var(--memix-primary-color);
		}
		.view { display: none; }
		.view.active { display: block; }
		.empty {
			display: none;
			min-height: calc(100vh - 90px);
			align-items: center;
			justify-content: center;
			text-align: center;
			padding: 20px 10px;
		}
		.empty.open { display: flex; }
		.empty-inner {
			display: flex;
			flex-direction: column;
			align-items: center;
			gap: 10px;
			max-width: 240px;
		}
		.empty-title {
			font-size: 12px;
			font-weight: 600;
		}
		.empty-sub {
			font-size: 11px;
			color: var(--vscode-descriptionForeground);
			line-height: 1.4;
		}
		.card {
			background: transparent;
			border-top: 1px solid rgba(255, 255, 255, 0.07);
			padding: 25px 10px;
			margin-bottom: 8px;
		}
		.card h3 {
			margin: 0 0 12px 0;
			font-size: 14px;
			color: var(--memix-primary-color);
		}
		.stat {
			display: flex;
			justify-content: space-between;
			padding: 4px 0;
			border: 0;
		}
		.icon-btn {
			background: transparent;
			border: 0;
			color: var(--vscode-foreground);
			cursor: pointer;
			font-size: 12px;
			padding: 0 4px;
			opacity: 0.8;
		}
		.icon-btn:hover { opacity: 1; }
		.select {
			background: transparent;
			border: 1px solid rgba(255, 255, 255, 0.12);
			color: var(--vscode-foreground);
			border-radius: 6px;
			padding: 4px 8px 4px 6px;
			font-size: 11px;
		}
		.mono {
			font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;
			font-size: 11px;
			line-height: 1.35;
			white-space: pre-wrap;
			word-break: break-word;
		}
		.stat:last-child { border-bottom: none; }
		.stat-value {
			font-weight: bold;
			font-size: 11px
		}
		.health-healthy { color: #4ec9b0; }
		.health-warning { color: #cca700; }
		.health-critical { color: #f44747; }
		.bar {
			height: 6px;
			background: var(--vscode-panel-border);
			border-radius: 3px;
			margin: 4px 0;
			overflow: hidden;
		}
		.bar-fill {
			height: 100%;
			border-radius: 3px;
			transition: width 0.3s;
		}
		.key-table {
			display: grid;
			grid-template-columns: 1fr auto;
			gap: 6px 10px;
			font-size: 12px;
			color: var(--vscode-descriptionForeground);
		}
		.key-table .k { color: var(--vscode-foreground); }
		.key-label {
			display: inline-flex;
			align-items: center;
			gap: 6px;
		}
		.info-icon {
			display: inline-flex;
			align-items: center;
			justify-content: center;
			width: 14px;
			height: 14px;
			font-size: 10px;
			line-height: 1;
			color: var(--vscode-descriptionForeground);
		}
		.info-icon svg {
			width: 11px;
			height: 11px;
			fill: currentColor;
			strokeWidth: 2px;
		}
		.hover-widget {
			/* Positioning */
			position: fixed;
			z-index: 1000;
			white-space: normal;
			max-width: 280px;
			border: 1px solid var(--vscode-editorHoverWidget-border, #454545);
			border-radius: 4px;
			padding: 8px 12px;
			background: var(--vscode-editorHoverWidget-background);
			border-color: var(--vscode-editorHoverWidget-border);
			/* Text colors */
			color: var(--vscode-editorHoverWidget-foreground);
			/* Optional shadow */
			box-shadow: 0 2px 8px var(--vscode-widget-shadow);
			/* Typography */
			font-family: var(--vscode-font-family);
			font-size: 11px;
			line-height: 1.45;
			pointer-events: none;
		}
		.warning { font-size: 11px; margin: 4px 0; }
		.card-actions {
			display: flex;
			gap: 6px;
			align-items: center;
			flex-wrap: wrap;
			margin-top: 4px;
		}
		.action-btn {
			background: var(--vscode-button-background);
			color: var(--vscode-button-foreground);
			border: 1px solid var(--vscode-button-border, transparent);
			border-radius: 4px;
			padding: 6px 14px;
			font-size: 13px;
			font-weight: 400;
			cursor: pointer;
			display: inline-flex;
			align-items: center;
			justify-content: center;
			transition: background 0.1s ease;
		}
		.action-btn:hover {
			background: var(--vscode-button-hoverBackground);
		}
		.action-btn.secondary {
			background: var(--vscode-button-secondaryBackground, rgba(128,128,128,0.1));
			color: var(--vscode-button-secondaryForeground, var(--vscode-foreground));
			border-color: var(--vscode-button-secondaryBorder, transparent);
			padding: 4px 10px;
			font-size: 11px;
		}
		.action-btn.secondary:hover {
			background: var(--vscode-button-secondaryHoverBackground, rgba(128,128,128,0.2));
		}
		.action-btn[hidden] { display: none; }
		.action-btn:disabled {
			opacity: 0.5;
			cursor: not-allowed;
		}
		.summary-row {
			font-size: 11px;
			line-height: 1.5;
			color: var(--vscode-descriptionForeground);
		}
		.summary-row strong { color: var(--vscode-foreground); }
		.coverage-summary {
			margin-bottom: 8px;
			font-size: 11px;
			line-height: 1.5;
			color: var(--vscode-descriptionForeground);
		}
		.status-ok { color: #4ec9b0; }
		.status-warning { color: #cca700; }
		.status-danger { color: #f44747; }
		.status-subtle { color: var(--vscode-descriptionForeground); }
		.modal-backdrop {
			position: fixed;
			inset: 0;
			background: rgba(0, 0, 0, 0.5);
			display: flex;
			align-items: center;
			justify-content: center;
			z-index: 10000;
			padding: 16px;
		}
		.modal-backdrop[hidden] { display: none; }
		.modal-shell {
			width: min(760px, 100%);
			max-height: min(80vh, 900px);
			background: var(--vscode-editor-background);
			border: 1px solid rgba(255, 255, 255, 0.12);
			border-radius: 12px;
			display: flex;
			flex-direction: column;
			overflow: hidden;
			box-shadow: 0 20px 60px rgba(0, 0, 0, 0.35);
		}
		.modal-header {
			padding: 12px 14px 8px;
			border-bottom: 1px solid rgba(255, 255, 255, 0.08);
		}
		.modal-header > div {
			display: flex;
			justify-content: space-between;
			align-items: center;
			gap: 8px;
		}
		.modal-title {
			font-size: 13px;
			font-weight: 600;
		}
		.modal-subtitle {
			padding: 5px 0 10px;
			font-size: 11px;
			color: var(--vscode-descriptionForeground);
		}
		.modal-body {
			padding: 12px 14px;
			overflow: auto;
			font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;
			font-size: 11px;
			line-height: 1.45;
			white-space: pre-wrap;
			word-break: break-word;
		}
		.modal-actions {
			display: flex;
			justify-content: flex-end;
			gap: 8px;
			padding: 12px 14px;
			border-top: 1px solid rgba(255, 255, 255, 0.08);
		}
		.category { margin: 4px 0 8px; }
		.category-name {
			display: flex;
			justify-content: space-between;
			font-size: 12px;
			text-transform: capitalize;
		}
		.category-name span {
			font-size: 11px;
		}
		.badge {
			background: var(--vscode-badge-background);
			color: var(--vscode-badge-foreground);
			border-radius: 11px;
			padding: 2px 6px;
			font-size: 10px;
			font-weight: 600;
			margin-left: 6px;
		}
		.advanced-grid {
			gap: 8px;
			margin-top: 2px;
		}
		.metric-box {
			display: flex;
			flex-direction: column;
			align-items: center;
		}
		.metric-value {
			width: 100%;
			color: var(--memix-primary-color);
			font-size: 16px;
			font-weight: bold;
		}
		.metric-label {
			flex-shrink: 0;
			font-size: 10px;
			color: var(--vscode-descriptionForeground);
			text-transform: uppercase;
		}
		.task-item {
			display: flex;
			align-items: flex-start;
			gap: 8px;
			padding: 6px 0;
			border-bottom: 1px solid rgba(255, 255, 255, 0.05);
		}
		.task-item:last-child { border-bottom: none; }
		.task-bullet { color: #cca700; margin-top: 2px; }
		#error-banner {
			color: #f44747;
			margin-bottom: 12px;
			display: none;
		}
		
		/* Toggle Switch CSS */
		.setting-row {
			display: flex;
			justify-content: space-between;
			align-items: center;
			padding: 10px 0 15px;
			border-bottom: 1px solid rgba(255, 255, 255, 0.05);
		}
		.setting-row:last-child { border-bottom: none; }
		.setting-info {
			display: flex;
			flex-direction: column;
			gap: 4px;
		}
		.setting-title { font-size: 13px; font-weight: 600; color: var(--vscode-foreground); }
		.setting-desc { margin-top:4px;font-size: 11px; color: var(--vscode-descriptionForeground); max-width: 200px; line-height: 1.3; }
		.switch {
			position: relative;
			display: inline-block;
			width: 38px;
			height: 20px;
			flex-shrink: 0;
		}
		.switch input { opacity: 0; width: 0; height: 0; }
		.slider {
			position: absolute;
			cursor: pointer;
			top: 0; left: 0; right: 0; bottom: 0;
			background-color: var(--vscode-settings-checkboxBackground, rgba(128,128,128,0.2));
			border: 1px solid var(--vscode-settings-checkboxBorder, transparent);
			transition: .15s;
			border-radius: 20px;
		}
		.slider:before {
			position: absolute;
			content: "";
			height: 14px;
			width: 14px;
			left: 2px;
			bottom: 2px;
			background-color: var(--vscode-settings-checkboxForeground, var(--vscode-foreground));
			transition: .15s;
			border-radius: 50%;
			box-shadow: 0 1px 3px rgba(0,0,0,0.2);
		}
		input:checked + .slider {
			background-color: var(--vscode-button-background);
			border-color: var(--vscode-button-border, transparent);
		}
		input:checked + .slider:before {
			transform: translateX(18px);
			background-color: var(--vscode-button-foreground);
		}
		.switch input:disabled + .slider {
			opacity: 0.4;
			cursor: not-allowed;
		}
		}
		input:checked + .slider {
			background-color: var(--memix-primary-color);
		}
		input:checked + .slider:before {
			transform: translateX(14px);
			background-color: var(--vscode-editor-background);
		}
		input:disabled + .slider { opacity: 0.5; cursor: not-allowed; }
	</style>
</head>
<body>
	<div id="loading-overlay" hidden>
		<svg class="spinner" width="14" height="14" viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
			<path d="M8 2a6 6 0 100 12A6 6 0 008 2z" stroke="currentColor" strokeWidth="1.5" stroke-dasharray="27 10" strokeLinecap="round"/>
		</svg>
		<div id="loading-text" style="font-size:12px;color:var(--vscode-foreground)">Connecting to Memix...</div>
	</div>


	<div id="error-banner"></div>
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
				<button id="payload-modal-copy" class="action-btn secondary">Copy</button>
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
				<button id="btn-empty-action" class="action-btn" style="width:100%;padding:10px">Initialize Brain</button>
				<button id="btn-empty-resume" class="action-btn" style="width:100%;padding:10px;display:none">Wake Brain Up</button>
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

	<div class="card">
		<h3>Brain Status</h3>
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
		<div class="bar" style="margin-top: 2px"><div id="redis-size-bar" class="bar-fill" style="width:0%;background:#4ec9b0"></div></div>
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

	<div class="card">
		<h3>Memory Categories</h3>
		<div id="categories"></div>
	</div>

	<div class="card">
		<h3>Warnings</h3>
		<div id="warnings"><span style="color:var(--vscode-descriptionForeground)">None</span></div>
	</div>
		</div>

		<div id="view-advanced" class="view">
			<div class="card">
				<h3>Intelligence Metrics</h3>
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
			
			<div class="card">
				<h3>Integrity & Freshness</h3>
				<div class="stat">
					<span>Required Keys</span>
					<span id="required-keys-status" class="stat-value">—</span>
				</div>
				<div id="missing-required-keys" style="margin-top:2px"></div>
				<div class="card-actions">
					<button id="fix-missing-keys" class="action-btn secondary" hidden>Restore baseline keys</button>
				</div>
				<div class="stat">
					<span>Staleness</span>
					<span id="staleness" class="stat-value">—</span>
				</div>
			</div>
			
			<div class="card">
				<h3 style="display:flex;justify-content:space-between">
					<span>Missing/Pending Tasks</span>
					<span id="pending-tasks-count">0</span>
				</h3>
				<div id="pending-tasks-container">
					<span>No pending tasks</span>
				</div>
			</div>

			<div class="card">
				<h3 style="display:flex;justify-content:space-between">
					<span>Session Log</span>
					<span id="session-log-count">0</span>
				</h3>
				<div id="session-log-preview">No entries</div>
			</div>

			<div class="card">
				<h3 style="display:flex;justify-content:space-between">
					<span>Daemon Timeline</span>
					<span id="session-timeline-count">0</span>
				</h3>
				<div id="session-timeline-preview">No events</div>
			</div>

			<div class="card">
				<h3>Observer Code DNA</h3>
				<div class="stat">
					<span>Architecture</span>
					<span id="observer-dna-architecture" class="stat-value">—</span>
				</div>
				<div class="stat">
					<span>Complexity Score</span>
					<span id="observer-dna-complexity" class="stat-value">—</span>
				</div>
				<div id="observer-dna-summary" style="margin-top:6px;color:var(--vscode-descriptionForeground)">No DNA snapshot</div>
				<div id="observer-dna-patterns" style="margin-top:6px"></div>
				<div id="observer-dna-hot-zones" style="margin-top:6px"></div>
				<div id="observer-dna-stable-zones" style="margin-top:6px"></div>
			</div>

			<div class="card">
				<h3>Observer DNA OTel Export</h3>
				<div id="observer-dna-otel-summary" class="summary-row">No OTel export</div>
				<div class="card-actions">
					<button id="observer-dna-otel-open" class="action-btn secondary">View export</button>
					<button id="observer-dna-otel-copy" class="action-btn secondary">Copy JSON</button>
				</div>
			</div>

			<div class="card">
				<h3>Predictive Intent</h3>
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

			<div class="card">
				<h3>Git Archaeology</h3>
				<div class="stat">
					<span>Repo Root</span>
					<span id="observer-git-repo" class="stat-value" style="font-weight:normal;max-width:140px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">—</span>
				</div>
				<div id="observer-git-authors" style="margin-top:6px;color:var(--vscode-descriptionForeground)">No archaeology snapshot</div>
				<div id="observer-git-hot-files" style="margin-top:6px"></div>
			</div>

			<div class="card">
				<h3>Daemon Agents</h3>
				<div id="agent-config-summary" style="color:var(--vscode-descriptionForeground)">No agent runtime data</div>
				<div id="agent-reports-summary" style="margin-top:6px"></div>
			</div>

			<div class="card">
				<h3>Compiled Context</h3>
				<div id="compiled-context-summary" style="color:var(--vscode-descriptionForeground)">No compiled context</div>
				<div id="compiled-context-sections" style="margin-top:6px"></div>
			</div>

			<div class="card">
				<h3>Proactive Risk</h3>
				<div id="proactive-risk-summary" style="color:var(--vscode-descriptionForeground)">No risk signal</div>
				<div id="proactive-risk-details" style="margin-top:6px"></div>
			</div>

			<div class="card">
				<h3>Learning Layer</h3>
				<div id="prompt-optimization-summary" style="color:var(--vscode-descriptionForeground)">No learning data</div>
				<div id="model-performance-summary" style="margin-top:6px"></div>
				<div id="developer-profile-summary" style="margin-top:6px"></div>
			</div>

			<div class="card">
				<h3>Hierarchy Resolution</h3>
				<div id="hierarchy-resolution-summary" style="color:var(--vscode-descriptionForeground)">No hierarchy resolution</div>
				<div class="card-actions">
					<button id="hierarchy-resolution-open" class="action-btn secondary">View JSON</button>
				</div>
			</div>

			<div class="card">
				<h3>IDE Rules Output</h3>
				<div class="stat">
					<span>IDE</span>
					<span id="ide" class="stat-value">—</span>
				</div>
				<div class="stat">
					<span>Rules Path</span>
					<span id="rules-path" class="stat-value" style="font-weight:normal;max-width:140px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">—</span>
				</div>
			</div>

			<div class="card">
				<h3>Top Memory Vectors (Size)</h3>
				<div id="key-sizes" class="key-table"></div>
			</div>

			<div class="card">
				<h3>Brain Key Coverage</h3>
				<div id="key-coverage" class="key-table"></div>
			</div>

			<div class="card">
				<h3 style="display:flex;justify-content:space-between;align-items:center">
					<span>Prompt Pack</span>
					<span style="display:flex;gap:6px;align-items:center">
						<select id="prompt-pack-variant" class="select" aria-label="Prompt Pack Variant">
							<option value="Small">Small</option>
							<option value="Standard" selected>Standard</option>
							<option value="Deep">Deep</option>
						</select>
						<button id="view-prompt-pack" class="action-btn secondary">View</button>
					</span>
				</h3>
				<div id="prompt-pack-summary" class="summary-row">Prompt Pack unavailable</div>
				<div id="prompt-pack-meta" style="margin-bottom:6px;color:var(--vscode-descriptionForeground)">tokens: —</div>
			</div>
		</div>

		<div id="view-settings" class="view">
			<div class="card">
				<h3>Global Control</h3>
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

			<div class="card" id="settings-config-info">
				<h3>Config</h3>
				<div class="setting-row" style="border:none;padding:4px 0">
					<span id="settings-config-path" class="stat-value" style="font-size:10px;font-weight:200">—</span>
				</div>
			</div>
		</div>
	</div>

	<script>
		window.onerror = function(msg, src, ln, col, err) {
			document.body.innerHTML = '<div style="color:red;padding:20px;font-size:14px;background:transparent;border:1px solid red"><b>FATAL WEBVIEW ERROR:</b><br/>' + msg + '<br/>Line: ' + ln + '</div>';
		};

		let vscode;
		try {
			vscode = acquireVsCodeApi();
		} catch(e) {
			document.body.innerHTML = '<div style="color:red;padding:20px;"><b>API ERROR:</b> ' + e.message + '</div>';
		}

		let spinnerActive = false;
		let lastState = { connected: true, isInitialized: true };
		let lastDaemonState = { kind: 'downloading', title: 'Preparing Memix Daemon', description: 'Checking daemon availability before Memix becomes available.' };
		let hasFirstState = false;
		let advancedHydrated = false;
		let hoverAnchor = null;
		let lastPromptPack = '';
		let lastObserverDnaOtel = '';
		let lastHierarchyResolution = '';
		let activeModalKind = '';
		let activeModalPayload = '';
		function byId(id) { return document.getElementById(id); }
		function escapeHtml(value) {
			return String(value || '')
				.replace(/&/g, '&amp;')
				.replace(/</g, '&lt;')
				.replace(/>/g, '&gt;')
				.replace(/"/g, '&quot;')
				.replace(/'/g, '&#39;');
		}
		
		function showLoading(text) {
			var t = byId('loading-text');
			if (t) t.textContent = text;
			var o = byId('loading-overlay');
			if (o) o.hidden = false;
			spinnerActive = true;
		}
		function hideLoading() {
			var o = byId('loading-overlay');
			if (o) o.hidden = true;
			spinnerActive = false;
		}
		function positionHoverWidget(clientX, clientY) {
			var hover = byId('hover-widget');
			if (!hover || hover.hidden) return;
			var margin = 14;
			var left = clientX + 14;
			var top = clientY + 16;
			var rect = hover.getBoundingClientRect();
			if (left + rect.width + margin > window.innerWidth) {
				left = Math.max(margin, clientX - rect.width - 14);
			}
			if (top + rect.height + margin > window.innerHeight) {
				top = Math.max(margin, window.innerHeight - rect.height - margin);
			}
			hover.style.left = left + 'px';
			hover.style.top = top + 'px';
		}
		function showHoverWidget(text, clientX, clientY) {
			var hover = byId('hover-widget');
			if (!hover || !text) return;
			hover.textContent = text;
			hover.hidden = false;
			positionHoverWidget(clientX, clientY);
		}
		function hideHoverWidget() {
			var hover = byId('hover-widget');
			if (hover) hover.hidden = true;
			hoverAnchor = null;
		}
		function findHoverAnchor(node) {
			while (node && node !== document.body) {
				if (node.classList && node.classList.contains('info-icon') && node.getAttribute('data-tooltip')) {
					return node;
				}
				node = node.parentNode;
			}
			return null;
		}
		function openPayloadModal(kind, title, payload, subtitle) {
			activeModalKind = kind || '';
			activeModalPayload = typeof payload === 'string' ? payload : String(payload || '');
			var backdrop = byId('payload-modal-backdrop');
			var titleEl = byId('payload-modal-title');
			var subtitleEl = byId('payload-modal-subtitle');
			var bodyEl = byId('payload-modal-body');
			if (titleEl) titleEl.textContent = title || 'Details';
			if (subtitleEl) subtitleEl.textContent = subtitle || '';
			if (bodyEl) bodyEl.textContent = activeModalPayload;
			if (backdrop) backdrop.hidden = false;
		}
		function closePayloadModal() {
			activeModalKind = '';
			var backdrop = byId('payload-modal-backdrop');
			if (backdrop) backdrop.hidden = true;
		}

		function send(cmd, opts, payload) {
			opts = opts || { showSpinner: true };
			var msgs = {
				'refresh': 'Refreshing data...',
				'healthCheck': 'Running health check...',
				'detectConflicts': 'Detecting conflicts...',
				'connectRedis': 'Opening Redis connect...',
				'initBrain': 'Initializing brain...',
				'exportBrain': 'Prompting export...',
				'importBrain': 'Prompting import...',
				'teamSync': 'Initiating team sync...',
				'prune': 'Pruning stale data...',
				'recoverBrain': 'Recovering brain...',
				'clearBrain': 'Clearing brain...'
			};
			if (opts.showSpinner && msgs[cmd]) showLoading(msgs[cmd]);
			var message = Object.assign({ command: cmd }, payload || {});
			vscode.postMessage(message);
		}
		function closeMenu() {
			var menu = document.getElementById('menu');
			if (!menu) return;
			menu.classList.remove('open');
			menu.setAttribute('aria-hidden', 'true');
		}
		function openMenu() {
			var menu = document.getElementById('menu');
			if (!menu) return;
			menu.classList.add('open');
			menu.setAttribute('aria-hidden', 'false');
		}
		function toggleMenu() {
			var m = document.getElementById('menu');
			if (!m) return;
			if (m.classList.contains('open')) closeMenu();
			else openMenu();
		}
		function setEmptyState(connected, isInitialized, reason, isPaused) {
			console.log('setEmptyState:', { connected, isInitialized, reason, isPaused }); // Add logging
			lastState.connected = !!connected;
			lastState.isInitialized = !!isInitialized;
			var empty = document.getElementById('empty');
			var main = document.getElementById('main');
			var title = document.getElementById('empty-title');
			var sub = document.getElementById('empty-sub');
			var btnAction = document.getElementById('btn-empty-action');
			var btnResume = document.getElementById('btn-empty-resume');
			if (!empty || !main || !title || !sub) {
				return;
			}
			
			if (!connected) {
				if (btnAction) btnAction.style.display = 'none';
				if (btnResume) btnResume.style.display = 'none';
				title.textContent = 'Connect Your Brain to Redis';
				sub.textContent = reason ? reason : 'To use Memix, connect your Redis for this workspace.';
				empty.classList.add('open');
				main.style.display = 'none';
				return;
			}
			
			if (!isInitialized) {
				if (btnAction) {
					btnAction.style.display = 'block';
					btnAction.textContent = 'Initialize Brain';
				}
				if (btnResume) btnResume.style.display = 'none';
				title.textContent = 'Initialize Your Brain';
				sub.textContent = 'Brain not initialized. Click "Initialize Brain" to start.';
				empty.classList.add('open');
				main.style.display = 'none';
				return;
			}
			
			if (isPaused) {
				title.textContent = 'Brain is Sleeping';
				sub.textContent = 'Memix daemon operations are paused to save resources. Search and generation still works.';
				empty.classList.add('open');
				main.style.display = 'none';
				if (btnAction) btnAction.style.display = 'none';
				if (btnResume) btnResume.style.display = 'block';
				return;
			}
			
			// Connected AND initialized AND unpaused - show main content
			empty.classList.remove('open');
			main.style.display = 'block';
			if (btnAction) btnAction.style.display = 'none';
			if (btnResume) btnResume.style.display = 'none';
		}

		function setDaemonBlockedState(state) {
			lastDaemonState = state || lastDaemonState;
			var empty = document.getElementById('empty');
			var main = document.getElementById('main');
			var title = document.getElementById('empty-title');
			var sub = document.getElementById('empty-sub');
			var btnAction = document.getElementById('btn-empty-action');
			var btnResume = document.getElementById('btn-empty-resume');
			if (!empty || !main || !title || !sub) {
				return;
			}
			title.textContent = state && state.title ? state.title : 'Preparing Memix Daemon';
			sub.textContent = state && state.description ? state.description : 'Memix is preparing the daemon required to enable the extension.';
			empty.classList.add('open');
			main.style.display = 'none';
			if (btnAction) btnAction.style.display = 'none';
			if (btnResume) btnResume.style.display = 'none';
		}
		function setInitMenuItem(isInitialized) {
			var item = document.getElementById('menu-init');
			if (!item) return;
			if (isInitialized) {
				item.setAttribute('aria-disabled', 'true');
				item.querySelector('span:last-child').textContent = 'Initialize Brain (Done)';
			} else {
				item.setAttribute('aria-disabled', 'false');
				item.querySelector('span:last-child').textContent = 'Initialize Brain';
			}
		}
		function activateTab(which) {
			var o = document.getElementById('tab-overview');
			var a = document.getElementById('tab-advanced');
			var s = document.getElementById('tab-settings');
			var vo = document.getElementById('view-overview');
			var va = document.getElementById('view-advanced');
			var vs = document.getElementById('view-settings');
			if (!o || !a || !s || !vo || !va || !vs) return;
			
			// Reset all
			o.classList.remove('active');
			a.classList.remove('active');
			s.classList.remove('active');
			vo.classList.remove('active');
			va.classList.remove('active');
			vs.classList.remove('active');

			if (which === 'advanced') {
				a.classList.add('active');
				va.classList.add('active');
				if (!advancedHydrated) {
					send('refresh', { showSpinner: true }, { includeAdvanced: true });
				}
			} else if (which === 'settings') {
				s.classList.add('active');
				vs.classList.add('active');
				send('refreshSettings', { showSpinner: true }); // Fetch latest daemon features state
			} else {
				o.classList.add('active');
				vo.classList.add('active');
			}
		}
		function bootUi() {
			// Tab click handling (event delegation so it survives any DOM changes)
			var tabs = document.querySelector('.tabs');
			if (tabs) {
				tabs.addEventListener('click', function(e) {
					var t = e.target;
					while (t && t !== tabs && !t.id) t = t.parentNode;
					if (!t || t === tabs) return;
					if (t.id === 'tab-overview') { e.preventDefault(); activateTab('overview'); }
					if (t.id === 'tab-advanced') { e.preventDefault(); activateTab('advanced'); }
					if (t.id === 'tab-settings') { e.preventDefault(); activateTab('settings'); }
				});
			}

			var menuInit = byId('menu-init');
			if (menuInit) {
				menuInit.addEventListener('click', function() {
					if (menuInit.getAttribute('aria-disabled') === 'true') return;
					closeMenu();
					send('initBrain', { showSpinner: true });
				});
			}

			var menuPause = byId('menu-pause');
			if (menuPause) {
				menuPause.addEventListener('click', function() {
					closeMenu();
					send('pauseBrain', { showSpinner: true });
				});
			}

			var menuSync = byId('menu-sync');
			if (menuSync) {
				menuSync.addEventListener('click', function() {
					closeMenu();
					send('teamSync', { showSpinner: true });
				});
			}

			var btnEmptyAction = byId('btn-empty-action');
			if (btnEmptyAction) {
				btnEmptyAction.addEventListener('click', function() {
					showLoading('Initializing brain...');
					vscode.postMessage({ command: 'initBrain' });
				});
			}

			var btnEmptyResume = byId('btn-empty-resume');
			if (btnEmptyResume) {
				btnEmptyResume.addEventListener('click', function() {
					send('resumeBrain', { showSpinner: true });
				});
			}

			var copyBtn = byId('copy-prompt-pack');
			if (copyBtn) {
				copyBtn.addEventListener('click', function() {
					vscode.postMessage({ command: 'copyPromptPack', text: lastPromptPack });
				});
			}

			var viewPromptPack = byId('view-prompt-pack');
			if (viewPromptPack) {
				viewPromptPack.addEventListener('click', function() {
					vscode.postMessage({
						command: 'openCenteredPayload',
						title: 'Prompt Pack',
						payload: lastPromptPack,
						subtitle: 'Copy-paste ready context bundle for AI chat.',
						notice: 'Memix Prompt Pack copied to clipboard'
					});
				});
			}

			var variantSel = byId('prompt-pack-variant');
			if (variantSel) {
				variantSel.addEventListener('change', function() {
					var v = variantSel.value;
					if (v === 'Small' || v === 'Standard' || v === 'Deep') {
						var pps = byId('prompt-pack-summary');
						if (pps) pps.textContent = 'Updating Prompt Pack...';
						var ppm = byId('prompt-pack-meta');
						if (ppm) ppm.textContent = 'tokens: recalculating...';
						send('setPromptPackVariant', { showSpinner: true }, { variant: v });
					}
				});
			}

			var fixMissingKeys = byId('fix-missing-keys');
			if (fixMissingKeys) {
				fixMissingKeys.addEventListener('click', function() {
					send('fixMissingKeys', { showSpinner: true });
				});
			}

			var otelOpen = byId('observer-dna-otel-open');
			if (otelOpen) {
				otelOpen.addEventListener('click', function() {
					vscode.postMessage({
						command: 'openCenteredPayload',
						title: 'Observer DNA OTel Export',
						payload: lastObserverDnaOtel,
						subtitle: 'OpenTelemetry-formatted observer export.',
						notice: 'Observer DNA OTel export copied to clipboard'
					});
				});
			}

			var otelCopy = byId('observer-dna-otel-copy');
			if (otelCopy) {
				otelCopy.addEventListener('click', function() {
					vscode.postMessage({ command: 'copyText', text: lastObserverDnaOtel, notice: 'Observer DNA OTel export copied to clipboard' });
				});
			}

			var hierarchyOpen = byId('hierarchy-resolution-open');
			if (hierarchyOpen) {
				hierarchyOpen.addEventListener('click', function() {
					vscode.postMessage({
						command: 'openCenteredPayload',
						title: 'Hierarchy Resolution',
						payload: lastHierarchyResolution,
						subtitle: 'Resolved merged brain value for the current hierarchy layers.',
						notice: 'Hierarchy resolution copied to clipboard'
					});
				});
			}

			var modalClose = byId('payload-modal-close');
			if (modalClose) {
				modalClose.addEventListener('click', closePayloadModal);
			}
			var modalDone = byId('payload-modal-done');
			if (modalDone) {
				modalDone.addEventListener('click', closePayloadModal);
			}
			var modalCopy = byId('payload-modal-copy');
			if (modalCopy) {
				modalCopy.addEventListener('click', function() {
					vscode.postMessage({ command: 'copyText', text: activeModalPayload, notice: 'Copied details to clipboard' });
				});
			}
			var modalBackdrop = byId('payload-modal-backdrop');
			if (modalBackdrop) {
				modalBackdrop.addEventListener('click', function(e) {
					if (e.target === modalBackdrop) closePayloadModal();
				});
			}
			document.addEventListener('keydown', function(e) {
				if (e.key === 'Escape') closePayloadModal();
			});

			var redisEdit = byId('redis-max-edit');
			if (redisEdit) {
				redisEdit.addEventListener('click', function(e) {
					e.preventDefault();
					send('editRedisMaxOverride', { showSpinner: true });
				});
			}

			// Settings Toggles
			var tBrainPause = byId('toggle-brain-pause');
			
			if (tBrainPause) {
				tBrainPause.addEventListener('change', function(e) {
					if (e.target.checked) {
						send('pauseBrain', { showSpinner: true });
					} else {
						send('resumeBrain', { showSpinner: true });
					}
				});
			}


			document.addEventListener('mouseover', function(e) {
				var anchor = findHoverAnchor(e.target);
				if (!anchor) return;
				hoverAnchor = anchor;
				showHoverWidget(anchor.getAttribute('data-tooltip') || '', e.clientX || 0, e.clientY || 0);
			});
			document.addEventListener('mousemove', function(e) {
				if (hoverAnchor) {
					positionHoverWidget(e.clientX || 0, e.clientY || 0);
				}
			});
			document.addEventListener('mouseout', function(e) {
				if (!hoverAnchor) return;
				var nextAnchor = findHoverAnchor(e.relatedTarget);
				if (nextAnchor === hoverAnchor) return;
				hideHoverWidget();
			});
			document.addEventListener('focusin', function(e) {
				var anchor = findHoverAnchor(e.target);
				if (!anchor) return;
				hoverAnchor = anchor;
				var rect = anchor.getBoundingClientRect();
				showHoverWidget(anchor.getAttribute('data-tooltip') || '', rect.left, rect.bottom);
			});
			document.addEventListener('focusout', function(e) {
				if (!findHoverAnchor(e.target)) return;
				hideHoverWidget();
			});

			// Start in empty mode until we have a definitive state
			setEmptyState(false, false, 'Checking daemon + Redis...');
			send('refresh', { showSpinner: false }, { silent: true });
			setInterval(function() { send('refresh', { showSpinner: false }, { silent: true }); }, 45000);
		}
		if (document.readyState === 'loading') {
			document.addEventListener('DOMContentLoaded', bootUi);
		} else {
			bootUi();
		}

		window.addEventListener('message', function(e) {
			if (spinnerActive) hideLoading();
			var command = e.data.command;
			var data = e.data.data;

			if (command === 'showLoading') {
				showLoading(e.data.text || data || 'Loading...');
				return;
			}

			if (command === 'hideLoading') {
				hideLoading();
				return;
			}

			if (command === 'settingsData') {
				var configPayload = data?.config || data || {};
				var configPath = data?.config_path || data?.configPath || '';
				var tBrainPause = byId('toggle-brain-pause');

				if (tBrainPause) tBrainPause.checked = !!configPayload.brain_paused;

				var configInfo = byId('settings-config-info');
				var configPathEl = byId('settings-config-path');
				if (configPathEl) {
					configPathEl.textContent = configPath ? configPath : '—';
				}
				if (spinnerActive) hideLoading();
				return;
			}

			if (command === 'state') {
				hasFirstState = true;
				if (lastDaemonState && lastDaemonState.kind !== 'ready') {
					setDaemonBlockedState(lastDaemonState);
					return;
				}
				setEmptyState(!!data.connected, !!data.isInitialized, data.reason, !!data.isPaused);
				return;
			}

			if (command === 'daemonState') {
				hasFirstState = true;
				lastDaemonState = data || lastDaemonState;
				if (data && data.kind === 'ready') {
					send('refresh', { showSpinner: false }, { silent: true });
					return;
				}
				setDaemonBlockedState(lastDaemonState);
				return;
			}

			if (command === 'update') {
				hasFirstState = true;
				lastDaemonState = { kind: 'ready', title: 'Memix Daemon Ready', description: 'The daemon is installed and ready.' };
				if (data.advancedDataLoaded) {
					advancedHydrated = true;
				}

				// Only show empty state if explicitly not initialized
				if (data.isInitialized === false) {
					setEmptyState(!!data.connected, false, data.reason || 'Missing required keys', !!data.isPaused);
				} else if (data.isInitialized === true && !data.isPaused) {
					// Force show main content
					var emptyEl = byId('empty');
					if (emptyEl) emptyEl.classList.remove('open');
					var mainEl = byId('main');
					if (mainEl) mainEl.style.display = 'block';
				}

				if (!data.isInitialized || data.isPaused) return;
				var healthEl = byId('health');
				if (healthEl) {
					healthEl.textContent = data.health.toUpperCase();
					healthEl.className = 'stat-value health-' + data.health;
				}

				var sizeKB = (data.totalSizeBytes / 1024).toFixed(1);
				var sizeEl = byId('size');
				if (sizeEl) sizeEl.textContent = sizeKB + ' KB';

				var usedMB = (data.redisUsedBytes / (1024 * 1024)).toFixed(1);
				var maxMB = (data.redisMaxBytes / (1024 * 1024)).toFixed(1);
				var pct = Math.min((data.redisUsedBytes / data.redisMaxBytes) * 100, 100);
				
				var suffix = data.redisMaxEstimated ? ' (est.)' : '';
				var rsText = byId('redis-size-text');
				if (rsText) rsText.textContent = pct.toFixed(1) + '% (' + usedMB + ' MB / ' + maxMB + ' MB)' + suffix;
				var rsBar = byId('redis-size-bar');
				if (rsBar) {
					rsBar.style.width = pct + '%';
					rsBar.style.background = pct > 90 ? '#f44747' : pct > 75 ? '#cca700' : '#4ec9b0';
				}

				var keyCountEl = byId('keyCount');
				if (keyCountEl) keyCountEl.textContent = data.keyCount;
				var sessionEl = byId('session');
				if (sessionEl) sessionEl.textContent = '#' + data.sessionNumber;
				function timeAgo(dateString) {
					var date = new Date(dateString);
					var seconds = Math.floor((new Date() - date) / 1000);
					var interval = seconds / 31536000;
					if (interval > 1) return Math.floor(interval) + " years ago";
					interval = seconds / 2592000;
					if (interval > 1) return Math.floor(interval) + " months ago";
					interval = seconds / 86400;
					if (interval > 1) return Math.floor(interval) + " days ago";
					interval = seconds / 3600;
					if (interval > 1) return Math.floor(interval) + " hours ago";
					interval = seconds / 60;
					if (interval > 1) return Math.floor(interval) + " minutes ago";
					return Math.floor(seconds) + " seconds ago";
				}
				
				var lastUpdatedEl = byId('lastUpdated');
				if (lastUpdatedEl) lastUpdatedEl.textContent = data.lastUpdated === 'Never' ? 'Never' : timeAgo(data.lastUpdated);
				var currentTaskEl = byId('currentTask');
				if (currentTaskEl) currentTaskEl.textContent = data.currentTask;
				setInitMenuItem(!!data.isInitialized);

				var missing = data.missingRequiredKeys || [];
				var requiredEl = byId('required-keys-status');
				var missingEl = byId('missing-required-keys');
				var fixBtn = byId('fix-missing-keys');
				if (requiredEl && missingEl) {
				if (missing.length === 0) {
					requiredEl.textContent = 'OK';
					requiredEl.style.color = '#4ec9b0';
					missingEl.innerHTML = '<span style="color:#4ec9b0">All required keys present</span>';
					if (fixBtn) fixBtn.hidden = true;
				} else {
					requiredEl.textContent = 'Missing ' + missing.length;
					requiredEl.style.color = '#f44747';
					missingEl.innerHTML = '<div style="color:#f44747">Missing required keys: ' + missing.map(escapeHtml).join(', ') + '</div>';
					if (fixBtn) fixBtn.hidden = false;
				}
				}

				var stalenessEl = byId('staleness');
				if (stalenessEl) {
				var sh = data.stalenessHours;
				if (typeof sh === 'number') {
					if (sh < 1) {
						stalenessEl.textContent = 'Fresh';
					} else if (sh < 24) {
						stalenessEl.textContent = Math.round(sh) + 'h';
					} else {
						stalenessEl.textContent = Math.round(sh) + 'h';
					}
				} else {
					stalenessEl.textContent = 'Unknown';
					stalenessEl.style.color = 'var(--vscode-descriptionForeground)';
				}
				}

				var catHtml = '';
				var cats = data.categories;
				for (var name in cats) {
					var info = cats[name];
					var kb = (info.size / 1024).toFixed(1);
					catHtml += '<div class="category"><div class="category-name">' + name + ' <span>' + kb + ' KB</span></div><div style="margin-top:3px;color:var(--vscode-descriptionForeground);font-size:11px;">' + info.keys.join(', ') + '</div></div>';
				}
				var categoriesEl = byId('categories');
				if (categoriesEl) categoriesEl.innerHTML = catHtml || '<span style="color:var(--vscode-descriptionForeground)">No data</span>';

				var keySizesHtml = '';
				var keys = data.keys || {};
				var sorted = Object.keys(keys).sort(function(a, b) { return (keys[b] || 0) - (keys[a] || 0); }).slice(0, 10);
				for (var i = 0; i < sorted.length; i++) {
					var k = sorted[i];
					var bytes = keys[k] || 0;
					keySizesHtml += '<div class="k">' + k + '</div><div>' + (bytes / 1024).toFixed(1) + ' KB</div>';
				}
				var keySizesEl = byId('key-sizes');
				if (keySizesEl) keySizesEl.innerHTML = keySizesHtml || '<span style="color:var(--vscode-descriptionForeground)">No data</span>';

				if (data.metrics) {
					var md = byId('metric-decisions');
					if (md) md.textContent = data.metrics.decisions;
					var mf = byId('metric-facts');
					if (mf) mf.textContent = data.metrics.facts;
					var mp = byId('metric-patterns');
					if (mp) mp.textContent = data.metrics.patterns;
					var mw = byId('metric-warnings');
					if (mw) mw.textContent = data.metrics.warnings;
				}

				var ideEl = byId('ide');
				if (ideEl) ideEl.textContent = (data.ide || '').toUpperCase();
				var rulesPathEl = byId('rules-path');
				if (rulesPathEl) rulesPathEl.textContent = (data.rulesDir || '') + '/' + (data.rulesFile || '');

				if (Array.isArray(data.keyCoverage)) {
					var rows = data.keyCoverage.map(function(r) {
						var status = '<div class="status-subtle">Optional</div>';
						if (r.exists) {
							status = '<div class="status-ok">Ready</div>';
						} else if (r.state === 'missing_required') {
							status = '<div class="status-danger">Needs initialization</div>';
						} else if (r.state === 'missing_recommended') {
							status = '<div class="status-warning">Recommended</div>';
						} else if (r.state === 'not_generated') {
							status = '<div class="status-subtle">Generated later</div>';
						}
						var label = escapeHtml(r.label || r.key);
						var tooltip = r.description ? ' data-tooltip="' + escapeHtml(r.description) + '" aria-label="' + escapeHtml(r.description) + '" tabindex="0"' : '';
						var icon = r.description ? '<span class="info-icon"' + tooltip + '><svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M13.25 7c0 .69-.56 1.25-1.25 1.25s-1.25-.56-1.25-1.25.56-1.25 1.25-1.25 1.25.56 1.25 1.25zm10.75 5c0 6.627-5.373 12-12 12s-12-5.373-12-12 5.373-12 12-12 12 5.373 12 12zm-2 0c0-5.514-4.486-10-10-10s-10 4.486-10 10 4.486 10 10 10 10-4.486 10-10zm-13-2v2h2v6h2v-8h-4z"/></svg></span>' : '';
						return '<div class="k"><span class="key-label">' + label + icon + '</span></div><div style="text-align:right;">' + status + '</div>';
					}).join('');
					var kc = byId('key-coverage');
					if (kc) kc.innerHTML = rows || '<div class="k">—</div><div>—</div>';
				}

				if (typeof data.promptPack === 'string') {
					lastPromptPack = data.promptPack;
					var pps = byId('prompt-pack-summary');
					if (pps) {
						var missingSections = Array.isArray(data.promptPackMissingSections) ? data.promptPackMissingSections : [];
						var observerSectionCount = typeof data.promptPackObserverSectionCount === 'number' ? data.promptPackObserverSectionCount : 0;
						var missingLine = missingSections.length > 0
							? '<div style="margin-top:4px">Missing right now: ' + missingSections.map(escapeHtml).join(', ') + '</div>'
							: '';
						pps.innerHTML = '<strong>' + (data.promptPackRequestedSectionCount || 0) + '</strong> sections prepared for the <strong>' + escapeHtml(data.promptPackVariant || 'Standard') + '</strong> variant.' +
							'<div style="margin-top:4px"><strong>' + (data.promptPackAvailableSectionCount || 0) + '</strong> available from the current brain.</div>' +
							'<div style="margin-top:4px"><strong>' + observerSectionCount + '</strong> observer intelligence sections included.</div>' +
							missingLine;
					}
					var tok = (typeof data.promptPackTokens === 'number') ? (data.promptPackTokens + ' tokens') : 'tokens: —';
					var ppm = byId('prompt-pack-meta');
					if (ppm) ppm.textContent = tok;
				}

				if (!data.advancedDataLoaded) {
					return;
				}

				if (typeof data.promptPackVariant === 'string') {
					var sel = byId('prompt-pack-variant');
					if (sel && (data.promptPackVariant === 'Small' || data.promptPackVariant === 'Standard' || data.promptPackVariant === 'Deep')) {
						sel.value = data.promptPackVariant;
					}
				}

				var slCount = byId('session-log-count');
				if (slCount) slCount.textContent = data.sessionLogCount || 0;
				var slPrev = byId('session-log-preview');
				if (!slPrev) {
					return;
				}
				var prev = data.sessionLogPreview || [];
				if (prev.length === 0) {
					slPrev.textContent = 'No entries';
				} else {
					var pHtml = '';
					for (var pi = 0; pi < prev.length; pi++) {
						var dt = prev[pi].date || '';
						var sm = prev[pi].summary || '';
						pHtml += '<div style="margin-bottom:6px"><div style="font-size:10px;opacity:0.8">' + dt + '</div><div style="font-size:11px;line-height:1.3">' + sm + '</div></div>';
					}
					slPrev.innerHTML = pHtml;
				}

				var stCount = byId('session-timeline-count');
				if (stCount) stCount.textContent = data.sessionTimelineCount || 0;
				var stPrev = byId('session-timeline-preview');
				if (stPrev) {
					var timeline = data.sessionTimelinePreview || [];
					if (timeline.length === 0) {
						stPrev.textContent = 'No events';
					} else {
						var tHtml = '';
						for (var ti = 0; ti < timeline.length; ti++) {
							var tts = timeline[ti].timestamp || '';
							var tev = timeline[ti].event || '';
							tHtml += '<div style="margin-bottom:6px"><div style="font-size:10px;opacity:0.8">' + tts + '</div><div style="font-size:11px;line-height:1.3">' + tev + '</div></div>';
						}
						stPrev.innerHTML = tHtml;
					}
				}

				var dna = data.observerDna || null;
				var dnaArch = byId('observer-dna-architecture');
				if (dnaArch) dnaArch.textContent = dna && dna.architecture ? dna.architecture : '—';
				var dnaComplexity = byId('observer-dna-complexity');
				if (dnaComplexity) {
					dnaComplexity.textContent = dna && typeof dna.complexity_score === 'number'
						? Math.round(dna.complexity_score * 100) + '%'
						: '—';
				}
				var dnaSummary = byId('observer-dna-summary');
				if (dnaSummary) {
					if (dna && typeof dna.indexed_files === 'number') {
						var complexityPct = typeof dna.complexity_score === 'number' ? Math.round(dna.complexity_score * 100) : 0;
						var typePct = typeof dna.type_coverage === 'number' ? Math.round(dna.type_coverage * 100) : 0;
						var summaryLine = dna.indexed_files + ' files • ' + (dna.functions_indexed || 0) + ' symbols • depth ' + (dna.dependency_depth || 0) + ' • complexity ' + complexityPct + '% • typed ' + typePct + '%';
						var explanation = typeof dna.explainability_summary === 'string' ? dna.explainability_summary : '';
						dnaSummary.innerHTML = '<div>' + summaryLine + '</div>' + (explanation ? '<div style="margin-top:6px;font-size:11px;line-height:1.4"><b>Explainability:</b> ' + explanation + '</div>' : '');
					} else {
						dnaSummary.textContent = 'No DNA snapshot';
					}
				}
				var dnaPatterns = byId('observer-dna-patterns');
				if (dnaPatterns) {
					var patterns = dna && Array.isArray(dna.dominant_patterns) ? dna.dominant_patterns : [];
					var languageBreakdown = dna && dna.language_breakdown ? dna.language_breakdown : null;
					var languages = [];
					if (languageBreakdown) {
						for (var language in languageBreakdown) {
							if (Object.prototype.hasOwnProperty.call(languageBreakdown, language)) {
								languages.push(language + ': ' + languageBreakdown[language]);
							}
						}
					}
					var ruleSource = dna && dna.rules_source ? dna.rules_source : '';
					var appliedRules = dna && Array.isArray(dna.applied_rule_ids) ? dna.applied_rule_ids : [];
					var parts = [];
					if (patterns.length > 0) {
						parts.push('<div style="font-size:11px;line-height:1.4"><b>Patterns:</b> ' + patterns.join(', ') + '</div>');
					}
					if (languages.length > 0) {
						parts.push('<div style="font-size:11px;line-height:1.4;margin-top:4px"><b>Languages:</b> ' + languages.join(', ') + '</div>');
					}
					if (ruleSource || appliedRules.length > 0) {
						parts.push('<div style="font-size:11px;line-height:1.4;margin-top:4px"><b>Rules:</b> ' + (ruleSource || 'built-in defaults') + (appliedRules.length > 0 ? ' • applied ' + appliedRules.join(', ') : '') + '</div>');
					}
					dnaPatterns.innerHTML = parts.length > 0
						? parts.join('')
						: '<div style="color:var(--vscode-descriptionForeground)">Patterns unavailable</div>';
				}
				var dnaHot = byId('observer-dna-hot-zones');
				if (dnaHot) {
					var hotZones = dna && Array.isArray(dna.hot_zones) ? dna.hot_zones : [];
					var circular = dna && Array.isArray(dna.circular_risks) ? dna.circular_risks : [];
					var hotHtml = hotZones.length > 0
						? '<div style="font-size:11px;line-height:1.4"><b>Hot zones:</b><br/>' + hotZones.slice(0, 4).join('<br/>') + '</div>'
						: '<div style="color:var(--vscode-descriptionForeground)">No hot zones detected</div>';
					if (circular.length > 0) {
						hotHtml += '<div style="font-size:11px;line-height:1.4;margin-top:6px"><b>Circular risks:</b><br/>' + circular.slice(0, 3).join('<br/>') + '</div>';
					}
					dnaHot.innerHTML = hotHtml;
				}
				var dnaStable = byId('observer-dna-stable-zones');
				if (dnaStable) {
					var stableZones = dna && Array.isArray(dna.stable_zones) ? dna.stable_zones : [];
					dnaStable.innerHTML = stableZones.length > 0
						? '<div style="font-size:11px;line-height:1.4"><b>Stable zones:</b><br/>' + stableZones.slice(0, 4).join('<br/>') + '</div>'
						: '<div style="color:var(--vscode-descriptionForeground)">No stable zones detected</div>';
				}
				var dnaOtel = data.observerDnaOtel || null;
				var dnaOtelEl = byId('observer-dna-otel-summary');
				if (dnaOtelEl) {
					if (dnaOtel && typeof dnaOtel.schema_url === 'string') {
						lastObserverDnaOtel = JSON.stringify(dnaOtel, null, 2);
						dnaOtelEl.innerHTML = '<strong>🧬 OpenTelemetry export ready.</strong> Schema: ' + escapeHtml(dnaOtel.schema_url);
					} else {
						lastObserverDnaOtel = '';
						dnaOtelEl.textContent = 'No OTel export';
					}
				}

				var intent = data.observerIntent || null;
				var intentType = byId('observer-intent-type');
				if (intentType) {
					if (intent && intent.intent_type) {
						var confidencePct2 = typeof intent.confidence === 'number' ? Math.round(intent.confidence * 100) : 0;
						intentType.textContent = intent.intent_type + ' (' + confidencePct2 + '%)';
					} else {
						intentType.textContent = '—';
					}
				}
				var intentFile = byId('observer-intent-active-file');
				if (intentFile) intentFile.textContent = intent && intent.active_file ? intent.active_file : '—';
				var intentRelated = byId('observer-intent-related-files');
				if (intentRelated) {
					var related = intent && Array.isArray(intent.related_files) ? intent.related_files : [];
					var tokenWeight = intent && typeof intent.token_weight === 'number' ? intent.token_weight : 0;
					intentRelated.innerHTML = related.length > 0
						? '<b>Related files</b> (' + tokenWeight + ' tokens)<br/>' + related.slice(0, 4).join('<br/>')
						: 'No predictive snapshot';
				}
				var intentRationale = byId('observer-intent-rationale');
				if (intentRationale) {
					var rationale = intent && Array.isArray(intent.rationale) ? intent.rationale : [];
					intentRationale.innerHTML = rationale.length > 0
						? '<div style="font-size:11px;line-height:1.4"><b>Rationale:</b><br/>' + rationale.join('<br/>') + '</div>'
						: '<div style="color:var(--vscode-descriptionForeground)">Rationale unavailable</div>';
				}

				var git = data.observerGit || null;
				var gitRepo = byId('observer-git-repo');
				if (gitRepo) gitRepo.textContent = git && git.repo_root ? git.repo_root : '—';
				var gitAuthors = byId('observer-git-authors');
				if (gitAuthors) {
					var authors = git && Array.isArray(git.recent_authors) ? git.recent_authors : [];
					gitAuthors.innerHTML = authors.length > 0
						? '<b>Recent authors:</b> ' + authors.join(', ')
						: 'No archaeology snapshot';
				}
				var gitHot = byId('observer-git-hot-files');
				if (gitHot) {
					var hotFiles = git && Array.isArray(git.hot_files) ? git.hot_files : [];
					if (hotFiles.length === 0) {
						gitHot.innerHTML = '<div style="color:var(--vscode-descriptionForeground)">No hot files</div>';
					} else {
						var gh = '';
						for (var gi = 0; gi < Math.min(hotFiles.length, 4); gi++) {
							var hf = hotFiles[gi] || {};
							var touch = hf.last_touch && hf.last_touch.summary ? ' • ' + hf.last_touch.summary : '';
							gh += '<div style="margin-bottom:6px"><div style="font-size:11px;line-height:1.3">' + (hf.file_path || '—') + ' <b>(' + (hf.churn_commits || 0) + ')</b></div><div style="font-size:10px;opacity:0.8">' + touch.replace(/^ • /, '') + '</div></div>';
						}
						gitHot.innerHTML = gh;
					}
				}

				var agentCfg = data.agentConfig || null;
				var agentCfgEl = byId('agent-config-summary');
				if (agentCfgEl) {
					var cfgs = agentCfg && Array.isArray(agentCfg.configs) ? agentCfg.configs : [];
					if (cfgs.length === 0) {
						agentCfgEl.textContent = 'No agent runtime data';
					} else {
						agentCfgEl.innerHTML = '<div><b>' + cfgs.length + '</b> agents from ' + (agentCfg.source_path || 'runtime defaults') + '</div>' +
							'<div style="margin-top:4px;font-size:11px;line-height:1.4">'+ cfgs.slice(0, 5).map(function(cfg) {
								return cfg.name + ' • ' + cfg.scope + ' • cooldown ' + cfg.cooldown_ms + 'ms';
							}).join('<br/>') + '</div>';
					}
				}
				var agentReportsEl = byId('agent-reports-summary');
				if (agentReportsEl) {
					var reports = data.agentReports && Array.isArray(data.agentReports.reports) ? data.agentReports.reports : [];
					agentReportsEl.innerHTML = reports.length > 0
						? '<div style="font-size:11px;line-height:1.4"><b>Recent reports:</b><br/>' + reports.slice(-4).reverse().map(function(report) {
							return report.agent_name + ' [' + report.severity + ']';
						}).join('<br/>') + '</div>'
						: '<div style="color:var(--vscode-descriptionForeground)">No recent agent reports</div>';
				}

				var compiled = data.compiledContext || null;
				var compiledSummary = byId('compiled-context-summary');
				if (compiledSummary) {
					if (compiled && typeof compiled.total_tokens === 'number') {
						compiledSummary.innerHTML = '<div><b>' + (data.activeFile || 'Active file unavailable') + '</b></div>' +
							'<div style="margin-top:4px">task=' + (data.inferredTaskType || 'unknown') + ' • ' + compiled.total_tokens + '/' + compiled.budget + ' tokens</div>' +
							'<div style="margin-top:4px;font-size:11px;line-height:1.4">' + (compiled.explainability_summary || '') + '</div>';
					} else {
						compiledSummary.textContent = 'No compiled context';
					}
				}
				var compiledSections = byId('compiled-context-sections');
				if (compiledSections) {
					var sections = compiled && Array.isArray(compiled.selected_sections) ? compiled.selected_sections : [];
					compiledSections.innerHTML = sections.length > 0
						? '<div style="font-size:11px;line-height:1.4"><b>Selected sections:</b><br/>' + sections.slice(0, 5).map(function(section) {
							return section.kind + ' • ' + section.tokens + ' tokens';
						}).join('<br/>') + '</div>'
						: '<div style="color:var(--vscode-descriptionForeground)">No selected sections</div>';
				}

				var risk = data.proactiveRisk || null;
				var riskSummary = byId('proactive-risk-summary');
				if (riskSummary) {
					if (risk && typeof risk.risk_score === 'number') {
						var riskPct = Math.round(risk.risk_score * 100);
						riskSummary.innerHTML = '<div><b>' + risk.file + '</b></div><div style="margin-top:4px">risk ' + riskPct + '% • dependents ' + (risk.dependents || 0) + '</div><div style="margin-top:4px">' + (risk.recommendation || '') + '</div>';
					} else {
						riskSummary.textContent = 'No risk signal';
					}
				}
				var riskDetails = byId('proactive-risk-details');
				if (riskDetails) {
					var knownIssues = risk && Array.isArray(risk.known_issues) ? risk.known_issues : [];
					var pastBreaks = risk && Array.isArray(risk.past_breaks) ? risk.past_breaks : [];
					var riskParts = [];
					if (knownIssues.length > 0) {
						riskParts.push('<b>Known issues:</b><br/>' + knownIssues.slice(0, 3).join('<br/>'));
					}
					if (pastBreaks.length > 0) {
						riskParts.push('<b>Past breaks:</b><br/>' + pastBreaks.slice(0, 3).join('<br/>'));
					}
					riskDetails.innerHTML = riskParts.length > 0
						? '<div style="font-size:11px;line-height:1.4">' + riskParts.join('<br/><br/>') + '</div>'
						: '<div style="color:var(--vscode-descriptionForeground)">No detailed risk history</div>';
				}

				var promptOpt = data.promptOptimization || null;
				var promptOptEl = byId('prompt-optimization-summary');
				if (promptOptEl) {
					if (promptOpt && typeof promptOpt.recommended_budget === 'number') {
						promptOptEl.innerHTML = '<div><b>Task:</b> ' + (promptOpt.task_type || data.inferredTaskType || 'unknown') + '</div>' +
							'<div style="margin-top:4px">recommended budget ' + promptOpt.recommended_budget + ' tokens</div>' +
							'<div style="margin-top:4px;font-size:11px;line-height:1.4"><b>Always include:</b> ' + ((promptOpt.always_include || []).join(', ') || '—') + '</div>';
					} else {
						promptOptEl.textContent = 'No learning data';
					}
				}
				var modelPerfEl = byId('model-performance-summary');
				if (modelPerfEl) {
					var perf = data.modelPerformance && data.modelPerformance.model_performance ? data.modelPerformance.model_performance : {};
					var perfLines = [];
					for (var model in perf) {
						if (!Object.prototype.hasOwnProperty.call(perf, model)) continue;
						var tasksPerf = perf[model] || {};
						for (var taskName in tasksPerf) {
							if (!Object.prototype.hasOwnProperty.call(tasksPerf, taskName)) continue;
							var tp = tasksPerf[taskName] || {};
							perfLines.push(model + ' • ' + taskName + ' • ' + Math.round((tp.first_try_rate || 0) * 100) + '% first-try • ' + (tp.runs || 0) + ' runs');
						}
					}
					modelPerfEl.innerHTML = perfLines.length > 0
						? '<div style="font-size:11px;line-height:1.4"><b>Model performance:</b><br/>' + perfLines.slice(0, 4).join('<br/>') + '</div>'
						: '<div style="color:var(--vscode-descriptionForeground)">No model performance data</div>';
				}
				var profileEl = byId('developer-profile-summary');
				if (profileEl) {
					var profile = data.developerProfile || null;
					if (profile) {
						profileEl.innerHTML = '<div style="font-size:11px;line-height:1.4"><b>Preferred stack:</b> ' + ((profile.preferred_stack || []).join(', ') || '—') + '</div>' +
							'<div style="margin-top:4px;font-size:11px;line-height:1.4"><b>Code style:</b> ' + ((profile.code_style || []).join(', ') || '—') + '</div>';
					} else {
						profileEl.textContent = 'No developer profile';
					}
				}

				var hierarchyEl = byId('hierarchy-resolution-summary');
				if (hierarchyEl) {
					var hierarchy = data.hierarchyIdentity || null;
					if (hierarchy && Array.isArray(hierarchy.resolved_from)) {
						lastHierarchyResolution = JSON.stringify(hierarchy.value || {}, null, 2);
						hierarchyEl.innerHTML = '<div><b>' + escapeHtml(hierarchy.entry_id || 'identity.json') + '</b> <span style="margin-top:4px">(resolved from: ' + hierarchy.resolved_from.map(escapeHtml).join(' → ') + ')</span></div>';
					} else {
						lastHierarchyResolution = '';
						hierarchyEl.textContent = 'No hierarchy resolution';
					}
				}

				var tasks = data.pendingTasks || [];
				var tBadge = byId('advanced-badge');
				var pendingCountEl = byId('pending-tasks-count');
				if (pendingCountEl) pendingCountEl.textContent = tasks.length;
				if (tasks.length > 0) {
					if (tBadge) {
						tBadge.style.display = 'inline-block';
						tBadge.textContent = tasks.length;
					}
					var tHtml = '';
					for (var j = 0; j < Math.min(tasks.length, 5); j++) {
						var title = typeof tasks[j] === 'string' ? tasks[j] : (tasks[j].title || tasks[j].task || 'Unknown task');
						tHtml += '<div class="task-item"><div class="task-bullet">●</div><div>' + title + '</div></div>';
					}
					if (tasks.length > 5) tHtml += '<div style="margin-top:4px;color:var(--vscode-descriptionForeground);font-size:10px">+' + (tasks.length - 5) + ' more tasks...</div>';
					var pendingContainerEl = byId('pending-tasks-container');
					if (pendingContainerEl) pendingContainerEl.innerHTML = tHtml;
				} else {
					if (tBadge) tBadge.style.display = 'none';
					var pendingContainerEl2 = byId('pending-tasks-container');
					if (pendingContainerEl2) pendingContainerEl2.innerHTML = '<span style="color:var(--vscode-descriptionForeground)">No pending tasks</span>';
				}

				if (data.recommendations.length > 0) {
					var warnHtml = '';
					for (var i = 0; i < data.recommendations.length; i++) {
						warnHtml += '<div class="warning">\u26A0\uFE0F ' + data.recommendations[i] + '</div>';
					}
					var warningsEl = byId('warnings');
					if (warningsEl) warningsEl.innerHTML = warnHtml;
				} else {
					var warningsEl2 = byId('warnings');
					if (warningsEl2) warningsEl2.innerHTML = '<span style="color:#4ec9b0">All clear</span>';
				}

				var errorBanner = byId('error-banner');
				if (errorBanner) errorBanner.style.display = 'none';
			}

			if (command === 'error') {
				var eb = byId('error-banner');
				if (eb) {
					eb.textContent = String(data || 'Unknown error');
					eb.style.display = 'block';
				}
			}

			if (command === 'state') {
				setEmptyState(!!data.connected, !!data.isInitialized, data.reason, !!data.isPaused);
				var eb2 = byId('error-banner');
				if (eb2) {
					eb2.textContent = String(data.reason || 'Not connected');
					eb2.style.display = data.connected ? 'none' : 'block';
				}
			}

			if (command === 'healthReport') {
				var w = byId('warnings');
				if (!w) return;
				if (data.recommendations.length > 0) {
					var html = '';
					for (var i = 0; i < data.recommendations.length; i++) {
						html += '<div class="warning">\u26A0\uFE0F ' + data.recommendations[i] + '</div>';
					}
					w.innerHTML = html;
				} else {
					w.innerHTML = '<span style="color:#4ec9b0">All checks passed</span>';
				}
			}

			if (command === 'conflicts') {
				var w2 = byId('warnings');
				if (!w2) return;
				if (data.length > 0) {
					var cHtml = '';
					for (var i = 0; i < data.length; i++) {
						cHtml += '<div class="warning">\u26A1 ' + data[i].recommendation + '</div>';
					}
					w2.innerHTML = cHtml;
				} else {
					w2.innerHTML = '<span style="color:#4ec9b0">No conflicts</span>';
				}
			}
		});
	</script>
</body>
</html>`;
	}
}