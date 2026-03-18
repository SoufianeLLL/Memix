// run `node scripts/sync-versions.mjs` to update versions.json

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(scriptDir, '..');
const versionsPath = path.join(root, 'versions.json');
const cargoTomlPath = path.join(root, 'daemon', 'Cargo.toml');
const packageJsonPath = path.join(root, 'extension', 'package.json');
const serverRsPath = path.join(root, 'daemon', 'src', 'server.rs');

const versions = JSON.parse(fs.readFileSync(versionsPath, 'utf8'));

if (!versions.daemonVersion || !versions.extensionVersion) {
	throw new Error('versions.json must contain daemonVersion and extensionVersion');
}

const cargoToml = fs.readFileSync(cargoTomlPath, 'utf8');
const serverRs = fs.readFileSync(serverRsPath, 'utf8');
const nextCargoToml = cargoToml.replace(
	/^version = ".*"$/m,
	`version = "${versions.daemonVersion}"`,
);
const nextServerRs = serverRs.replace(
	/^\s*"version": \s*".*",$/m,
	`        "version": "${versions.daemonVersion}",`
);

if (cargoToml === nextCargoToml) {
	console.log(`daemon/Cargo.toml already at ${versions.daemonVersion}`);
} else {
	fs.writeFileSync(cargoTomlPath, nextCargoToml);
	console.log(`Updated daemon/Cargo.toml to ${versions.daemonVersion}`);
}

if (serverRs === nextServerRs) {
	console.log(`daemon/src/server.rs already at ${versions.daemonVersion}`);
} else {
	fs.writeFileSync(serverRsPath, nextServerRs);
	console.log(`Updated daemon/src/server.rs to ${versions.daemonVersion}`);
}

const brainTsPath = path.join(root, 'extension', 'src', 'core', 'brain.ts');
const exporterTsPath = path.join(root, 'extension', 'src', 'utils', 'exporter.ts');

const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, 'utf8'));
if (packageJson.version === versions.extensionVersion) {
	console.log(`extension/package.json already at ${versions.extensionVersion}`);
} else {
	packageJson.version = versions.extensionVersion;
	fs.writeFileSync(packageJsonPath, `${JSON.stringify(packageJson, null, '\t')}\n`);
	console.log(`Updated extension/package.json to ${versions.extensionVersion}`);
}

const brainTs = fs.readFileSync(brainTsPath, 'utf8');
const nextBrainTs = brainTs.replace(
	/^\s*brainVersion:\s*'.*',$/m,
	`            brainVersion: '${versions.extensionVersion}',`
);
if (brainTs === nextBrainTs) {
	console.log(`extension/src/core/brain.ts already at ${versions.extensionVersion}`);
} else {
	fs.writeFileSync(brainTsPath, nextBrainTs);
	console.log(`Updated extension/src/core/brain.ts to ${versions.extensionVersion}`);
}

const exporterTs = fs.readFileSync(exporterTsPath, 'utf8');
const nextExporterTs = exporterTs.replace(
	/^\s*memix_version:\s*'.*',$/m,
	`        memix_version: '${versions.extensionVersion}',`
);
if (exporterTs === nextExporterTs) {
	console.log(`extension/src/utils/exporter.ts already at ${versions.extensionVersion}`);
} else {
	fs.writeFileSync(exporterTsPath, nextExporterTs);
	console.log(`Updated extension/src/utils/exporter.ts to ${versions.extensionVersion}`);
}
