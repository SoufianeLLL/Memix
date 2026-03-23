document.addEventListener('click', function(e) {
    var target = e.target;
    while (target && target !== document.body) {
        if (target.classList && target.classList.contains('brain-key-link')) {
            e.preventDefault();
            var keyName = target.getAttribute('data-key');
            if (keyName) vscode.postMessage({ command: 'openBrainKey', key: keyName });
            return;
        }
        target = target.parentNode;
    }
});

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
				if (ppm) ppm.textContent = 'Tokens: Recalculating...';
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

	var btnChangeRedis = byId('btn-change-redis');
	if (btnChangeRedis) {
		btnChangeRedis.addEventListener('click', function() {
			send('connectRedis', { showSpinner: true });
		});
	}

	var btnVersionInfo = byId('btn-version-info');
	if (btnVersionInfo) {
		btnVersionInfo.addEventListener('click', function() {
			vscode.postMessage({ command: 'showVersionInfo' });
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

var btnScanPatterns = byId('btn-scan-patterns');
if (btnScanPatterns) {
	btnScanPatterns.addEventListener('click', function () {
		send('scanPatterns', { showSpinner: false });
	});
}

// Define at module level, updated when workspaceRoot arrives
var workspaceRootForPaths = '';

function stripRoot(p) {
    if (!workspaceRootForPaths || typeof p !== 'string') { return p || '—'; }
    if (p.startsWith(workspaceRootForPaths)) {
        return p.slice(workspaceRootForPaths.length).replace(/^[/\\]/, '');
    }
    return p;
}

window.addEventListener('message', function (e) {
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

	if (command === 'showBlastRadius') {
		var modalKind = 'blast_radius';
		var modalTitle = 'Blast Radius Warning';
		var modalSubtitle = 'This change has a substantial impact on your project.';

		var listHtml = (data.affected || []).slice(0, 50).map(function(f) { 
			var shortVia = f.via ? f.via.split('/').pop() : 'Direct';
			return `<li>
				<span class="file-link" style="color:var(--vscode-textLink-foreground); cursor:pointer;" onclick="vscode.postMessage({command: \\'openPath\\', path: \\'' + f.path.replace(/\\'/g, "\\\\'") + '\\'})">` + f.path + `</span> 
				<span style="opacity:0.6">(via ` + shortVia + `, depth ` + f.depth + `)</span>
			</li>`;
		}).join('');
		
		if (data.affected && data.affected.length > 50) {
			listHtml += '<li>...and ' + (data.affected.length - 50) + ' more</li>';
		}

		var htmlStr = '<div style="margin-bottom: 12px;"><strong>Affected files:</strong> ' + (data.affected_count || 0) + '</div>' + 
						'<div style="margin-bottom: 12px;"><strong>Max recursion depth:</strong> ' + (data.max_depth || 0) + '</div>' + 
						'<ul style="margin:0; padding-left: 20px; max-height: 250px; overflow-y: auto; font-family: monospace; font-size: 11px;">' + 
						listHtml + '</ul>';

		activeModalKind = modalKind;
		activeModalPayload = JSON.stringify(data, null, 2);

		var backdrop = byId('payload-modal-backdrop');
		var titleEl = byId('payload-modal-title');
		var subtitleEl = byId('payload-modal-subtitle');
		var bodyEl = byId('payload-modal-body');

		if (titleEl) titleEl.textContent = modalTitle;
		if (subtitleEl) subtitleEl.textContent = modalSubtitle;
		if (bodyEl) bodyEl.innerHTML = htmlStr;
		if (backdrop) backdrop.hidden = false;

		if (spinnerActive) hideLoading();
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
		// Update the shared path-stripping root whenever we get a refresh
		if (data.workspaceRoot && typeof data.workspaceRoot === 'string') {
			workspaceRootForPaths = data.workspaceRoot;
		}

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
				missingEl.textContent = 'All required keys present';
				missingEl.style.color = '#4ec9b0';
				if (fixBtn) fixBtn.hidden = true;
			} else {
				requiredEl.textContent = 'Missing ' + missing.length;
				missingEl.textContent = 'Missing required keys: ' + missing.map(escapeHtml).join(', ');
				missingEl.style.color = '#f44747';
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
			catHtml += '<div class="w-full py-0.5"><div class="stat">' + name + ' <span>' + kb + ' KB</span></div><div class="text-[11px] opacity-50">' + info.keys.join(', ') + '</div></div>';
		}
		var categoriesEl = byId('categories');
		if (categoriesEl) categoriesEl.innerHTML = catHtml || '<span style="color:var(--vscode-descriptionForeground)">No data</span>';

		var keySizesHtml = '';
		var keys = data.keys || {};
		var sorted = Object.keys(keys).sort(function(a, b) { return (keys[b] || 0) - (keys[a] || 0); }).slice(0, 10);
		for (var i = 0; i < sorted.length; i++) {
			var k = sorted[i];
			var bytes = keys[k] || 0;
			keySizesHtml += '<div class="stat"><a class="brain-key-link hover:text-sky-600 transition-all" data-key="' + escapeHtml(k) + '" href="#" title="Open ' + escapeHtml(k) + '.json" style="max-width:70%;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">' + escapeHtml(k) + '</a><span>' + (bytes / 1024).toFixed(1) + ' KB</span></div>';
		}
		var keySizesEl = byId('key-sizes');
		if (keySizesEl) {
			keySizesEl.innerHTML = keySizesHtml || '<span style="color:var(--vscode-descriptionForeground)">No data</span>';
		}

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

		// Token Intelligence rendering
		if (data.tokenStats) {
			var ts = data.tokenStats;
			var session = ts.session || {};
			var lifetime = ts.lifetime || {};
			
			var tsa = byId('token-session-ai');
			if (tsa) tsa.textContent = (session.ai_tokens_consumed || 0).toLocaleString();
			
			var tsc = byId('token-session-compiled');
			if (tsc) tsc.textContent = (session.context_tokens_compiled || 0).toLocaleString();
			
			var tss = byId('token-session-saved');
			if (tss) tss.textContent = (session.estimated_tokens_saved || 0).toLocaleString();
			
			var tsf = byId('token-session-files');
			if (tsf) tsf.textContent = (session.files_indexed || 0).toLocaleString();
			
			var tscomp = byId('token-session-compilations');
			if (tscomp) tscomp.textContent = (session.context_compilations || 0).toLocaleString();
			
			var tce = byId('token-cache-efficiency');
			if (tce) {
				var ce = typeof ts.cache_efficiency_pct === 'number' ? ts.cache_efficiency_pct : 0;
				tce.textContent = ce.toFixed(1) + '%';
			}
			
			var tcr = byId('token-compression-ratio');
			if (tcr) {
				var cr = typeof ts.compression_ratio === 'number' ? ts.compression_ratio : 1.0;
				tcr.textContent = cr.toFixed(2) + 'x';
			}
			
			var tla = byId('token-lifetime-ai');
			if (tla) tla.textContent = (lifetime.total_ai_tokens_consumed || 0).toLocaleString();
			
			var tls = byId('token-lifetime-saved');
			if (tls) tls.textContent = (lifetime.total_tokens_saved || 0).toLocaleString();
			
			var tlss = byId('token-lifetime-sessions');
			if (tlss) tlss.textContent = (lifetime.sessions_recorded || 0).toLocaleString();
		}

		var ideEl = byId('ide');
		if (ideEl) ideEl.textContent = (data.ide || '').toUpperCase();
		var rulesPathEl = byId('rules-path');
		if (rulesPathEl) {
			rulesPathEl.textContent = (data.rulesDir || '') + '/' + (data.rulesFile || '');
			rulesPathEl.title = (data.rulesDir || '') + '/' + (data.rulesFile || '');
		}

		if (Array.isArray(data.keyCoverage)) {
			var rows = data.keyCoverage.map(function(r) {
				var status = '<div class="status-subtle">Optional</div>';
				if (r.exists) {
					status = '<div class="status-subtle">Ready</div>';
				} else if (r.state === 'missing_required') {
					status = '<div class="status-danger">Needs initialization</div>';
				} else if (r.state === 'missing_recommended') {
					status = '<div class="status-subtle">Recommended</div>';
				} else if (r.state === 'not_generated') {
					status = '<div class="status-subtle">Generated later</div>';
				}
				var label = escapeHtml(r.label || r.key);
				var tooltip = r.description ? ' data-tooltip="' + escapeHtml(r.description) + '" aria-label="' + escapeHtml(r.description) + '" tabindex="0"' : '';
				var icon = r.description ? '<span class="ml-2 info-icon"' + tooltip + '><svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M13.25 7c0 .69-.56 1.25-1.25 1.25s-1.25-.56-1.25-1.25.56-1.25 1.25-1.25 1.25.56 1.25 1.25zm10.75 5c0 6.627-5.373 12-12 12s-12-5.373-12-12 5.373-12 12-12 12 5.373 12 12zm-2 0c0-5.514-4.486-10-10-10s-10 4.486-10 10 4.486 10 10 10 10-4.486 10-10zm-13-2v2h2v6h2v-8h-4z"/></svg></span>' : '';
				return '<div class="stat"><span style="max-width:70%;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">' + label + icon + '</span><span>' + status + '</span></div>';
			}).join('');
			var kc = byId('key-coverage');
			if (kc) kc.innerHTML = rows || '<div class="stat"><span>—</span><span>—</span></div>';
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
			var tok = (typeof data.promptPackTokens === 'number') ? (data.promptPackTokens + ' tokens') : 'Tokens: —';
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
			slPrev.innerHTML = '<span style="color:var(--vscode-descriptionForeground)">No entries</span>';
		} else {
			var pHtml = '';
			for (var pi = 0; pi < prev.length; pi++) {
				var dt = prev[pi].date || '';
				var sm = prev[pi].summary || '';
				pHtml += '<div class="mb-2"><div class="opacity-50">' + dt + '</div><div class="text-sm">' + sm + '</div></div>';
			}
			slPrev.innerHTML = pHtml;
		}

		function formatRelativeTime(timestamp) {
			const date = new Date(timestamp);
			const now = new Date();
			
			const diffMs = now.getTime() - date.getTime();
			const diffSecs = Math.floor(diffMs / 1000);
			const diffMins = Math.floor(diffSecs / 60);
			const diffHours = Math.floor(diffMins / 60);
			const diffDays = Math.floor(diffHours / 24);
			
			// Format time as HH:MM AM/PM
			const formatTime = (d) => {
				let hours = d.getHours();
				const minutes = d.getMinutes();
				const ampm = hours >= 12 ? 'PM' : 'AM';
				hours = hours % 12;
				hours = hours ? hours : 12; // 12-hour format
				const minutesStr = minutes < 10 ? '0' + minutes : minutes;
				return `${hours}:${minutesStr} ${ampm}`;
			};
			
			// Format date as Month Day
			const formatDate = (d) => {
				return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
			};
			
			if (diffDays === 0) {
				// Today
				return `Today ~ ${formatTime(date)}`;
			} else if (diffDays === 1) {
				// Yesterday
				return `Yesterday ~ ${formatTime(date)}`;
			} else if (diffDays < 7) {
				// Within a week: "Monday ~ 3:45 PM"
				const days = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'];
				return `${days[date.getDay()]} ~ ${formatTime(date)}`;
			} else if (diffDays < 14) {
				// 1 week ago
				return `1 week ago ~ ${formatDate(date)}`;
			} else if (diffDays < 21) {
				// 2 weeks ago
				return `2 weeks ago ~ ${formatDate(date)}`;
			} else if (diffDays < 28) {
				// 3 weeks ago
				return `3 weeks ago ~ ${formatDate(date)}`;
			} else if (diffDays < 60) {
				// X weeks ago
				const weeks = Math.floor(diffDays / 7);
				return `${weeks} weeks ago ~ ${formatDate(date)}`;
			} else if (diffDays < 365) {
				// X months ago
				const months = Math.floor(diffDays / 30);
				return `${months} month${months > 1 ? 's' : ''} ago ~ ${formatDate(date)}`;
			} else {
				// More than a year
				const years = Math.floor(diffDays / 365);
				return `${years} year${years > 1 ? 's' : ''} ago ~ ${formatDate(date)}`;
			}
		}

		var stCount = byId('session-timeline-count');
		if (stCount) stCount.textContent = data.sessionTimelineCount || 0;
		var stPrev = byId('session-timeline-preview');
		if (stPrev) {
			var timeline = data.sessionTimelinePreview || [];
			if (timeline.length === 0) {
				stPrev.innerHTML = '<span style="color:var(--vscode-descriptionForeground)">No events</span>';
			} else {
				var tHtml = '';
				for (var ti = 0; ti < timeline.length; ti++) {
					var tts = timeline[ti].timestamp || '';
					var formattedTime = formatRelativeTime(tts);
					var tev = timeline[ti].event || '';
					tHtml += '<div class="mb-2"><div class="text-[11px] opacity-50">' + formattedTime + '</div><div>' + tev + '</div></div>';
				}
				stPrev.innerHTML = tHtml;
			}
		}

		var dna = data.observerDna || null;
		var dnaArch = byId('observer-dna-architecture');
		if (dnaArch) {
			var archLabels = {
				// Application Types
				'component-driven':                     'Component-Driven UI',
				'component-driven/application-layered': 'Component-Driven + Layered',
				'application-layered':                  'Application Layered',
				'layered-architecture':                 'Layered Architecture',
				'monolith':                             'Monolith',
				'modular-monolith':                     'Modular Monolith',
				'microservices':                        'Microservices',
				'serverless':                           'Serverless / FaaS',
				'jamstack':                             'Jamstack',
				'spa':                                  'Single Page Application',
				'mpa':                                  'Multi Page Application',
				'ssr':                                  'Server-Side Rendered',
				'ssg':                                  'Static Site Generated',
				'hybrid-rendering':                     'Hybrid Rendering (SSR + SSG + Client)',
				'islands':                              'Islands Architecture',
				'api-server':                           'API Server',
				'api-gateway':                          'API Gateway',
				'bff':                                  'Backend For Frontend',

				// Structural
				'clean-architecture':                   'Clean Architecture',
				'hexagonal':                            'Hexagonal / Ports & Adapters',
				'onion':                                'Onion Architecture',
				'cqrs':                                 'CQRS',
				'event-driven':                         'Event-Driven',
				'event-sourcing':                       'Event Sourcing',
				'plugin-architecture':                  'Plugin Architecture',
				'pipe-and-filter':                      'Pipe & Filter',
				'ddd':                                  'Domain-Driven Design',

				// Project Types
				'library':                              'Library / Package',
				'cli':                                  'CLI Tool',
				'sdk':                                  'SDK',
				'monorepo':                             'Monorepo',
				'data-pipeline':                        'Data Pipeline',
				'worker':                               'Background Worker / Job Processor',
				'bot':                                  'Bot / Automation',
				'extension':                            'Editor Extension / Plugin',
				'mobile':                               'Mobile App',
				'desktop':                              'Desktop App',
				'embedded':                             'Embedded / IoT',
				'game':                                 'Game',

				// Fallback
				'unknown':                              'Unknown',
				'mixed':                                'Mixed / Transitional',
			};
			const raw = dna && dna.architecture ? dna.architecture : '';
			dnaArch.textContent = raw ? (archLabels[raw] ?? raw.replace(/-/g, ' ')) : '—';
		}
		var dnaComplexity = byId('observer-dna-complexity');
		if (dnaComplexity) {
			dnaComplexity.textContent = dna && typeof dna.complexity_score === 'number'
				? Math.round(dna.complexity_score * 100) + '%'
				: '—';
		}
		var dnaFiles = byId('observer-dna-files');
		if (dnaFiles) dnaFiles.textContent = dna && typeof dna.indexed_files === 'number' ? dna.indexed_files : '—';
		var dnaSymbols = byId('observer-dna-symbols');
		if (dnaSymbols) dnaSymbols.textContent = dna && typeof dna.functions_indexed === 'number' ? dna.functions_indexed : '—';
		var dnaDepth = byId('observer-dna-depth');
		if (dnaDepth) dnaDepth.textContent = dna && typeof dna.dependency_depth === 'number' ? dna.dependency_depth : '—';
		var dnaTyped = byId('observer-dna-typed');
		if (dnaTyped) dnaTyped.textContent = dna && typeof dna.type_coverage === 'number' ? Math.round(dna.type_coverage * 100) + '%' : '—';
		
		var dnaExplain = byId('observer-dna-explainability');
		if (dnaExplain) {
			if (dna && typeof dna.indexed_files === 'number') {
				var explanation = typeof dna.explainability_summary === 'string' ? dna.explainability_summary : '';
				dnaExplain.innerHTML = explanation ? '<div><div class="w-full mb-1 font-bold">Explainability:</div><div>' + explanation + '</div></div>' : '';
			} else {
				dnaExplain.textContent = 'No DNA snapshot';
			}
		}

		// Languages — rendered from DNA auto-refresh on every panel update
		var dnaLang = byId('observer-dna-languages');
		if (dnaLang) {
			var languageBreakdown = dna && dna.language_breakdown ? dna.language_breakdown : null;
			var langEntries = languageBreakdown
				? Object.entries(languageBreakdown).sort(function(a, b) { return b[1] - a[1]; })
				: [];
			if (langEntries.length > 0) {
				var langHtml = '<div class="w-full">'
					+ '<div class="text-sm mb-1">Languages:</div>'
					+ langEntries.map(function(entry) {
						var name = escapeHtml(entry[0].charAt(0).toUpperCase() + entry[0].slice(1));
						var count = entry[1];
						return '<div class="stat">'
							+ '<span>' + name + '</span>'
							+ '<span>' + count + ' file' + (count !== 1 ? 's' : '') + '</span>'
							+ '</div>';
					}).join('')
					+ '</div>';
				dnaLang.innerHTML = langHtml;
			} else {
				dnaLang.innerHTML = '';
			}
		}

		// Rules source — only shown when non-default rules are applied
		var dnaRules = byId('observer-dna-rules');
		if (dnaRules) {
			var ruleSource = dna && dna.rules_source ? dna.rules_source : '';
			// "built-in defaults" is the common case — no need to surface it
			if (ruleSource && ruleSource !== 'built-in defaults') {
				dnaRules.innerHTML = '<div class="w-full">'
					+ '<div class="text-sm mb-1">Rules source:</div>' + escapeHtml(ruleSource) + '</div>';
			} else {
				dnaRules.innerHTML = '';
			}
		}
		
		var dnaHot = byId('observer-dna-hot-zones');
		if (dnaHot) {
			var hotZones = dna && Array.isArray(dna.hot_zones) ? dna.hot_zones : [];
			var circular = dna && Array.isArray(dna.circular_risks) ? dna.circular_risks : [];
			var hotHtml = hotZones.length > 0
				? '<div class="w-full"><div class="w-full text-sm">Hot zones:</div><div class="w-full space-y-0.5">' + hotZones.slice(0, 4).map(stripRoot).map(file => '<div>— ' + file + '</div>') + '</div></div>'
				: '<div style="color:var(--vscode-descriptionForeground)">No hot zones detected</div>';
			if (circular.length > 0) {
				hotHtml += '<div class="w-full"><div class="w-full text-sm">Circular risks:</div><div class="w-full space-y-0.5">' + circular.slice(0, 3).map(stripRoot).map(file => '<div>— ' + file + '</div>') + '</div></div>';
			}
			dnaHot.innerHTML = hotHtml;
		}
		var dnaStable = byId('observer-dna-stable-zones');
		if (dnaStable) {
			var stableZones = dna && Array.isArray(dna.stable_zones) ? dna.stable_zones : [];
			dnaStable.innerHTML = stableZones.length > 0
				? '<div class="w-full"><div class="w-full text-sm">Stable zones:</div><div class="w-full space-y-0.5">' + stableZones.slice(0, 4).map(stripRoot).map(file => '<div>— ' + file + '</div>') + '</div></div>'
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
				? '<div class="w-full"><span>Related files</span> (' + tokenWeight + ' tokens)</div><div class="w-full space-y-0.5">' + related.slice(0, 4).map(stripRoot).map(file => '<div>• ' + file + '</div>').join('') + '</div>'
				: 'No predictive snapshot';
		}

		var intentRationale = byId('observer-intent-rationale');
		if (intentRationale) {
			var r = intent && intent.rationale && typeof intent.rationale === 'object'
				? intent.rationale
				: null;

			if (r && !Array.isArray(r)) {
				// Structured object from Rust
				var rows = [
					['Intent',        r.intent || '—'],
					['Confidence',    r.confidence || '—'],
					['Related Files', r.related_files ?? '—'],
					['Nodes Changed', r.nodes_changed ?? '—'],
				];
				var tableHtml = '';
				rows.forEach(function(row) {
					tableHtml += '<div class="stat">'
						+ '<span>' + row[0] + '</span>'
						+ '<span>' + row[1] + '</span>'
					+ '</div>';
				});
				intentRationale.innerHTML = '<div class="w-full">'
					+ '<div class="w-full mb-1">Rationale</div>'
					+ tableHtml + '</div>';
			} else if (Array.isArray(r) && r.length > 0) {
				// Legacy format fallback (string array)
				var legacyRows = [];
				r.forEach(function(item) {
					var parts = String(item).split(',');
					parts.forEach(function(part) {
						var kv = part.split('=');
						if (kv.length === 2) {
							var label = kv[0].trim().replace(/_/g, ' ').replace(/\b\w/g, function(c) {
								return c.toUpperCase();
							});
							legacyRows.push([label, kv[1].trim()]);
						}
					});
				});
				if (legacyRows.length > 0) {
					var tableHtml2 = '';
					legacyRows.forEach(function(row) {
						tableHtml2 += '<div class="stat">'
							+ '<span>' + row[0] + '</span>'
							+ '<span>' + row[1] + '</span>'
						+ '</div>';
					});
					intentRationale.innerHTML = '<div class="w-full">'
						+ '<div class="w-full mb-1">Rationale</div>'
						+ tableHtml2 + '</div>';
				} else {
					intentRationale.innerHTML = '<div style="color:var(--vscode-descriptionForeground)">Rationale unavailable</div>';
				}
			} else {
				intentRationale.innerHTML = '<div style="color:var(--vscode-descriptionForeground)">Rationale unavailable</div>';
			}
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

		var importance = data.importanceData || null;
		var importanceSummary = byId('importance-summary');
		if (importanceSummary) {
			if (importance && typeof importance.node_count === 'number') {
				importanceSummary.innerHTML =
					'<div class="stat"><span>Nodes</span><span>' + importance.node_count + '</span></div>' +
					'<div class="stat"><span>Cycles</span><span>' + (importance.cycle_count || 0) + '</span></div>' +
					'<div class="stat"><span>Topo Order</span><span>' + (importance.topological_order_length || 0) + '</span></div>';
			} else {
				importanceSummary.textContent = 'No structural graph data';
			}
		}
		var importanceTopFiles = byId('importance-top-files');
		if (importanceTopFiles) {
			var topFiles = importance && Array.isArray(importance.top_files) ? importance.top_files : [];
			var sccGroups = importance && Array.isArray(importance.scc_groups) ? importance.scc_groups : [];
			if (topFiles.length === 0) {
				importanceTopFiles.innerHTML = '<div style="color:var(--vscode-descriptionForeground)">No load-bearing files yet</div>';
			} else {
				var ixHtml = '<div><b>Load-bearing files</b></div>' + topFiles.slice(0, 5).map(function(item) {
					var score = Array.isArray(item) ? item[1] : 0;
					var file = Array.isArray(item) ? item[0] : 'unknown';
					return '<div class="stat"><span style="max-width:75%;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">' +
						escapeHtml(stripRoot(file)) + '</span><span>' + (Number(score || 0).toFixed(2)) + '</span></div>';
				}).join('');
				if (sccGroups.length > 0) {
					ixHtml += '<div style="margin-top:8px;font-size:11px;line-height:1.4"><b>Circular clusters:</b><br/>' +
						sccGroups.slice(0, 3).map(function(group) { return escapeHtml((group || []).join(' → ')); }).join('<br/>') +
						'</div>';
				}
				importanceTopFiles.innerHTML = ixHtml;
			}
		}

		var blast = data.blastRadius || null;
		var blastSummary = byId('blast-radius-summary');
		if (blastSummary) {
			if (blast && typeof blast.affected_count === 'number') {
				blastSummary.innerHTML =
					'<div>' + escapeHtml(blast.source || data.activeFile || '—') + '</div>' +
					'<div class="stat"><span>Affected files</span><span>' + blast.affected_count + '</span></div>' +
					'<div class="stat"><span>Depth</span><span>' + (blast.max_depth || 0) + '</span></div>';
			} else {
				blastSummary.textContent = 'No blast radius available';
			}
		}
		var blastDetails = byId('blast-radius-details');
		if (blastDetails) {
			var criticalPath = blast && Array.isArray(blast.critical_path) ? blast.critical_path : [];
			var affectedFiles = blast && Array.isArray(blast.affected_files) ? blast.affected_files : [];
			if (criticalPath.length === 0 && affectedFiles.length === 0) {
				blastDetails.innerHTML = '<div style="color:var(--vscode-descriptionForeground)">No affected dependents detected</div>';
			} else {
				var blastHtml = '';
				if (criticalPath.length > 0) {
					blastHtml += '<div class="w-full"><div class="w-full text-sm">Critical path:</div><div class="w-full space-y-0.5">' +
						criticalPath.map(escapeHtml).join(' → ') + '</div></div>';
				}
				if (affectedFiles.length > 0) {
					blastHtml += '<div class="w-full"><div class="w-full text-sm">Reach:</div><div class="w-full space-y-0.5">' +
						affectedFiles.slice(0, 5).map(function(entry) {
							return escapeHtml(entry.path || 'unknown') + ' (depth ' + (entry.depth || 0) + ')';
						}).join('<br/>') + '</div></div>';
				}
				blastDetails.innerHTML = blastHtml;
			}
		}

		var causal = data.causalChain || null;
		var causalSummary = byId('causal-chain-summary');
		if (causalSummary) {
			if (causal && Array.isArray(causal.symbols)) {
				causalSummary.innerHTML =
					'<div>' + escapeHtml(causal.file || data.activeFile || '—') + '</div>' +
					'<div class="stat"><span>Symbols</span><span>' + causal.symbols.length + '</span></div>' +
					'<div class="stat"><span>Outgoing</span><span>' + (causal.total_outgoing_edges || 0) + '</span></div>' +
					'<div class="stat"><span>Incoming</span><span>' + (causal.total_incoming_edges || 0) + '</span></div>';
			} else {
				causalSummary.textContent = 'No causal chain available';
			}
		}
		var causalDetails = byId('causal-chain-details');
		if (causalDetails) {
			var symbols = causal && Array.isArray(causal.symbols) ? causal.symbols : [];
			if (symbols.length === 0) {
				causalDetails.innerHTML = '<div style="color:var(--vscode-descriptionForeground)">No resolved symbol-level edges yet</div>';
			} else {
				causalDetails.innerHTML = symbols.slice(0, 4).map(function(symbolEntry) {
					var outgoing = Array.isArray(symbolEntry.calls) ? symbolEntry.calls.slice(0, 3) : [];
					var incoming = Array.isArray(symbolEntry.called_by) ? symbolEntry.called_by.slice(0, 3) : [];
					var outgoingHtml = outgoing.length > 0
						? outgoing.map(function(edge) {
							var target = edge.callee_file
								? escapeHtml(edge.callee_file) + ' :: ' + escapeHtml(edge.callee_symbol || 'unknown')
								: escapeHtml(edge.callee_symbol || 'unknown');
							return target + (edge.callee_line ? ' (line ' + edge.callee_line + ')' : '');
						}).join('<br/>')
						: 'none';
					var incomingHtml = incoming.length > 0
						? incoming.map(function(edge) {
							return escapeHtml(edge.caller_file || 'unknown') + ' :: ' + escapeHtml(edge.caller_symbol || 'unknown') +
								(edge.call_line ? ' (line ' + edge.call_line + ')' : '');
						}).join('<br/>')
						: 'none';
					return '<div style="margin-bottom:10px;font-size:11px;line-height:1.4">' +
						'<b>' + escapeHtml(symbolEntry.symbol || 'unknown') + '</b>' +
						'<div style="margin-top:4px"><b>Calls:</b><br/>' + outgoingHtml + '</div>' +
						'<div style="margin-top:4px"><b>Called by:</b><br/>' + incomingHtml + '</div>' +
						'</div>';
				}).join('');
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
				compiledSummary.innerHTML = '<div>' + (data.activeFile || 'Active file unavailable') + '</div>' +
					'<div class="stat mt-1"><span>Task</span>' + '<span class="capitalize">' + ((data.inferredTaskType || '').replace(/_/g, ' ') || 'unknown') + '</span></div>' + 
					'<div class="stat"><span>Tokens/Budget</span>' + '<span>' + compiled.total_tokens + '/' + compiled.budget + '</span></div>' +
					'<div class="mb-2 mt-1" style="color:var(--vscode-descriptionForeground)">' + (compiled.explainability_summary || '') + '</div>';
			} else {
				compiledSummary.textContent = 'No compiled context';
			}
		}
		var compiledSections = byId('compiled-context-sections');
		if (compiledSections) {
			var sections = compiled && Array.isArray(compiled.selected_sections) ? compiled.selected_sections : [];
			compiledSections.innerHTML = sections.length > 0
				? '<div><b>Selected sections:</b></div>' + sections.slice(0, 5).map(function(section) {
					return '<div class="stat"><span class="capitalize">' + section.kind + '</span>' + '<span>' + section.tokens + ' tokens</span></div>';
				}).join('')
				: '<div style="color:var(--vscode-descriptionForeground)">No selected sections</div>';
		}

		var risk = data.proactiveRisk || null;
		var riskSummary = byId('proactive-risk-summary');
		if (riskSummary) {
			if (risk && typeof risk.risk_score === 'number') {
				var riskPct = Math.round(risk.risk_score * 100);
				riskSummary.innerHTML = '<div>' + risk.file + '</div>' + 
					'<div class="stat"><span>Risk</span>' + '<span>' + riskPct + '%</span></div>' + 
					'<div class="stat"><span>Dependents</span>' + '<span>' + (risk.dependents || 0) + '</span></div>' + 
					'<div class="mt-2" style="color:var(--vscode-descriptionForeground)">' + (risk.recommendation || '') + '</div>';
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
				promptOptEl.innerHTML = '<div class="stat"><span>Task</span>' + '<span>' + ((promptOpt.task_type || '').replace(/_/g, ' ') || (data.inferredTaskType || '').replace(/_/g, ' ') || 'unknown') + '</span></div>' + 
					'<div class="stat"><span>Recommended budget</span>' + '<span>' + promptOpt.recommended_budget + ' tokens</span></div>' + 
					'<div class="mt-2" style="color:var(--vscode-descriptionForeground)">' + (promptOpt.always_include || []).join(', ') + '</div>';
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
					perfLines.push(
					'<div class="w-full">' +
					'<div class="capitalize text-base font-semibold mb-2">' + (model || 'unknown') + '</div>' +
					'<ul class="pl-5">' +
					'<li class="stat"><span>Task</span>' + '<span>' + (taskName || 'unknown') + '</span></li>' +
					'<li class="stat"><span>First-try rate</span>' + '<span>' + Math.round((tp.first_try_rate || 0) * 100) + '%</span></li>' +
					'<li class="stat"><span>Runs</span>' + '<span>' + (tp.runs || 0) + '</span></li>' +
					'<ul>' +
					'</div>');
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
			for (var j = 0; j < Math.min(tasks.length, 10); j++) {
				var title = typeof tasks[j] === 'string' ? tasks[j] : (tasks[j].title || tasks[j].task || 'Unknown task');
				tHtml += '<div class="task-item"><div class="task-bullet">●</div><div>' + title + '</div></div>';
			}
			if (tasks.length > 10) tHtml += '<div class="text-[11px] mt-2" style="color:var(--vscode-descriptionForeground);">+' + (tasks.length - 5) + ' more tasks...</div>';
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
	
	if (command === 'patternReport') {
		var patterns = data.patterns || [];
		var stats = byId('pattern-stats');
		if (stats) {
			if (data.total_files_scanned > 0) {
				stats.textContent = data.total_files_scanned + ' files · '
					+ data.total_functions_analyzed + ' functions · '
					+ data.scan_duration_ms + 'ms';
			} else {
				stats.textContent = '';
			}
		}

		var known    = patterns.filter(function(p) { return p.tier === 'known'; });
		var framework = patterns.filter(function(p) { return p.tier === 'framework'; });
		var emergent  = patterns.filter(function(p) { return p.tier === 'emergent'; });

		// Confidence icon: high (≥0.85) = clean, medium (≥0.60) = warning, low = uncertain
		function confIcon(c) {
			if (c >= 0.85) return '';
			if (c >= 0.60) return ' <span title="Medium confidence">⚠️</span>';
			return ' <span title="Low confidence">❓</span>';
		}

		function renderPatternGroup(elementId, title, icon, items) {
			var el = byId(elementId);
			if (!el) { return; }
			if (items.length === 0) { el.innerHTML = ''; return; }

			var html = '<div style="margin:6px 0 4px 0">'
				+ '<span style="font-size:11px;font-weight:600;opacity:0.9">'
				+ icon + ' ' + escapeHtml(title) + ' (' + items.length + ')'
				+ '</span></div>';

			for (var i = 0; i < items.length; i++) {
				var p = items[i];
				var count = p.occurrences > 1
					? ' <span style="opacity:0.5">(×' + p.occurrences + ')</span>'
					: '';
				var isLast = i === items.length - 1;
				html += '<div style="padding:2px 0 2px 8px;font-size:11px;'
					+ 'border-left:2px solid var(--vscode-panel-border);'
					+ 'margin:2px 0;line-height:1.5">'
					+ (isLast ? '└─ ' : '├─ ')
					+ escapeHtml(p.label) + count + confIcon(p.confidence)
					+ '</div>';
			}

			el.innerHTML = html;
		}

		renderPatternGroup('patterns-known',     'Known Patterns',     '', known);
		renderPatternGroup('patterns-framework', 'Framework Patterns', '', framework);
		renderPatternGroup('patterns-emergent',  'Emergent Patterns',  '', emergent);
	}
});