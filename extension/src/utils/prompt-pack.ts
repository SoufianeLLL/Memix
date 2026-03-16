import { BRAIN_KEYS, BRAIN_KEY_SPECS } from './constants';

export type PromptPackVariant = 'Small' | 'Standard' | 'Deep';

const OBSERVER_ENTRY_IDS = {
	DNA: 'observer_dna.json',
	INTENT: 'observer_intent.json',
	CHANGES: 'observer_changes.json',
	GIT: 'observer_git.json'
} as const;

interface PromptPackSection {
	title: string;
	content: string;
}

export interface PromptPackResult {
	text: string;
	requestedSectionCount: number;
	availableSectionCount: number;
	missingSections: string[];
	observerSectionCount: number;
	includedSectionTitles: string[];
}

function prettyJson(value: unknown) {
	return typeof value === 'string' ? value : JSON.stringify(value, null, 2);
}

function coreKeysForVariant(variant: PromptPackVariant) {
	if (variant === 'Small') {
		return [BRAIN_KEYS.IDENTITY, BRAIN_KEYS.SESSION_STATE, BRAIN_KEYS.PATTERNS];
	}
	if (variant === 'Deep') {
		return [
			BRAIN_KEYS.IDENTITY,
			BRAIN_KEYS.SESSION_STATE,
			BRAIN_KEYS.PATTERNS,
			BRAIN_KEYS.DECISIONS,
			BRAIN_KEYS.KNOWN_ISSUES,
			BRAIN_KEYS.TASKS,
			BRAIN_KEYS.FILE_MAP,
			BRAIN_KEYS.SESSION_LOG
		];
	}
	return [
		BRAIN_KEYS.IDENTITY,
		BRAIN_KEYS.SESSION_STATE,
		BRAIN_KEYS.PATTERNS,
		BRAIN_KEYS.DECISIONS,
		BRAIN_KEYS.KNOWN_ISSUES,
		BRAIN_KEYS.TASKS,
		BRAIN_KEYS.FILE_MAP
	];
}

function observerKeysForVariant(variant: PromptPackVariant) {
	if (variant === 'Small') {
		return [OBSERVER_ENTRY_IDS.INTENT] as string[];
	}
	if (variant === 'Deep') {
		return [
			OBSERVER_ENTRY_IDS.INTENT,
			OBSERVER_ENTRY_IDS.DNA,
			OBSERVER_ENTRY_IDS.CHANGES,
			OBSERVER_ENTRY_IDS.GIT
		] as string[];
	}
	return [
		OBSERVER_ENTRY_IDS.INTENT,
		OBSERVER_ENTRY_IDS.DNA,
		OBSERVER_ENTRY_IDS.CHANGES
	] as string[];
}

function coreLabel(key: string) {
	return BRAIN_KEY_SPECS[key]?.label || key;
}

function summarizeObserverIntent(value: any) {
	const lines = [
		`active_file: ${value?.active_file || '—'}`,
		`intent_type: ${value?.intent_type || 'unknown'}`,
		`confidence: ${typeof value?.confidence === 'number' ? Math.round(value.confidence * 100) + '%' : '—'}`
	];
	const related = Array.isArray(value?.related_files) ? value.related_files.slice(0, 8) : [];
	if (related.length > 0) {
		lines.push(`related_files: ${related.join(', ')}`);
	}
	const rationale = Array.isArray(value?.rationale) ? value.rationale.slice(0, 5) : [];
	if (rationale.length > 0) {
		lines.push('', 'rationale:');
		for (const item of rationale) {
			lines.push(`- ${item}`);
		}
	}
	return lines.join('\n');
}

function summarizeObserverDna(value: any) {
	const lines = [
		`architecture: ${value?.architecture || 'unknown'}`,
		`indexed_files: ${value?.indexed_files ?? '—'}`,
		`functions_indexed: ${value?.functions_indexed ?? '—'}`,
		`complexity_score: ${value?.complexity_score ?? '—'}`
	];
	const patterns = Array.isArray(value?.dominant_patterns) ? value.dominant_patterns.slice(0, 8) : [];
	if (patterns.length > 0) {
		lines.push(`dominant_patterns: ${patterns.join(', ')}`);
	}
	const hotZones = Array.isArray(value?.hot_zones) ? value.hot_zones.slice(0, 6) : [];
	if (hotZones.length > 0) {
		lines.push(`hot_zones: ${hotZones.join(', ')}`);
	}
	if (typeof value?.explainability_summary === 'string' && value.explainability_summary.trim()) {
		lines.push('', value.explainability_summary.trim());
	}
	return lines.join('\n');
}

function summarizeObserverChanges(value: any) {
	const items = Array.isArray(value) ? value : [];
	if (items.length === 0) {
		return 'No recent semantic changes recorded.';
	}
	return items.slice(0, 10).map((item: any, index: number) => {
		const file = item?.file || 'unknown-file';
		const added = Array.isArray(item?.nodes_added) ? item.nodes_added.length : 0;
		const removed = Array.isArray(item?.nodes_removed) ? item.nodes_removed.length : 0;
		const modified = Array.isArray(item?.nodes_modified) ? item.nodes_modified.length : 0;
		return `${index + 1}. ${file} — added ${added}, removed ${removed}, modified ${modified}`;
	}).join('\n');
}

function summarizeObserverGit(value: any) {
	const lines = [
		`repo_root: ${value?.repo_root || '—'}`,
		`available: ${value?.available ? 'yes' : 'no'}`
	];
	const summary = Array.isArray(value?.summary) ? value.summary.slice(0, 6) : [];
	if (summary.length > 0) {
		lines.push('', ...summary.map((item: string) => `- ${item}`));
	}
	const hotFiles = Array.isArray(value?.hot_files) ? value.hot_files.slice(0, 5) : [];
	if (hotFiles.length > 0) {
		lines.push('', 'hot_files:');
		for (const file of hotFiles) {
			lines.push(`- ${file.file_path} (${file.churn_commits || 0} commits)`);
		}
	}
	return lines.join('\n');
}

function observerSectionTitle(key: string) {
	switch (key) {
		case OBSERVER_ENTRY_IDS.INTENT:
			return 'Observer Intent';
		case OBSERVER_ENTRY_IDS.DNA:
			return 'Observer DNA Summary';
		case OBSERVER_ENTRY_IDS.CHANGES:
			return 'Recent Code Changes';
		case OBSERVER_ENTRY_IDS.GIT:
			return 'Git Archaeology';
		default:
			return key;
	}
}

function observerSectionContent(key: string, value: any) {
	switch (key) {
		case OBSERVER_ENTRY_IDS.INTENT:
			return summarizeObserverIntent(value);
		case OBSERVER_ENTRY_IDS.DNA:
			return summarizeObserverDna(value);
		case OBSERVER_ENTRY_IDS.CHANGES:
			return summarizeObserverChanges(value);
		case OBSERVER_ENTRY_IDS.GIT:
			return summarizeObserverGit(value);
		default:
			return prettyJson(value);
	}
}

export function createPromptPack(allData: Record<string, any>, variant: PromptPackVariant): PromptPackResult {
	const requestedCoreKeys = coreKeysForVariant(variant);
	const requestedCoreLabels = requestedCoreKeys.map(coreLabel);
	const availableCoreKeys = requestedCoreKeys.filter((key) => Object.prototype.hasOwnProperty.call(allData, key));
	const missingSections = requestedCoreKeys
		.filter((key) => !Object.prototype.hasOwnProperty.call(allData, key))
		.map(coreLabel);

	const observerSections: PromptPackSection[] = [];
	for (const key of observerKeysForVariant(variant)) {
		if (!Object.prototype.hasOwnProperty.call(allData, key)) {
			continue;
		}
		observerSections.push({
			title: observerSectionTitle(key),
			content: observerSectionContent(key, allData[key])
		});
	}

	const sections: PromptPackSection[] = [];
	sections.push({
		title: 'Memix Prompt Pack',
		content: [
			`variant: ${variant}`,
			`core_sections_requested: ${requestedCoreLabels.join(', ')}`,
			`core_sections_available: ${availableCoreKeys.map(coreLabel).join(', ') || 'None'}`,
			`core_sections_missing: ${missingSections.join(', ') || 'None'}`,
			`observer_sections_included: ${observerSections.map((section) => section.title).join(', ') || 'None'}`
		].join('\n')
	});

	for (const key of availableCoreKeys) {
		sections.push({
			title: coreLabel(key),
			content: prettyJson(allData[key])
		});
	}
	sections.push(...observerSections);

	const text = sections
		.map((section) => `## ${section.title}\n\n${section.content}`)
		.join('\n\n');

	return {
		text,
		requestedSectionCount: requestedCoreKeys.length,
		availableSectionCount: availableCoreKeys.length,
		missingSections,
		observerSectionCount: observerSections.length,
		includedSectionTitles: sections.map((section) => section.title)
	};
}
