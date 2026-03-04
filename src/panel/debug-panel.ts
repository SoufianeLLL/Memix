import * as vscode from 'vscode';
import { BrainManager } from '../core/brain';
import { HealthMonitor } from '../core/health';
import { ConflictHandler } from '../core/conflict';
import { BRAIN_KEYS, TAXONOMY_MAP } from '../utils/constants';
import * as redisClient from '../core/redis-client';

export class DebugPanelProvider implements vscode.WebviewViewProvider {
	public static readonly viewType = 'memix.debugPanel';

	private _view?: vscode.WebviewView;
	private brain: BrainManager | null;
	private health: HealthMonitor | null;
	private conflicts: ConflictHandler | null;

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

	resolveWebviewView(webviewView: vscode.WebviewView) {
		this._view = webviewView;

		webviewView.webview.options = {
			enableScripts: true,
			localResourceRoots: [this.extensionUri]
		};

		webviewView.webview.html = this.getHtml();

		webviewView.webview.onDidReceiveMessage(async (msg) => {
			switch (msg.command) {
				case 'refresh':
					await this.sendUpdate();
					break;
				case 'clearBrain':
					if (!this.brain) { return; }
					const confirm = await vscode.window.showWarningMessage(
						'Clear entire brain? This cannot be undone.',
						'Yes, clear', 'Cancel'
					);
					if (confirm === 'Yes, clear') {
						await this.brain.clearAll();
						await this.sendUpdate();
					}
					break;
				case 'initBrain':
					await vscode.commands.executeCommand('memix.init');
					await this.sendUpdate();
					break;
				case 'exportBrain':
					await vscode.commands.executeCommand('memix.exportBrain');
					await this.sendUpdate();
					break;
				case 'importBrain':
					await vscode.commands.executeCommand('memix.importBrain');
					await this.sendUpdate();
					break;
				case 'teamSync':
					await vscode.commands.executeCommand('memix.teamSync');
					await this.sendUpdate();
					break;
				case 'prune':
					await vscode.commands.executeCommand('memix.prune');
					await this.sendUpdate();
					break;
				case 'recoverBrain':
					await vscode.commands.executeCommand('memix.recoverBrain');
					await this.sendUpdate();
					break;
				case 'healthCheck':
					if (!this.health) { return; }
					const report = await this.health.runFullCheck();
					webviewView.webview.postMessage({ command: 'healthReport', data: report });
					break;
				case 'detectConflicts':
					if (!this.conflicts) { return; }
					const conflictList = await this.conflicts.detectConflicts();
					webviewView.webview.postMessage({ command: 'conflicts', data: conflictList });
					break;
			}
		});

		// Defer initial load so the webview script has time to register its listener
		setTimeout(() => this.sendUpdate(), 500);
	}

	async sendUpdate() {
		if (!this._view) { return; }

		if (!this.brain || redisClient.getStatus() !== 'connected') {
			this._view.webview.postMessage({
				command: 'error',
				data: redisClient.getStatus() === 'connected'
					? 'No workspace open'
					: 'Not connected to Redis. Run "Memix: Connect Redis"'
			});
			return;
		}

		try {
			const sizeInfo = await this.brain.getSize();
			const memoryInfo = await redisClient.infoMemory();
			const allData = await this.brain.getAll();
			const healthReport = await this.health!.runFullCheck();

			const categories: Record<string, { keys: string[]; size: number }> = {};
			for (const [key, tax] of Object.entries(TAXONOMY_MAP)) {
				if (!categories[tax]) { categories[tax] = { keys: [], size: 0 }; }
				categories[tax].keys.push(key);
				categories[tax].size += sizeInfo.keys[key] || 0;
			}

			this._view.webview.postMessage({
				command: 'update',
				data: {
					totalSizeBytes: sizeInfo.totalBytes,
					redisUsedBytes: memoryInfo.usedBytes,
					redisMaxBytes: memoryInfo.maxBytes,
					keys: sizeInfo.keys,
					categories,
					health: healthReport.status,
					recommendations: healthReport.recommendations,
					lastUpdated: allData[BRAIN_KEYS.SESSION_STATE]?.last_updated || 'Never',
					sessionNumber: allData[BRAIN_KEYS.SESSION_STATE]?.session_number || 0,
					currentTask: allData[BRAIN_KEYS.SESSION_STATE]?.current_task || 'None',
					keyCount: Object.keys(sizeInfo.keys).length,
					isInitialized: !!sizeInfo.keys[BRAIN_KEYS.IDENTITY]
				}
			});
		} catch (e) {
			this._view.webview.postMessage({
				command: 'error',
				data: 'Failed to read brain data'
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
		.spinner {
			animation: spin 1s linear infinite;
			margin-bottom: 12px;
			color: var(--vscode-textLink-foreground);
		}
		@keyframes spin { 100% { transform: rotate(360deg); } }
		.card {
			background: transparent;
			border: 1px solid rgba(255, 255, 255, 0.07);
			border-radius: 6px;
			padding: 10px;
			margin-bottom: 8px;
		}
		.card h3 {
			margin: 0 0 7px 0;
			font-size: 12px;
		}
		.stat {
			display: flex;
			justify-content: space-between;
			padding: 4px 0;
			border: 0;
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
		.action-bar {
			display: flex;
			gap: 4px;
			align-items: center;
			margin-top: 3px;
		}
		.action-bar select {
			flex: 1;
			padding: 5px 6px;
			border: 1px solid var(--vscode-dropdown-border, var(--vscode-panel-border));
			background: transparent;
			color: var(--vscode-dropdown-foreground, var(--vscode-foreground));
			border-radius: 4px;
			font-size: 11px;
			font-family: var(--vscode-font-family);
			cursor: pointer;
			outline: none;
		}
		.action-bar select:focus {
			border-color: var(--vscode-focusBorder);
		}
		.action-bar select option { padding: 4px; }
		.run-btn {
			padding: 5px 10px;
			border: 1px solid var(--vscode-button-border, var(--vscode-panel-border));
			background: var(--vscode-button-background);
			color: var(--vscode-button-foreground);
			border-radius: 4px;
			cursor: pointer;
			font-size: 11px;
			font-family: var(--vscode-font-family);
			white-space: nowrap;
		}
		.run-btn:hover { opacity: 0.9; }
		.warning { font-size: 11px; margin: 4px 0; }
		.category { margin: 4px 0 8px; }
		.category-name {
			display: flex;
			justify-content: space-between;
			font-size: 12px;
			text-transform: capitalize;
		}
		.category-name span {
			font-size: 11px;
			font-weight: bold;
			font-size: 11px;
		}
		#error-banner {
			color: #f44747;
			margin-bottom: 12px;
			display: none;
		}
	</style>
</head>
<body>
	<div id="loading-overlay">
		<svg class="spinner" width="24" height="24" viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
			<path d="M8 2a6 6 0 100 12A6 6 0 008 2z" stroke="currentColor" stroke-width="1.5" stroke-dasharray="27 10" stroke-linecap="round"/>
		</svg>
		<div id="loading-text" style="font-size:12px;color:var(--vscode-foreground)">Connecting to Memix...</div>
	</div>

	<div id="error-banner"></div>

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
			<span>Redis Dataset</span>
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

	<div class="card">
		<h3>Actions</h3>
		<div class="action-bar">
			<select id="actionSelect">
				<option value="" disabled selected>Choose action</option>
				<option value="refresh">Refresh</option>
				<option value="healthCheck">Health Check</option>
				<option value="detectConflicts">Detect Conflicts</option>
				<option value="" disabled>Data Management</option>
				<option id="opt-init" value="initBrain">Initialize Brain</option>
				<option value="exportBrain">Export Brain</option>
				<option value="importBrain">Import Brain...</option>
				<option value="teamSync">Team Sync...</option>
				<option value="" disabled>Maintenance</option>
				<option value="prune">Prune Stale Data</option>
				<option value="recoverBrain">Recover Corruption</option>
				<option value="" disabled>Danger</option>
				<option value="clearBrain">Clear Brain</option>
			</select>
			<button class="run-btn" id="runAction">Run</button>
		</div>
	</div>

	<script>
		const vscode = acquireVsCodeApi();
		
		function showLoading(text) {
			document.getElementById('loading-text').textContent = text;
			document.getElementById('loading-overlay').style.display = 'flex';
		}
		function hideLoading() {
			document.getElementById('loading-overlay').style.display = 'none';
		}

		function send(cmd) {
			var msgs = {
				'refresh': 'Refreshing data...',
				'healthCheck': 'Running health check...',
				'detectConflicts': 'Detecting conflicts...',
				'initBrain': 'Initializing brain...',
				'exportBrain': 'Prompting export...',
				'importBrain': 'Prompting import...',
				'teamSync': 'Initiating team sync...',
				'prune': 'Pruning stale data...',
				'recoverBrain': 'Recovering brain...',
				'clearBrain': 'Clearing brain...'
			};
			if (msgs[cmd]) showLoading(msgs[cmd]);
			vscode.postMessage({ command: cmd });
		}

		document.getElementById('runAction').addEventListener('click', function() {
			var sel = document.getElementById('actionSelect');
			if (sel.value) {
				send(sel.value);
				sel.selectedIndex = 0;
			}
		});

		window.addEventListener('message', function(e) {
			hideLoading();
			var command = e.data.command;
			var data = e.data.data;

			if (command === 'update') {
				document.getElementById('health').textContent = data.health.toUpperCase();
				document.getElementById('health').className = 'stat-value health-' + data.health;

				var sizeKB = (data.totalSizeBytes / 1024).toFixed(1);
				document.getElementById('size').textContent = sizeKB + ' KB';

				var usedMB = (data.redisUsedBytes / (1024 * 1024)).toFixed(1);
				var maxMB = (data.redisMaxBytes / (1024 * 1024)).toFixed(1);
				var pct = Math.min((data.redisUsedBytes / data.redisMaxBytes) * 100, 100);
				
				document.getElementById('redis-size-text').textContent = pct.toFixed(1) + '% (' + usedMB + ' MB / ' + maxMB + ' MB)';
				document.getElementById('redis-size-bar').style.width = pct + '%';
				document.getElementById('redis-size-bar').style.background = pct > 90 ? '#f44747' : pct > 75 ? '#cca700' : '#4ec9b0';

				document.getElementById('keyCount').textContent = data.keyCount;
				document.getElementById('session').textContent = '#' + data.sessionNumber;
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
				
				document.getElementById('lastUpdated').textContent = data.lastUpdated === 'Never' ? 'Never' : timeAgo(data.lastUpdated);
				document.getElementById('currentTask').textContent = data.currentTask;

				var optInit = document.getElementById('opt-init');
				if (optInit) {
					if (data.isInitialized) {
						optInit.disabled = true;
						optInit.textContent = 'Initialize Brain (Done)';
					} else {
						optInit.disabled = false;
						optInit.textContent = 'Initialize Brain';
					}
				}

				var catHtml = '';
				var cats = data.categories;
				for (var name in cats) {
					var info = cats[name];
					var kb = (info.size / 1024).toFixed(1);
					catHtml += '<div class="category"><div class="category-name">' + name + ' <span>' + kb + ' KB</span></div><div style="margin-top:3px;color:var(--vscode-descriptionForeground);font-size:11px;">' + info.keys.join(', ') + '</div></div>';
				}
				document.getElementById('categories').innerHTML = catHtml || '<span style="color:var(--vscode-descriptionForeground)">No data</span>';

				if (data.recommendations.length > 0) {
					var warnHtml = '';
					for (var i = 0; i < data.recommendations.length; i++) {
						warnHtml += '<div class="warning">\u26A0\uFE0F ' + data.recommendations[i] + '</div>';
					}
					document.getElementById('warnings').innerHTML = warnHtml;
				} else {
					document.getElementById('warnings').innerHTML = '<span style="color:#4ec9b0">\u2705 All clear</span>';
				}

				document.getElementById('error-banner').style.display = 'none';
			}

			if (command === 'error') {
				document.getElementById('error-banner').innerHTML = data;
				document.getElementById('error-banner').style.display = 'block';
			}

			if (command === 'healthReport') {
				var w = document.getElementById('warnings');
				if (data.recommendations.length > 0) {
					var html = '';
					for (var i = 0; i < data.recommendations.length; i++) {
						html += '<div class="warning">\u26A0\uFE0F ' + data.recommendations[i] + '</div>';
					}
					w.innerHTML = html;
				} else {
					w.innerHTML = '<span style="color:#4ec9b0">\u2705 All checks passed</span>';
				}
			}

			if (command === 'conflicts') {
				var w2 = document.getElementById('warnings');
				if (data.length > 0) {
					var cHtml = '';
					for (var i = 0; i < data.length; i++) {
						cHtml += '<div class="warning">\u26A1 ' + data[i].recommendation + '</div>';
					}
					w2.innerHTML = cHtml;
				} else {
					w2.innerHTML += '<div style="color:#4ec9b0;margin-top:4px">\u2705 No conflicts</div>';
				}
			}
		});

		setInterval(function() { send('refresh'); }, 30000);
	</script>
</body>
</html>`;
	}
}