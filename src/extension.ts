import * as vscode from 'vscode';
import * as path from 'path';
import * as redisClient from './core/redis-client';
import { BrainManager } from './core/brain';
import { MemoryOrchestrator } from './core/orchestrator';
import { HealthMonitor } from './core/health';
import { BrainPruner } from './core/pruner';
import { SessionScorer } from './core/scoring';
import { TeamSync } from './core/team';
import { RulesEngine } from './ide/rules-engine';
import { DebugPanelProvider } from './panel/debug-panel';
import { exportBrain, importBrain } from './utils/exporter';
import { hashProjectId } from './utils/crypto';
import { BRAIN_KEYS } from './utils/constants';
import { SecretManager } from './core/secrets';

let brain: BrainManager;
let orchestrator: MemoryOrchestrator;
let scorer: SessionScorer;
let pruner: BrainPruner;
let panelProvider: DebugPanelProvider;
let statusBarItem: vscode.StatusBarItem;

/** Helper — returns true if workspace-dependent modules are ready */
function requireWorkspace(): boolean {
	if (!brain) {
		vscode.window.showWarningMessage('Memix: Open a workspace folder first.');
		return false;
	}
	return true;
}

export async function activate(context: vscode.ExtensionContext) {
	const workspaceRoot = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
	const config = vscode.workspace.getConfiguration('memix');

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

	// --- Initialize workspace-dependent modules (only if workspace is open) ---
	if (workspaceRoot) {
		const configProjectId = config.get<string>('projectId');
		const projectId = configProjectId || hashProjectId(path.basename(workspaceRoot));

		brain = new BrainManager(projectId);
		orchestrator = new MemoryOrchestrator(brain);
		scorer = new SessionScorer(brain);
		pruner = new BrainPruner(brain);
	}

	// --- Debug Panel (always registered so sidebar doesn't get stuck loading) ---
	panelProvider = new DebugPanelProvider(
		context.extensionUri,
		brain || null,
		workspaceRoot || null
	);
	context.subscriptions.push(
		vscode.window.registerWebviewViewProvider(
			DebugPanelProvider.viewType,
			panelProvider,
			{ webviewOptions: { retainContextWhenHidden: true } }
		)
	);

	// --- Status Bar (always created) ---
	statusBarItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 100);
	statusBarItem.text = 'Memix: Disconnected';
	statusBarItem.command = 'memix.showPanel';
	statusBarItem.show();
	context.subscriptions.push(statusBarItem);

	// Listen for connection changes
	redisClient.onStatusChange((status) => {
		statusBarItem.text = status === 'connected'
			? 'Memix: Connected'
			: status === 'error'
				? 'Memix: Error'
				: 'Memix: Disconnected';
	});

	// =====================================================================
	// COMMANDS — always registered so VS Code never says "command not found"
	// =====================================================================

	// Connect Redis
	context.subscriptions.push(
		vscode.commands.registerCommand('memix.connect', async () => {
			let redisUrl = await secretManager.getSecret('redisUrl');

			if (!redisUrl) {
				redisUrl = await vscode.window.showInputBox({
					prompt: 'Enter Redis URL',
					placeHolder: 'redis://:password@localhost:6379',
					password: true
				});
				if (!redisUrl) { return; }
				await secretManager.storeSecret('redisUrl', redisUrl);
			}

			try {
				await redisClient.connect(redisUrl);
				vscode.window.showInformationMessage('Memix: Connected to Redis ✅');

				// Auto-generate rules if enabled and workspace is open
				if (workspaceRoot && config.get<boolean>('autoGenerateRules')) {
					const configProjectId = config.get<string>('projectId');
					const projectId = configProjectId || hashProjectId(path.basename(workspaceRoot));
					const rulesEngine = new RulesEngine(workspaceRoot);
					if (!rulesEngine.rulesExist()) {
						await rulesEngine.generateRules(projectId, redisUrl);
					}
				}

				panelProvider?.sendUpdate();
			} catch (err: any) {
				vscode.window.showErrorMessage(`Memix: Redis connection failed — ${err.message}`);
			}
		})
	);

	// Disconnect Redis
	context.subscriptions.push(
		vscode.commands.registerCommand('memix.disconnect', async () => {
			await redisClient.disconnect();
			vscode.window.showInformationMessage('Memix: Disconnected from Redis');
		})
	);

	// Initialize Brain
	context.subscriptions.push(
		vscode.commands.registerCommand('memix.init', async () => {
			if (!requireWorkspace() || !workspaceRoot) { return; }
			try {
				const redisUrl = await secretManager.getSecret('redisUrl');
				if (!redisUrl) {
					vscode.window.showWarningMessage('Connect to Redis first');
					return;
				}

				let projectId = config.get<string>('projectId');
				if (!projectId) {
					const answer = await vscode.window.showInputBox({
						prompt: 'Enter Memix Project Name (optional, defaults to hashed folder name)',
						placeHolder: path.basename(workspaceRoot)
					});
					projectId = answer || hashProjectId(path.basename(workspaceRoot));
					if (answer) {
						await config.update('projectId', projectId, vscode.ConfigurationTarget.Workspace);
					}
				}
				const rulesEngine = new RulesEngine(workspaceRoot);
				await rulesEngine.generateRules(projectId, redisUrl);

				// Check if brain already exists
				const exists = await brain.exists();
				if (exists) {
					const overwrite = await vscode.window.showWarningMessage(
						'Brain already exists for this project. Overwrite?',
						'Yes', 'No'
					);
					if (overwrite !== 'Yes') { return; }
				}

				// Actually initialize the Redis database keys
				await brain.init(projectId);

				vscode.window.showInformationMessage(
					`Memix: Initialized! Project ID: ${projectId}. Rules generated for ${rulesEngine.getConfig().ide}.`
				);
				panelProvider?.sendUpdate();
			} catch (err: any) {
				vscode.window.showErrorMessage(`Memix init failed: ${err.message}`);
			}
		})
	);

	// Show Panel
	context.subscriptions.push(
		vscode.commands.registerCommand('memix.showPanel', () => {
			vscode.commands.executeCommand('workbench.view.extension.memix-sidebar');
		})
	);

	// Export Brain
	context.subscriptions.push(
		vscode.commands.registerCommand('memix.exportBrain', async () => {
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

	// Health Check
	context.subscriptions.push(
		vscode.commands.registerCommand('memix.healthCheck', async () => {
			if (!requireWorkspace()) { return; }
			const monitor = new HealthMonitor(brain);
			const report = await monitor.runFullCheck();

			const msg = `Brain Health: ${report.status.toUpperCase()}\n`
				+ `Size: ${(report.totalSizeBytes / 1024).toFixed(1)}KB\n`
				+ `Issues: ${report.recommendations.length}`;

			if (report.status === 'critical') {
				vscode.window.showErrorMessage(msg);
			} else if (report.status === 'warning') {
				vscode.window.showWarningMessage(msg);
			} else {
				vscode.window.showInformationMessage(msg);
			}
		})
	);

	// Prune
	context.subscriptions.push(
		vscode.commands.registerCommand('memix.prune', async () => {
			if (!requireWorkspace()) { return; }
			const actions = await pruner.prune();
			vscode.window.showInformationMessage(
				`Memix Prune: ${actions.join('; ')}`
			);
			panelProvider?.sendUpdate();
		})
	);

	// Clear Brain
	context.subscriptions.push(
		vscode.commands.registerCommand('memix.clearBrain', async () => {
			if (!requireWorkspace()) { return; }
			const confirm = await vscode.window.showWarningMessage(
				'This will delete ALL brain data. Are you sure?',
				'Yes, clear everything', 'Cancel'
			);
			if (confirm !== 'Yes, clear everything') { return; }
			await brain.clearAll();
			vscode.window.showInformationMessage('Memix: Brain cleared');
			panelProvider?.sendUpdate();
		})
	);

	// Recover Corrupted Brain
	context.subscriptions.push(
		vscode.commands.registerCommand('memix.recoverBrain', async () => {
			if (!requireWorkspace()) { return; }
			const actions = await pruner.recoverCorruption();
			vscode.window.showInformationMessage(
				`Memix Recovery: ${actions.join('; ')}`
			);
			panelProvider?.sendUpdate();
		})
	);

	// Team Sync
	context.subscriptions.push(
		vscode.commands.registerCommand('memix.teamSync', async () => {
			if (!requireWorkspace() || !workspaceRoot) { return; }
			let teamId = await secretManager.getSecret('teamId');
			if (!teamId) {
				teamId = await vscode.window.showInputBox({
					prompt: 'Enter Team ID (share this with teammates)',
					password: true
				});
				if (!teamId) { return; }
				await secretManager.storeSecret('teamId', teamId);
			}

			const action = await vscode.window.showQuickPick(
				['Push to team', 'Pull from team', 'Merge decisions'],
				{ placeHolder: 'Team sync action' }
			);

			const configProjectId = config.get<string>('projectId');
			const projectId = configProjectId || hashProjectId(path.basename(workspaceRoot));
			const memberId = hashProjectId(require('os').hostname());
			const team = new TeamSync(brain, teamId || '', projectId, memberId);

			if (action === 'Push to team') {
				const pushed = await team.pushToTeam();
				vscode.window.showInformationMessage(`Pushed ${pushed.length} keys to team`);
			} else if (action === 'Pull from team') {
				const pulled = await team.pullFromTeam();
				vscode.window.showInformationMessage(`Pulled ${pulled.length} keys from team`);
				panelProvider?.sendUpdate();
			} else if (action === 'Merge decisions') {
				const count = await team.mergeDecisions();
				vscode.window.showInformationMessage(`Merged ${count} new decisions from team`);
			}
		})
	);

	// --- FILE SAVE WATCHER ---
	context.subscriptions.push(
		vscode.workspace.onDidSaveTextDocument(async (doc) => {
			if (!brain || redisClient.getStatus() !== 'connected') { return; }

			try {
				const relativePath = vscode.workspace.asRelativePath(doc.uri);
				const keysToUpdate = await orchestrator.determineUpdates({
					type: 'file_modified',
					data: { file: relativePath }
				});

				// Auto-update file_map with the saved file's purpose
				if (keysToUpdate.includes(BRAIN_KEYS.FILE_MAP)) {
					const fileMap = await brain.get(BRAIN_KEYS.FILE_MAP) || {};
					if (!fileMap[relativePath]) {
						fileMap[relativePath] = `Modified on ${new Date().toISOString().split('T')[0]}`;
						await brain.set(BRAIN_KEYS.FILE_MAP, fileMap);
					}
				}

				scorer.increment('files_modified');
			} catch {
				// Silent fail on auto-tracking
			}
		})
	);

	// End Session
	context.subscriptions.push(
		vscode.commands.registerCommand('memix.endSession', async () => {
			if (!requireWorkspace()) { return; }
			if (redisClient.getStatus() !== 'connected') {
				vscode.window.showWarningMessage('Connect to Redis first');
				return;
			}

			try {
				const state = await brain.get(BRAIN_KEYS.SESSION_STATE);
				const sessionNumber = state?.session_number || 0;

				// Save scoring
				await scorer.saveScore(sessionNumber);

				// Append to session log
				let log = await brain.get(BRAIN_KEYS.SESSION_LOG) || [];
				if (!Array.isArray(log)) { log = []; }

				log.push({
					session: sessionNumber,
					date: new Date().toISOString(),
					summary: state?.current_task || 'No task recorded',
					files_changed: state?.modified_files || [],
					score: scorer.getScore()
				});

				await brain.set(BRAIN_KEYS.SESSION_LOG, log);

				// Increment session number for next session
				if (state) {
					state.session_number = sessionNumber + 1;
					state.progress = [];
					state.current_task = '';
					state.next_steps = [];
					state.modified_files = [];
					state.last_updated = new Date().toISOString();
					await brain.set(BRAIN_KEYS.SESSION_STATE, state);
				}

				scorer.reset();

				vscode.window.showInformationMessage(
					`Memix: Session #${sessionNumber} ended. Score saved. Brain synced. ✅`
				);
				panelProvider?.sendUpdate();
			} catch (err: any) {
				vscode.window.showErrorMessage(`End session failed: ${err.message}`);
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

	// --- AUTO-CONNECT on startup ---
	const savedUrl = await secretManager.getSecret('redisUrl');
	if (savedUrl) {
		redisClient.connect(savedUrl).then(() => {
			// Auto-generate rules if not present
			if (workspaceRoot && config.get<boolean>('autoGenerateRules')) {
				const configProjectId = config.get<string>('projectId');
				const projectId = configProjectId || hashProjectId(path.basename(workspaceRoot));
				const rulesEngine = new RulesEngine(workspaceRoot);
				if (!rulesEngine.rulesExist()) {
					rulesEngine.generateRules(projectId, savedUrl);
				}
			}
		}).catch(() => {
			// Silent fail on auto-connect — user can manually connect
			statusBarItem.text = 'Memix: Offline';
		});
	}
}

export function deactivate() {
	redisClient.disconnect();
}