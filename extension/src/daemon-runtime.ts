import * as vscode from 'vscode';
import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import * as crypto from 'crypto';
import * as http from 'http';
import * as https from 'https';

const DEFAULT_MANIFEST_URL = 'https://api.memix.dev/daemon/manifest.json';
const REQUEST_TIMEOUT_MS = 5000;
const MAX_REDIRECTS = 5;
const MAX_DOWNLOAD_ATTEMPTS = 4;

type PlatformKey = 'darwin-arm64' | 'darwin-x64' | 'linux-x64' | 'linux-arm64' | 'windows-x64';

export type DaemonReadinessKind = 'ready' | 'downloading' | 'updating' | 'error' | 'missing';

export interface DaemonReadinessState {
	kind: DaemonReadinessKind;
	title: string;
	description: string;
	version?: string | null;
	reason?: string | null;
}

interface ManifestBinary {
	url: string;
	sha256: string;
}

interface DaemonManifest {
	version: string;
	minExtensionVersion?: string;
	releaseNotes?: string;
	binaries: Record<PlatformKey, ManifestBinary>;
}

export interface DaemonBootstrapResult {
	binaryPath: string;
	version: string;
	updated: boolean;
	manifest?: DaemonManifest | null;
}

export class DaemonRuntimeManager {
	private static outputChannel: vscode.OutputChannel | null = null;

	static setOutputChannel(channel: vscode.OutputChannel) {
		this.outputChannel = channel;
	}

	static getDefaultManifestUrl(): string {
		return process.env.MEMIX_DAEMON_MANIFEST_URL || DEFAULT_MANIFEST_URL;
	}

	static getPaths(context: vscode.ExtensionContext) {
		const storageRoot = context.globalStorageUri.fsPath;
		const binDir = path.join(storageRoot, 'bin');
		const binaryPath = path.join(binDir, process.platform === 'win32' ? 'memix-daemon.exe' : 'memix-daemon');
		const versionFile = path.join(storageRoot, 'daemon-version.txt');
		return { storageRoot, binDir, binaryPath, versionFile };
	}

	static getLocalDevBinaryPath(extensionPath: string): string {
		const binaryName = process.platform === 'win32' ? 'memix-daemon.exe' : 'memix-daemon';
		return path.join(extensionPath, '..', 'daemon', 'target', 'release', binaryName);
	}

	static getCurrentPlatformKey(): PlatformKey {
		if (process.platform === 'darwin' && process.arch === 'arm64') return 'darwin-arm64';
		if (process.platform === 'darwin' && process.arch === 'x64') return 'darwin-x64';
		if (process.platform === 'linux' && process.arch === 'x64') return 'linux-x64';
		if (process.platform === 'linux' && process.arch === 'arm64') return 'linux-arm64';
		if (process.platform === 'win32' && process.arch === 'x64') return 'windows-x64';
		throw new Error(`Unsupported platform for Memix daemon: ${process.platform}-${process.arch}`);
	}

	static getInitialState(): DaemonReadinessState {
		return {
			kind: 'downloading',
			title: 'Preparing Memix Daemon',
			description: 'Checking for the latest daemon before Memix becomes available.',
		};
	}

	static async prepareDaemon(
		context: vscode.ExtensionContext,
		extensionPath: string,
		extensionVersion: string,
		onStateChange?: (state: DaemonReadinessState) => void,
	): Promise<DaemonBootstrapResult> {
		const localDevBinaryPath = this.getLocalDevBinaryPath(extensionPath);
		if (fs.existsSync(localDevBinaryPath) && context.extensionMode === vscode.ExtensionMode.Development) {
			const result: DaemonBootstrapResult = {
				binaryPath: localDevBinaryPath,
				version: 'dev-local',
				updated: false,
				manifest: null,
			};
			onStateChange?.({
				kind: 'ready',
				title: 'Memix Daemon Ready',
				description: 'Using your locally built development daemon binary.',
				version: result.version,
			});
			return result;
		}

		const paths = this.getPaths(context);
		await fs.promises.mkdir(paths.binDir, { recursive: true });

		const installedVersion = await this.readInstalledVersion(paths.versionFile);
		const binaryExists = fs.existsSync(paths.binaryPath);

		let manifest: DaemonManifest | null = null;
		try {
			manifest = await this.fetchManifest(this.getDefaultManifestUrl());
		} catch (error: any) {
			this.outputChannel?.appendLine(`[runtime] Manifest fetch failed: ${error?.message || String(error)}`);
			if (binaryExists && installedVersion) {
				onStateChange?.({
					kind: 'ready',
					title: 'Memix Daemon Ready',
					description: 'Using the installed daemon because the update check is temporarily unavailable.',
					version: installedVersion,
				});
				return {
					binaryPath: paths.binaryPath,
					version: installedVersion,
					updated: false,
					manifest: null,
				};
			}
			throw new Error('Memix needs to download its daemon, but the manifest could not be fetched. Check your connection and try again.');
		}

		if (manifest.minExtensionVersion && this.compareVersions(extensionVersion, manifest.minExtensionVersion) < 0) {
			throw new Error(`This Memix extension version is too old for daemon ${manifest.version}. Update the extension to at least ${manifest.minExtensionVersion}.`);
		}

		const needsDownload = !binaryExists || installedVersion !== manifest.version;
		if (!needsDownload) {
			onStateChange?.({
				kind: 'ready',
				title: 'Memix Daemon Ready',
				description: 'The daemon is installed and up to date.',
				version: installedVersion,
			});
			return {
				binaryPath: paths.binaryPath,
				version: manifest.version,
				updated: false,
				manifest,
			};
		}

		const platformKey = this.getCurrentPlatformKey();
		const binary = manifest.binaries[platformKey];
		if (!binary?.url || !binary?.sha256) {
			throw new Error(`No daemon binary is published yet for ${platformKey}.`);
		}

		const transitionState: DaemonReadinessState = {
			kind: binaryExists ? 'updating' : 'downloading',
			title: binaryExists ? 'Updating Memix Daemon' : 'Downloading Memix Daemon',
			description: binaryExists
				? 'Installing the latest daemon update before Memix becomes available.'
				: 'Downloading the Memix daemon required to enable the extension.',
			version: manifest.version,
		};
		onStateChange?.(transitionState);

		await vscode.window.withProgress(
			{
				location: vscode.ProgressLocation.Notification,
				title: transitionState.title,
				cancellable: false,
			},
			async (progress) => {
				await this.downloadBinary(binary.url, paths.binaryPath, binary.sha256, progress, manifest!.version);
			}
		);

		await fs.promises.writeFile(paths.versionFile, `${manifest.version}\n`, 'utf8');

		onStateChange?.({
			kind: 'ready',
			title: 'Memix Daemon Ready',
			description: 'The daemon is installed and up to date.',
			version: manifest.version,
		});

		return {
			binaryPath: paths.binaryPath,
			version: manifest.version,
			updated: true,
			manifest,
		};
	}

	private static async readInstalledVersion(versionFile: string): Promise<string | null> {
		try {
			return (await fs.promises.readFile(versionFile, 'utf8')).trim() || null;
		} catch {
			return null;
		}
	}

	private static async fetchManifest(manifestUrl: string): Promise<DaemonManifest> {
		const body = await this.fetchText(manifestUrl, 0);
		const parsed = JSON.parse(body) as DaemonManifest;
		if (!parsed?.version || !parsed?.binaries) {
			throw new Error('Invalid daemon manifest payload');
		}
		return parsed;
	}

	private static async fetchText(urlString: string, redirectDepth: number): Promise<string> {
		const url = new URL(urlString);
		const transport = url.protocol === 'https:' ? https : http;
		return await new Promise((resolve, reject) => {
			const req = transport.get(url, { timeout: REQUEST_TIMEOUT_MS }, (res) => {
				if ((res.statusCode ?? 500) >= 300 && (res.statusCode ?? 500) < 400 && res.headers.location) {
					if (redirectDepth >= MAX_REDIRECTS) {
						res.resume();
						reject(new Error('Manifest request exceeded maximum redirect depth'));
						return;
					}
					res.resume();
					this.fetchText(new URL(res.headers.location, url).toString(), redirectDepth + 1).then(resolve, reject);
					return;
				}
				if ((res.statusCode ?? 500) >= 400) {
					res.resume();
					reject(new Error(`Request failed with status ${res.statusCode}`));
					return;
				}
				const chunks: Buffer[] = [];
				res.on('data', (chunk) => chunks.push(Buffer.from(chunk)));
				res.on('end', () => resolve(Buffer.concat(chunks).toString('utf8')));
			});
			req.on('timeout', () => req.destroy(new Error('Request timed out')));
			req.on('error', reject);
		});
	}

	private static async downloadBinary(
		urlString: string,
		destinationPath: string,
		expectedSha256: string,
		progress: vscode.Progress<{ message?: string; increment?: number }>,
		version: string,
		redirectDepth = 0,
		attempt = 1,
	): Promise<void> {
		const tempPath = `${destinationPath}.download`;
		await fs.promises.mkdir(path.dirname(destinationPath), { recursive: true });
		await fs.promises.rm(tempPath, { force: true });

		const url = new URL(urlString);
		const transport = url.protocol === 'https:' ? https : http;

		try {
			await new Promise<void>((resolve, reject) => {
			const request = transport.get(url, (response) => {
				if ((response.statusCode ?? 500) >= 300 && (response.statusCode ?? 500) < 400 && response.headers.location) {
					if (redirectDepth >= MAX_REDIRECTS) {
						response.resume();
						reject(new Error('Binary download exceeded maximum redirect depth'));
						return;
					}
					response.resume();
					this.downloadBinary(
						new URL(response.headers.location, url).toString(),
						destinationPath,
						expectedSha256,
						progress,
						version,
						redirectDepth + 1,
						attempt,
					)
						.then(resolve, reject);
					return;
				}
				if ((response.statusCode ?? 500) >= 400) {
					response.resume();
					reject(new Error(`Binary download failed with status ${response.statusCode}`));
					return;
				}

				const totalBytes = Number(response.headers['content-length'] || 0);
				let receivedBytes = 0;
				let lastReported = 0;
				const hash = crypto.createHash('sha256');
				const file = fs.createWriteStream(tempPath);

				response.on('data', (chunk: Buffer) => {
					receivedBytes += chunk.length;
					hash.update(chunk);
					if (totalBytes > 0) {
						const percent = Math.floor((receivedBytes / totalBytes) * 100);
						const increment = Math.max(0, percent - lastReported);
						lastReported = percent;
						progress.report({
							increment,
							message: `Downloading daemon ${percent}% (${this.formatBytes(receivedBytes)} / ${this.formatBytes(totalBytes)})`,
						});
					} else {
						progress.report({
							message: `Downloading daemon ${this.formatBytes(receivedBytes)} (version ${version})`,
						});
					}
				});

				response.pipe(file);

				file.on('finish', async () => {
					file.close();
					try {
						const actualSha256 = hash.digest('hex');
						if (actualSha256.toLowerCase() !== expectedSha256.toLowerCase()) {
							await fs.promises.rm(tempPath, { force: true });
							reject(new Error('Downloaded daemon checksum verification failed'));
							return;
						}
						await fs.promises.rename(tempPath, destinationPath);
						if (process.platform !== 'win32') {
							await fs.promises.chmod(destinationPath, 0o755);
						}
						progress.report({ increment: 100 - lastReported, message: 'Daemon download complete' });
						resolve();
					} catch (error) {
						reject(error);
					}
				});

				file.on('error', async (error) => {
					await fs.promises.rm(tempPath, { force: true });
					reject(error);
				});
			});

			request.setTimeout(30000, () => request.destroy(new Error('Binary download timed out')));
			request.on('error', async (error) => {
				await fs.promises.rm(tempPath, { force: true });
				reject(error);
			});
			});
		} catch (error) {
			await fs.promises.rm(tempPath, { force: true });
			if (attempt < MAX_DOWNLOAD_ATTEMPTS) {
				this.outputChannel?.appendLine(`[runtime] Download attempt ${attempt} failed, retrying once: ${error instanceof Error ? error.message : String(error)}`);
				progress.report({ message: 'Retrying daemon download after a transient failure...' });
				await this.downloadBinary(urlString, destinationPath, expectedSha256, progress, version, 0, attempt + 1);
				return;
			}
			throw error;
		}
	}

	private static formatBytes(bytes: number): string {
		if (bytes < 1024) return `${bytes} B`;
		if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
		return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
	}

	private static compareVersions(left: string, right: string): number {
		const normalize = (value: string) => value.replace(/^v/, '').split('-')[0].split('.').map((part) => Number(part) || 0);
		const a = normalize(left);
		const b = normalize(right);
		const len = Math.max(a.length, b.length);
		for (let index = 0; index < len; index += 1) {
			const av = a[index] || 0;
			const bv = b[index] || 0;
			if (av > bv) return 1;
			if (av < bv) return -1;
		}
		return 0;
	}
}
