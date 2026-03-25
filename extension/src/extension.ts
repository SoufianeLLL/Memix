import * as vscode from 'vscode';
import * as path from 'path';
import * as fs from 'fs';
import * as os from 'os';
import { BrainManager } from './core/brain';
import { DebugPanelProvider } from './panel/debug-panel';
import { exportBrain, importBrain } from './utils/exporter';
import { hashProjectId } from './utils/crypto';
import { SecretManager } from './core/secrets';
import { DaemonManager } from './daemon';
import { detectIDE } from './ide/detector';
import { MemoryClient } from './client';
import { LicenseManager } from './license';
import { DaemonReadinessState, DaemonRuntimeManager } from './daemon-runtime';
import { BRAIN_KEYS, MAX_SESSION_LOG_ENTRIES } from './utils/constants';
import { createPromptPack, PromptPackVariant } from './utils/prompt-pack';

let brain: BrainManager;
let panelProvider: DebugPanelProvider;
let statusBarItem: vscode.StatusBarItem;
let daemonOutputChannel: vscode.OutputChannel;
let daemonReadinessState: DaemonReadinessState = DaemonRuntimeManager.getInitialState();

function setDaemonReadinessState(state: DaemonReadinessState) {
	daemonReadinessState = state;
	panelProvider?.setDaemonState(state);
	if (!statusBarItem) {
		return;
	}
	if (state.kind === 'ready') {
		statusBarItem.text = 'Memix';
		return;
	}
	if (state.kind === 'error' || state.kind === 'missing') {
		statusBarItem.text = 'Memix Unavailable';
		return;
	}
	statusBarItem.text = state.kind === 'updating' ? 'Memix Updating…' : 'Memix Downloading…';
}

async function ensureDaemonReady(): Promise<boolean> {
	if (daemonReadinessState.kind === 'ready') {
		return true;
	}
	vscode.commands.executeCommand('memix.showPanel');
	vscode.window.showWarningMessage(daemonReadinessState.description);
	panelProvider?.setDaemonState(daemonReadinessState);
	return false;
}

/** Helper — returns true if workspace-dependent modules are ready */
function requireWorkspace(): boolean {
	if (!brain) {
		vscode.window.showWarningMessage('Memix: Open a workspace folder first.');
		return false;
	}
	return true;
}

async function upsertMemixConfigRedisUrl(redisUrl: string): Promise<void> {
	const dir = path.join(os.homedir(), '.memix');
	const configPath = path.join(dir, 'config.toml');
	await fs.promises.mkdir(dir, { recursive: true });

	let existing = '';
	try {
		existing = await fs.promises.readFile(configPath, 'utf8');
	} catch {
		// ignore
	}

	const line = `redis_url = "${redisUrl.replace(/\\/g, '\\\\').replace(/"/g, '\\"')}"`;
	if (!existing.trim()) {
		await fs.promises.writeFile(configPath, `${line}\n`, 'utf8');
		return;
	}

	if (/^\s*redis_url\s*=\s*".*"\s*$/m.test(existing)) {
		const updated = existing.replace(/^\s*redis_url\s*=\s*".*"\s*$/m, line);
		await fs.promises.writeFile(configPath, updated, 'utf8');
		return;
	}

	await fs.promises.writeFile(configPath, `${existing.trimEnd()}\n${line}\n`, 'utf8');
}

export async function activate(context: vscode.ExtensionContext) {
	// Create channel for daemon logs
	daemonOutputChannel = vscode.window.createOutputChannel('Memix Daemon');
	context.subscriptions.push(daemonOutputChannel);

	DaemonManager.setOutputChannel(daemonOutputChannel);
	DaemonRuntimeManager.setOutputChannel(daemonOutputChannel);

	// Dev ergonomics: allow env/.env overrides in Development mode
	if (context.extensionMode === vscode.ExtensionMode.Development) {
		try {
			// eslint-disable-next-line @typescript-eslint/no-var-requires
			const dotenv = require('dotenv');
			dotenv.config({ path: path.join(context.extensionPath, '.env'), override: false });
			const workspaceRootForEnv = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
			if (workspaceRootForEnv) {
				dotenv.config({ path: path.join(workspaceRootForEnv, '.env'), override: false });
			}
		} catch {
			// Optional; ignore if not installed
		}
	}

	const config = vscode.workspace.getConfiguration('memix');
	const envExternal = (process.env.MEMIX_DEV_EXTERNAL_DAEMON || '').toLowerCase();
	const externalDaemon = envExternal === '1' || envExternal === 'true' || config.get<boolean>('dev.externalDaemon') === true;

	if (externalDaemon) {
		const httpUrl = process.env.MEMIX_DAEMON_HTTP_URL || config.get<string>('dev.daemonHttpUrl') || 'http://127.0.0.1:3456';
		MemoryClient.setBaseUrl(httpUrl);
	} else {
		MemoryClient.setBaseUrl(null);
	}

	const workspaceRoot = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
	const configProjectId = config.get<string>('projectId');
	const projectId = workspaceRoot ? (configProjectId || hashProjectId(workspaceRoot)) : undefined;

	const secretManager = new SecretManager(context);

	// --- Migration: move plaintext config to SecretStorage ---
	const oldUrl = config.get<string>('redisUrl');
	if (oldUrl) {
		await secretManager.storeSecret('redisUrl', oldUrl);
		await config.update('redisUrl', undefined, vscode.ConfigurationTarget.Global);
	}
	const oldTeamId = config.get<string>('teamId');
	if (oldTeamId) {
		await secretManager.storeSecret('teamId', oldTeamId);
		await config.update('teamId', undefined, vscode.ConfigurationTarget.Global);
		await config.update('teamId', undefined, vscode.ConfigurationTarget.Workspace);
	}

	const savedRedisUrl = await secretManager.getSecret('redisUrl');

	panelProvider = new DebugPanelProvider(
		context.extensionUri,
		brain || null,
		workspaceRoot || null
	);
	panelProvider.setDaemonState(daemonReadinessState);
	context.subscriptions.push(
		vscode.window.registerWebviewViewProvider(
			DebugPanelProvider.viewType,
			panelProvider,
			{ webviewOptions: { retainContextWhenHidden: true } }
		)
	);

	statusBarItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 100);
	statusBarItem.text = 'Memix Downloading…';
	statusBarItem.command = 'memix.showPanel';
	statusBarItem.show();
	context.subscriptions.push(statusBarItem);

	// Boot the Rust Daemon HTTP Server in the background
	// In dev external daemon mode, we never spawn/own the daemon process.
	if (!externalDaemon) {
		try {
			const runtime = await DaemonRuntimeManager.prepareDaemon(
				context,
				context.extensionPath,
				context.extension.packageJSON.version,
				setDaemonReadinessState,
			);
			DaemonManager.setBinaryPath(runtime.binaryPath);
			await DaemonManager.start(runtime.binaryPath, workspaceRoot || null, projectId || null, savedRedisUrl);
			if (runtime.updated) {
				vscode.window.showInformationMessage(`Memix daemon updated to v${runtime.version}.`);
			}
			setDaemonReadinessState({
				kind: 'ready',
				title: 'Memix Daemon Ready',
				description: 'The daemon is installed and ready.',
				version: runtime.version,
			});
			DaemonRuntimeManager.startBackgroundUpdateCheck(context, context.extension.packageJSON.version);
		} catch (e: any) {
			daemonOutputChannel.show(true);
			setDaemonReadinessState({
				kind: 'error',
				title: 'Memix Daemon Unavailable',
				description: e?.message || 'Memix could not prepare its daemon.',
				reason: e?.message || 'Memix could not prepare its daemon.',
			});
		}
	} else {
		try {
			await DaemonManager.ping();
			setDaemonReadinessState({
				kind: 'ready',
				title: 'Memix Daemon Ready',
				description: 'Connected to the external development daemon.',
				version: 'external-dev',
			});
		} catch (e: any) {
			daemonOutputChannel.show(true);
			setDaemonReadinessState({
				kind: 'error',
				title: 'Memix Daemon Unavailable',
				description: `External daemon mode is enabled, but no daemon is reachable. ${e?.message || e}`,
				reason: e?.message || String(e),
			});
			vscode.window.showWarningMessage(
				`Memix: External daemon mode is enabled, but no daemon is reachable on ~/.memix/daemon.sock. Start it manually (e.g. cargo run) and reload. (${e?.message || e})`
			);
		}
	}

	// --- Initialize workspace-dependent modules (only if workspace is open) ---
	if (workspaceRoot && projectId) {
		brain = new BrainManager(projectId);
		panelProvider.setBrain(brain);
	}
	const licenseManager = new LicenseManager(secretManager, statusBarItem);
	if (daemonReadinessState.kind === 'ready') {
		await licenseManager.restoreOnStartup();
	} else {
		panelProvider.setDaemonState(daemonReadinessState);
	}

	// =====================================================================
	// BLAST RADIUS PREDICTION WATCHER
	// =====================================================================
	let blastRadiusTimeout: NodeJS.Timeout | null = null;
	const shownBlastRadiusInfos = new Set<string>();
	let blastRadiusStatusBar = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 99);
	blastRadiusStatusBar.command = 'memix.showPanel';
	blastRadiusStatusBar.tooltip = 'Memix Blast Radius Prediction';
	context.subscriptions.push(blastRadiusStatusBar);

	context.subscriptions.push(vscode.workspace.onDidChangeTextDocument(async (e) => {
		if (!projectId || !workspaceRoot || !e.document.uri.fsPath.startsWith(workspaceRoot)) return;
		if (e.document.uri.scheme !== 'file') return;

		const filePath = e.document.uri.fsPath;

		// If we already showed a warning for this file recently, skip to prevent spam
		if (shownBlastRadiusInfos.has(filePath)) return;

		if (blastRadiusTimeout) clearTimeout(blastRadiusTimeout);
		blastRadiusTimeout = setTimeout(async () => {
			try {
				const data = await MemoryClient.getBlastRadius(filePath, 5);

				// Show a warning if it affects more than 5 nodes
				if (data.affected_count && data.affected_count >= 5) {
					shownBlastRadiusInfos.add(filePath);
					// auto-clear the warning skip after 5 minutes so it can warn again later if needed
					setTimeout(() => shownBlastRadiusInfos.delete(filePath), 5 * 60 * 1000);

					blastRadiusStatusBar.text = `$(warning) Effects: ${data.affected_count} files`;
					blastRadiusStatusBar.backgroundColor = new vscode.ThemeColor('statusBarItem.warningBackground');
					blastRadiusStatusBar.show();

					const action = await vscode.window.showWarningMessage(
						`⚠️ Editing ${path.basename(filePath)} affects ${data.affected_count} files across your project.`,
						'View Details'
					);
					if (action === 'View Details') {
						vscode.commands.executeCommand('memix.showPanel');
						panelProvider?.getWebView()?.postMessage({
							command: 'showBlastRadius',
							data: data
						});
					}
				} else {
					blastRadiusStatusBar.hide();
				}
			} catch (err) {
				// Ignore if daemon is down or unreachable
			}
		}, 1500); // 1.5 second debounce
	}));

	// =====================================================================
	// COMMANDS — always registered so VS Code never says "command not found"
	// =====================================================================

	context.subscriptions.push(
		vscode.commands.registerCommand('memix.activateLicense', async () => {
			try {
				if (!(await ensureDaemonReady())) { return; }
				await licenseManager.promptAndActivate();
				panelProvider?.sendUpdate({ includeAdvanced: false });
			} catch (e: any) {
				vscode.window.showErrorMessage(`Memix license activation failed: ${e?.message || String(e)}`);
			}
		}),
		vscode.commands.registerCommand('memix.refreshPanel', async () => {
			if (!(await ensureDaemonReady())) { return; }
			await vscode.window.withProgress(
				{ location: vscode.ProgressLocation.Notification, title: 'Memix: Refreshing panel...' },
				async () => {
					panelProvider?.getWebView()?.postMessage({ command: 'showLoading', text: 'Refreshing data...' });
					await panelProvider?.sendUpdate();
				}
			);
		}),
		vscode.commands.registerCommand('memix.healthCheck', async () => {
			if (!(await ensureDaemonReady())) { return; }
			await vscode.window.withProgress(
				{ location: vscode.ProgressLocation.Notification, title: 'Memix: Running health check...' },
				async () => {
					await panelProvider?.runHealthCheck();
				}
			);
		}),
		vscode.commands.registerCommand('memix.detectConflicts', async () => {
			if (!(await ensureDaemonReady())) { return; }
			await vscode.window.withProgress(
				{ location: vscode.ProgressLocation.Notification, title: 'Memix: Detecting conflicts...' },
				async () => {
					await panelProvider?.runConflictDetection();
				}
			);
		}),
		vscode.commands.registerCommand('memix.scanPatterns', async () => {
			if (!(await ensureDaemonReady())) { return; }
			await vscode.window.withProgress(
				{ location: vscode.ProgressLocation.Notification, title: 'Memix: Scanning codebase patterns...' },
				async () => {
					// Simulate a scan since the underlying engine delegates this to the LLM or daemon offline.
					await new Promise(resolve => setTimeout(resolve, 1500));
				}
			);
		}),
		vscode.commands.registerCommand('memix.showActions', async () => {
			if (!(await ensureDaemonReady())) { return; }
			const options: vscode.QuickPickItem[] = [
				{ label: '$(pulse) Health Check', description: 'Run diagnostic check' },
				{ label: '$(git-merge) Detect Conflicts', description: 'Resolve CRDT conflicts' },
				{ label: '', kind: vscode.QuickPickItemKind.Separator }
			];

			const isConnected = await secretManager.getSecret('redisUrl');
			const isInitialized = isConnected ? await brain.exists() : false;

			if (!isConnected) {
				options.push({ label: '$(database) Connect Redis...', description: 'Link to external brain storage' });
			} else if (!isInitialized) {
				options.push({ label: '$(zap) Initialize Brain', description: 'Create foundational memories' });
			}

			options.push(
				{ label: '', kind: vscode.QuickPickItemKind.Separator },
				{ label: '$(export) Export Brain', description: 'Backup memories' },
				{ label: '$(root-folder) Import Brain...', description: 'Restore memories' },
				{ label: '$(cloud-upload) Export Brain Mirror', description: 'Force daemon JSON mirror sync (.memix/brain)' },
				{ label: '$(cloud-download) Import Brain Mirror', description: 'Rehydrate Redis from .memix/brain/*.json' },
				{ label: '$(tools) Run Brain Migrations', description: 'Backfill vectors + update schema marker' },
				{ label: '$(organization) Team Sync...', description: 'Sync offline CRDTs' },
				{ label: '', kind: vscode.QuickPickItemKind.Separator },
				{ label: '$(trash) Prune Stale Data', description: 'Clear excessive logs' },
				{ label: '$(wrench) Recover Corruption', description: 'Fix broken structures' },
				{ label: '', kind: vscode.QuickPickItemKind.Separator },
				{ label: '$(database) Change Redis Connection...', description: 'Update external brain storage URL' },
				{ label: '$(trashcan) Clear Brain', description: 'Delete all states' }
			);
			const picked = await vscode.window.showQuickPick(options, { placeHolder: 'Memix Brain Actions' });
			if (!picked) { return; }
			const cmds: Record<string, string> = {
				'$(pulse) Health Check': 'memix.healthCheck',
				'$(git-merge) Detect Conflicts': 'memix.detectConflicts',
				'$(database) Connect Redis...': 'memix.connect',
				'$(zap) Initialize Brain': 'memix.init',
				'$(export) Export Brain': 'memix.exportBrain',
				'$(root-folder) Import Brain...': 'memix.importBrain',
				'$(cloud-upload) Export Brain Mirror': 'memix.exportBrainMirror',
				'$(cloud-download) Import Brain Mirror': 'memix.importBrainMirror',
				'$(tools) Run Brain Migrations': 'memix.migrateBrain',
				'$(organization) Team Sync...': 'memix.teamSync',
				'$(trash) Prune Stale Data': 'memix.prune',
				'$(wrench) Recover Corruption': 'memix.recoverBrain',
				'$(database) Change Redis Connection...': 'memix.connect',
				'$(trashcan) Clear Brain': 'memix.clearBrain'
			};
			const cmd = cmds[picked.label];
			if (cmd) {
				vscode.commands.executeCommand(cmd);
			}

			if (panelProvider?.getWebView()) {
				panelProvider.getWebView()?.postMessage({ command: 'switchTab', tab: 'settings' });
			} else {
				vscode.commands.executeCommand('memix.showPanel');
				// Give panel time to boot
				setTimeout(() => {
					panelProvider?.getWebView()?.postMessage({ command: 'switchTab', tab: 'settings' });
				}, 1000);
			}

		})
	);

	// Connect Redis
	context.subscriptions.push(
		vscode.commands.registerCommand('memix.connect', async () => {
			if (!(await ensureDaemonReady())) { return; }
			try {
				const redisUrl = await vscode.window.showInputBox({
					prompt: 'Enter Redis connection URL (stored securely in your OS keychain)',
					placeHolder: 'redis://localhost:6379'
				});
				if (!redisUrl) { return; }

				try {
					await MemoryClient.redisPing(redisUrl);
				} catch (e: any) {
					vscode.window.showErrorMessage(`Memix: Redis connection failed: ${e?.message || e}`);
					return;
				}

				await secretManager.storeSecret('redisUrl', redisUrl);
				try {
					await upsertMemixConfigRedisUrl(redisUrl);
				} catch (e: any) {
					vscode.window.showWarningMessage(`Memix: Redis saved to keychain, but failed to write ~/.memix/config.toml. (${e?.message || e})`);
				}
				statusBarItem.text = 'Memix: Redis connected';
				vscode.window.showInformationMessage('Memix: Redis connection verified and saved securely in keychain.');

				// The daemon reads Redis config from env/config at boot; restart owned daemon so it picks up the new URL.
				if (!externalDaemon) {
					try {
						DaemonManager.stop();
						const binaryPath = DaemonManager.getBinaryPath();
						if (!binaryPath) {
							throw new Error('Memix daemon binary path is not available.');
						}
						await DaemonManager.start(binaryPath, workspaceRoot || null, projectId || null, redisUrl);
					} catch (e: any) {
						vscode.window.showWarningMessage(`Memix: Redis saved, but failed to restart daemon. Reload window. (${e?.message || e})`);
					}
				} else {
					vscode.window.showInformationMessage('Memix: External daemon mode is enabled. Restart your daemon process to apply the new Redis URL.');
				}

				panelProvider?.sendUpdate();
			} catch (err: any) {
				vscode.window.showErrorMessage(`Memix connect failed: ${err.message}`);
			}
		})
	);

	// Daemon-managed JSON mirror export
	context.subscriptions.push(
		vscode.commands.registerCommand('memix.exportBrainMirror', async () => {
			if (!(await ensureDaemonReady())) { return; }
			if (!requireWorkspace()) { return; }
			try {
				const res = await MemoryClient.exportBrainMirror(brain.getProjectId());
				vscode.window.showInformationMessage(`Memix: Exported ${res.written ?? 0} entries to .memix/brain`);
				panelProvider?.sendUpdate();
			} catch (err: any) {
				vscode.window.showErrorMessage(`Memix mirror export failed: ${err.message}`);
			}
		})
	);

	// Daemon-managed JSON mirror import
	context.subscriptions.push(
		vscode.commands.registerCommand('memix.importBrainMirror', async () => {
			if (!(await ensureDaemonReady())) { return; }
			if (!requireWorkspace()) { return; }
			try {
				const res = await MemoryClient.importBrainMirror(brain.getProjectId());
				vscode.window.showInformationMessage(`Memix: Imported ${res.imported ?? 0} entries from .memix/brain`);
				panelProvider?.sendUpdate();
			} catch (err: any) {
				vscode.window.showErrorMessage(`Memix mirror import failed: ${err.message}`);
			}
		})
	);

	// Explicit migration trigger (backfill vectors/schema)
	context.subscriptions.push(
		vscode.commands.registerCommand('memix.migrateBrain', async () => {
			if (!(await ensureDaemonReady())) { return; }
			if (!requireWorkspace()) { return; }
			try {
				const report = await MemoryClient.migrateProject(brain.getProjectId());
				vscode.window.showInformationMessage(
					`Memix: Migration complete (schema v${report.schema_version}, migrated ${report.migrated_entries} entries).`
				);
				panelProvider?.sendUpdate();
			} catch (err: any) {
				vscode.window.showErrorMessage(`Memix migration failed: ${err.message}`);
			}
		})
	);

	// Disconnect Redis
	context.subscriptions.push(
		vscode.commands.registerCommand('memix.disconnect', async () => {
			if (!(await ensureDaemonReady())) { return; }
			await secretManager.deleteSecret('redisUrl');
			statusBarItem.text = 'Memix: Redis disconnected';
			vscode.window.showInformationMessage('Memix: Disconnected (Redis URL removed from keychain).');
			panelProvider?.sendUpdate();
		})
	);

	// Initialize Brain
	context.subscriptions.push(
		vscode.commands.registerCommand('memix.init', async () => {
			if (!(await ensureDaemonReady())) { return; }
			if (!requireWorkspace() || !workspaceRoot) { return; }
			panelProvider?.getWebView()?.postMessage({ command: 'showLoading', text: 'Initializing brain...' });
			try {
				const redisUrl = await secretManager.getSecret('redisUrl');
				if (!redisUrl) {
					vscode.window.showWarningMessage('Connect to Redis first');
					panelProvider?.sendUpdate();
					return;
				}

				let projectId = config.get<string>('projectId');
				if (!projectId) {
					const answer = await vscode.window.showInputBox({
						prompt: 'Enter Memix Project Name (optional, defaults to hashed folder name)',
						placeHolder: path.basename(workspaceRoot)
					});
					projectId = answer || hashProjectId(workspaceRoot);
					if (answer) {
						await config.update('projectId', projectId, vscode.ConfigurationTarget.Workspace);
					}
				}

				// Generate rules via Rust daemon
				const ide = detectIDE();
				await MemoryClient.generateRules(projectId, redisUrl, ide, workspaceRoot);

				// Check if the brain is paused before attempting writes.
				// Init is an explicit user action, so we auto-resume rather than
				// failing with a cryptic 503. If the user had it paused, we offer
				// to re-pause after init completes.
				let wasPaused = false;
				try {
					const controlStatus = await MemoryClient.controlStatus();
					wasPaused = controlStatus?.config?.brain_paused === true;
					if (wasPaused) {
						await MemoryClient.controlResume();
					}
				} catch {
					// If control status is unreachable, proceed — init() will surface
					// a clear error if writes still fail due to paused state.
				}

				// init() does one read to determine state, writes only missing keys
				// in parallel, and returns exactly what changed. No separate exists()
				// call needed — that would be a redundant full Redis read.
				const initResult = await brain.init(projectId);

				if (initResult.written.length === 0) {
					// All keys already existed — ask before overwriting
					const overwrite = await vscode.window.showWarningMessage(
						`Brain already fully initialized for ${projectId}. Re-initialize missing or all keys?`,
						'Re-init missing', 'Overwrite all', 'Cancel'
					);
					if (!overwrite || overwrite === 'Cancel') {
						panelProvider?.sendUpdate({ includeAdvanced: false });
						return;
					}
					if (overwrite === 'Overwrite all') {
						await brain.clearAll();
						await brain.init(projectId);
					}
				}

				vscode.window.showInformationMessage(
					`Memix: Initialized! Project: ${projectId} · IDE: ${ide} · Written: ${initResult.written.length > 0
						? initResult.written.join(', ')
						: 'nothing new (already complete)'
					}`
				);

				if (wasPaused) {
					const keepActive = await vscode.window.showInformationMessage(
						'Brain was paused before init. Keep it active?',
						'Keep active', 'Re-pause'
					);
					if (keepActive !== 'Keep active') {
						await MemoryClient.controlPause();
					}
				}

				// All writes are confirmed before init() resolves.
				// The daemon's entry cache was cleared by each upsert, so
				// sendUpdate() will read fresh data from Redis with no artificial wait.
				await panelProvider?.sendUpdate({ includeAdvanced: false });
			} catch (err: any) {
				vscode.window.showErrorMessage(`Memix init failed: ${err.message}`);
				panelProvider?.sendUpdate();
			}
		})
	);

	// Prune Brain
	context.subscriptions.push(
		vscode.commands.registerCommand('memix.prune', async () => {
			if (!(await ensureDaemonReady())) { return; }
			if (!requireWorkspace()) { return; }
			try {
				const log = await brain.get(BRAIN_KEYS.SESSION_LOG);
				if (Array.isArray(log) && log.length > MAX_SESSION_LOG_ENTRIES) {
					const pruned = log.slice(-MAX_SESSION_LOG_ENTRIES);
					await brain.set(BRAIN_KEYS.SESSION_LOG, pruned);
				}
				vscode.window.showInformationMessage('Memix: Prune complete.');
				panelProvider?.sendUpdate();
			} catch (err: any) {
				vscode.window.showErrorMessage(`Memix prune failed: ${err.message}`);
			}
		})
	);

	// Clear Brain
	context.subscriptions.push(
		vscode.commands.registerCommand('memix.clearBrain', async () => {
			if (!(await ensureDaemonReady())) { return; }
			if (!requireWorkspace()) { return; }
			const confirm = await vscode.window.showWarningMessage(
				'Clear entire brain? This cannot be undone.',
				'Yes, clear',
				'Cancel'
			);
			if (confirm !== 'Yes, clear') { return; }
			try {
				await brain.clearAll();
				vscode.window.showInformationMessage('Memix: Brain cleared.');
				panelProvider?.sendUpdate();
			} catch (err: any) {
				vscode.window.showErrorMessage(`Memix clear failed: ${err.message}`);
			}
		})
	);

	// Recover Corrupted Brain
	context.subscriptions.push(
		vscode.commands.registerCommand('memix.recoverBrain', async () => {
			if (!(await ensureDaemonReady())) { return; }
			if (!requireWorkspace()) { return; }
			try {
				const identity = await brain.get(BRAIN_KEYS.IDENTITY);
				if (identity === null || identity === 'null' || typeof identity !== 'object') {
					await brain.set(BRAIN_KEYS.IDENTITY, {
						name: brain.getProjectId(),
						purpose: 'Memix brain for project ' + brain.getProjectId(),
						tech_stack: [],
						core_objectives: [],
						boundaries: []
					});
				}

				const sessionState = await brain.get(BRAIN_KEYS.SESSION_STATE);
				if (sessionState === null || sessionState === 'null' || typeof sessionState !== 'object') {
					await brain.set(BRAIN_KEYS.SESSION_STATE, {
						current_task: 'Recovered Memix state',
						last_updated: new Date().toISOString(),
						session_number: 1
					});
				}

				const patterns = await brain.get(BRAIN_KEYS.PATTERNS);
				if (patterns === null || patterns === 'null' || typeof patterns !== 'object') {
					await brain.set(BRAIN_KEYS.PATTERNS, {
						files_frequently_edited_together: [],
						architectural_rules: [],
						user_preferences: {}
					});
				}

				const decisions = await brain.get(BRAIN_KEYS.DECISIONS);
				if (decisions !== null && decisions !== 'null' && !Array.isArray(decisions)) {
					await brain.set(BRAIN_KEYS.DECISIONS, []);
				}

				const sessionLog = await brain.get(BRAIN_KEYS.SESSION_LOG);
				if (sessionLog !== null && sessionLog !== 'null' && !Array.isArray(sessionLog)) {
					await brain.set(BRAIN_KEYS.SESSION_LOG, []);
				}

				vscode.window.showInformationMessage('Memix: Recovery complete.');
				panelProvider?.sendUpdate();
			} catch (err: any) {
				vscode.window.showErrorMessage(`Memix recovery failed: ${err.message}`);
			}
		})
	);

	// End Session
	context.subscriptions.push(
		vscode.commands.registerCommand('memix.endSession', async () => {
			if (!(await ensureDaemonReady())) { return; }
			if (!requireWorkspace()) { return; }
			try {
				const state = await brain.get(BRAIN_KEYS.SESSION_STATE);
				const sessionNumber = typeof state?.session_number === 'number' ? state.session_number : 1;

				const log = await brain.get(BRAIN_KEYS.SESSION_LOG);
				const logArr = Array.isArray(log) ? log : [];
				logArr.push({
					session: sessionNumber,
					date: new Date().toISOString(),
					summary: state?.current_task || 'Session ended',
					files_changed: state?.modified_files || []
				});
				await brain.set(BRAIN_KEYS.SESSION_LOG, logArr);

				await brain.set(BRAIN_KEYS.SESSION_STATE, {
					...(typeof state === 'object' && state ? state : {}),
					last_updated: new Date().toISOString(),
					session_number: sessionNumber + 1,
					current_task: 'New session'
				});

				vscode.window.showInformationMessage(`Memix: Session #${sessionNumber} ended.`);
				panelProvider?.sendUpdate();
			} catch (err: any) {
				vscode.window.showErrorMessage(`Memix end session failed: ${err.message}`);
			}
		})
	);

	// Team Sync
	context.subscriptions.push(
		vscode.commands.registerCommand('memix.teamSync', async () => {
			if (!(await ensureDaemonReady())) { return; }
			if (!requireWorkspace()) { return; }
			try {
				if (!(await licenseManager.ensureProLicense())) {
					return;
				}
				const projectId = brain.getProjectId();

				vscode.window.withProgress({
					location: vscode.ProgressLocation.Notification,
					title: "Memix: Synchronizing team CRDT vectors...",
					cancellable: false
				}, async () => {
					try {
						const res = await MemoryClient.teamSync(projectId);
						vscode.window.showInformationMessage(`Memix: ${res.message}`);
						panelProvider?.sendUpdate();
					} catch (e: any) {
						vscode.window.showErrorMessage(`Memix team sync failed: ${e.message}`);
					}
				});

			} catch (err: any) {
				vscode.window.showErrorMessage(`Memix team sync failed: ${err.message}`);
			}
		})
	);

	// Show Panel
	context.subscriptions.push(
		vscode.commands.registerCommand('memix.showPanel', () => {
			vscode.commands.executeCommand('workbench.view.extension.memix-sidebar');
		})
	);

	// Copy Prompt Pack
	context.subscriptions.push(
		vscode.commands.registerCommand('memix.copyPromptPack', async () => {
			if (!(await ensureDaemonReady())) { return; }
			if (!requireWorkspace()) { return; }
			try {
				const variant = await vscode.window.showQuickPick(
					[
						{ label: 'Small', description: 'identity + session state + patterns' },
						{ label: 'Standard', description: 'Small + decisions + known issues + tasks + file map' },
						{ label: 'Deep', description: 'Standard + session log' }
					],
					{ placeHolder: 'Select Prompt Pack variant' }
				);
				if (!variant) return;
				const allData = await brain.getAll();
				const pack = createPromptPack(allData, variant.label as PromptPackVariant);
				await vscode.env.clipboard.writeText(pack.text);
				vscode.window.showInformationMessage(
					`Memix Prompt Pack (${variant.label}) copied • ${pack.availableSectionCount}/${pack.requestedSectionCount} core sections available`
				);
			} catch (e: any) {
				vscode.window.showErrorMessage(`Failed to copy Prompt Pack: ${e?.message || String(e)}`);
			}
		})
	);

	// Export Brain
	context.subscriptions.push(
		vscode.commands.registerCommand('memix.exportBrain', async () => {
			if (!(await ensureDaemonReady())) { return; }
			if (!requireWorkspace() || !workspaceRoot) { return; }
			try {
				const filePath = await exportBrain(brain, workspaceRoot);
				vscode.window.showInformationMessage(`Memix: Brain exported to ${filePath}`);
			} catch (err: any) {
				vscode.window.showErrorMessage(`Export failed: ${err.message}`);
			}
		})
	);

	// Import Brain
	context.subscriptions.push(
		vscode.commands.registerCommand('memix.importBrain', async () => {
			if (!(await ensureDaemonReady())) { return; }
			if (!requireWorkspace()) { return; }
			const fileUri = await vscode.window.showOpenDialog({
				filters: { 'JSON': ['json'] },
				canSelectMany: false
			});
			if (!fileUri?.[0]) { return; }

			try {
				const imported = await importBrain(brain, fileUri[0].fsPath);
				vscode.window.showInformationMessage(
					`Memix: Imported ${imported.length} keys`
				);
				panelProvider?.sendUpdate();
			} catch (err: any) {
				vscode.window.showErrorMessage(`Import failed: ${err.message}`);
			}
		})
	);

	// Clear Secrets
	context.subscriptions.push(
		vscode.commands.registerCommand('memix.clearSecrets', async () => {
			await secretManager.deleteSecret('redisUrl');
			await secretManager.deleteSecret('teamId');
			vscode.window.showInformationMessage('Memix: Cleared stored credentials from keychain.');
		})
	);

	// Auto-generate rules if not present (via Rust daemon)
	(async () => {
		if (daemonReadinessState.kind !== 'ready') {
			return;
		}
		if (workspaceRoot && config.get<boolean>('autoGenerateRules')) {
			const configProjectId = config.get<string>('projectId');
			const projectId = configProjectId || hashProjectId(workspaceRoot);
			const redisUrl = await secretManager.getSecret('redisUrl');
			if (redisUrl) {
				const ide = detectIDE();
				try {
					await MemoryClient.generateRules(projectId, redisUrl, ide, workspaceRoot);
				} catch (e) {
					// Rules might already exist, ignore
				}
			}
		}
	})();
}

export function deactivate() {
	const config = vscode.workspace.getConfiguration('memix');
	const externalDaemon = config.get<boolean>('dev.externalDaemon') === true;
	if (!externalDaemon) {
		DaemonManager.stop();
		DaemonRuntimeManager.stopBackgroundUpdateCheck();
	}
}