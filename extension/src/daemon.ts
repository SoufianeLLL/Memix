import * as vscode from 'vscode';
import { spawn, ChildProcess } from 'child_process';
import * as http from 'http';
import * as os from 'os';
import * as path from 'path';

export class DaemonManager {
    private static outputChannel: vscode.OutputChannel | null = null;
    private static binaryPath: string | null = null;

    static setOutputChannel(channel: vscode.OutputChannel) {
        this.outputChannel = channel;
    }

    static setBinaryPath(binaryPath: string) {
        this.binaryPath = binaryPath;
    }

    static getBinaryPath(): string | null {
        return this.binaryPath;
    }

    private static process: ChildProcess | null = null;
    private static socketPath = path.join(os.homedir(), '.memix', 'daemon.sock');

    /**
     * Pings an already-running daemon (no spawn). Useful for dev external daemon mode.
     */
    static async ping(): Promise<{ status: string, message?: string, workspace_root?: string, project_id?: string }> {
        const httpUrl = process.env.MEMIX_DAEMON_HTTP_URL;
        if (httpUrl) {
            return await this.pingHttp(httpUrl);
        }
        return await this.pingDaemon();
    }

    private static pingHttp(baseUrl: string): Promise<{ status: string, message?: string, workspace_root?: string, project_id?: string }> {
        return new Promise((resolve, reject) => {
            let url: URL;
            try {
                url = new URL('/health', baseUrl);
            } catch (e) {
                reject(e);
                return;
            }
            const req = http.request(
                {
                    hostname: url.hostname,
                    port: url.port ? Number(url.port) : 80,
                    path: url.pathname,
                    method: 'GET'
                },
                (res) => {
                    let body = '';
                    res.on('data', chunk => body += chunk);
                    res.on('end', () => {
                        if (res.statusCode === 200) {
                            try {
                                const parsed = JSON.parse(body);
                                resolve(parsed);
                            } catch (e) {
                                resolve({ status: 'healthy', message: body });
                            }
                        } else {
                            reject(new Error(`Ping returned ${res.statusCode}`));
                        }
                    });
                }
            );
            req.on('error', reject);
            req.end();
        });
    }

    /**
     * Spawns the Rust Axum daemon and waits for it to become healthy.
     */
    static async start(binaryPath: string, redisUrl?: string): Promise<void>;
    static async start(binaryPath: string, workspaceRoot?: string | null, redisUrl?: string): Promise<void>;
    static async start(binaryPath: string, workspaceRoot?: string | null, projectId?: string | null, redisUrl?: string): Promise<void>;
    static async start(binaryPath: string, a?: string | null, b?: string | null, c?: string): Promise<void> {
        if (this.process) return;
        this.binaryPath = binaryPath;

        const workspaceRoot = c === undefined ? (b === undefined ? null : a) : a;
        const projectId = c === undefined ? null : b;
        const redisUrl = c === undefined ? ((b === undefined ? (a ?? undefined) : b) ?? undefined) : c;

        try {
            const resp = await this.pingDaemon();
            const sameWorkspace = !workspaceRoot || !resp.workspace_root || resp.workspace_root === workspaceRoot;
            const sameProject = !projectId || !resp.project_id || resp.project_id === projectId;

            if (sameWorkspace && sameProject) {
                console.log(`Memix Daemon is already running. Status: ${resp.status}`);
                return;
            } else {
                console.log(`Memix Daemon running for different project (${resp.project_id}). Shutting it down...`);
                try {
                    await new Promise<void>((resolve, reject) => {
                        const req = http.request({
                            socketPath: this.socketPath,
                            path: '/api/v1/daemon/shutdown',
                            method: 'POST'
                        }, (res) => {
                            res.on('data', () => {});
                            res.on('end', resolve);
                        });
                        req.on('error', resolve); // ignore errors during shutdown
                        req.end();
                    });
                    await new Promise(r => setTimeout(r, 500)); // give it a moment to exit
                } catch (e) {
                    console.error("Error shutting down previous daemon", e);
                }
            }
        } catch {
            // Not running, proceed with spawning
        }

        if (!this.binaryPath) {
            throw new Error('Memix daemon binary path is not configured.');
        }

        this.outputChannel?.appendLine(`[runtime] Starting daemon from ${this.binaryPath}`);
        this.process = spawn(this.binaryPath, [], {
            detached: false,
            env: {
                ...process.env,
                MEMIX_PORT: process.env.MEMIX_PORT || '3456',
                ...(workspaceRoot ? { MEMIX_WORKSPACE_ROOT: workspaceRoot } : {}),
                ...(projectId ? { MEMIX_PROJECT_ID: projectId } : {}),
                ...(redisUrl ? { MEMIX_REDIS_URL: redisUrl } : {}),
                RUST_LOG: process.env.RUST_LOG || 'info,memix_daemon=debug',
            },
            stdio: 'pipe'
        });

        // this.process.stdout?.on('data', (data) => console.log(`[Memix Daemon] ${data.toString()}`));
        // this.process.stderr?.on('data', (data) => console.error(`[Memix Daemon Err] ${data.toString()}`));

        this.process.stdout?.on('data', (data) => {
            const lines = data.toString().split('\n');
            lines.forEach((line: string) => {
                if (line.trim()) {
                    console.log(`[Memix Daemon] ${line}`);
                    this.outputChannel?.appendLine(`[stdout] ${line}`);
                }
            });
        });

        this.process.stderr?.on('data', (data) => {
            const lines = data.toString().split('\n');
            lines.forEach((line: string) => {
                if (line.trim()) {
                    console.error(`[Memix Daemon Err] ${line}`);
                    this.outputChannel?.appendLine(`[ERROR] ${line}`);
                }
            });
        });

        this.process.on('close', (code) => {
            console.log(`[Memix Daemon] exited with code ${code}`);
            this.process = null;
        });

        try {
            await this.waitForHealthCheck();
            console.log("Memix Memory Bridge (Rust Daemon) is online and healthy.");
        } catch (e) {
            console.error("Failed to spawn Memix Daemon. Ensure the Rust binary is compiled and present.", e);
            this.outputChannel?.appendLine(`[runtime] Failed to start daemon: ${e instanceof Error ? e.message : String(e)}`);
            this.stop();
            throw e;
        }
    }

    /**
     * Kills the Rust daemon when VS Code is closed.
     */
    static stop() {
        if (this.process) {
            this.process.kill();
            this.process = null;
        }
    }

    private static waitForHealthCheck(): Promise<{ status: string, message?: string, workspace_root?: string, project_id?: string }> {
        return new Promise((resolve, reject) => {
            let retries = 0;
            const maxRetries = 60;
            const healthPath = '/health';

            const requestOnce = () => {
                const req = http.request(
                    {
                        socketPath: this.socketPath,
                        path: healthPath,
                        method: 'GET'
                    },
                    (res) => {
                        let body = '';
                        res.on('data', chunk => body += chunk);
                        res.on('end', () => {
                            if (res.statusCode === 200) {
                                try {
                                    const parsed = JSON.parse(body);
                                    resolve(parsed);
                                } catch (e) {
                                    resolve({ status: 'healthy', message: body });
                                }
                            } else {
                                retry(new Error(`Health check returned ${res.statusCode}`));
                            }
                        });
                    }
                );
                req.on('error', (err) => retry(err));
                req.end();
            };

            const retry = (err?: Error) => {
                retries++;
                if (retries > maxRetries) {
                    const extra = err ? ` Last error: ${err.message}` : '';
                    reject(new Error(`Daemon failed to start or respond on ${this.socketPath}${healthPath}.${extra}`));
                } else {
                    setTimeout(requestOnce, 1000);
                }
            };

            // Start first check
            setTimeout(requestOnce, 1000);
        });
    }

    private static pingDaemon(): Promise<{ status: string, message?: string, workspace_root?: string, project_id?: string }> {
        return new Promise((resolve, reject) => {
            const req = http.request(
                {
                    socketPath: this.socketPath,
                    path: '/health',
                    method: 'GET'
                },
                (res) => {
                    let body = '';
                    res.on('data', chunk => body += chunk);
                    res.on('end', () => {
                        if (res.statusCode === 200) {
                            try {
                                const parsed = JSON.parse(body);
                                resolve(parsed);
                            } catch (e) {
                                resolve({ status: 'healthy', message: body });
                            }
                        } else {
                            reject(new Error(`Ping returned ${res.statusCode}`));
                        }
                    });
                }
            );
            req.on('error', reject);
            req.end();
        });
    }

    static async getSettings(): Promise<any> {
        return new Promise((resolve, reject) => {
            const httpUrl = process.env.MEMIX_DAEMON_HTTP_URL;
            const options: http.RequestOptions = {
                method: 'GET',
                path: '/api/v1/control/status'
            };

            if (httpUrl) {
                try {
                    const url = new URL(options.path!, httpUrl);
                    options.hostname = url.hostname;
                    options.port = url.port ? Number(url.port) : 80;
                    options.path = url.pathname;
                } catch (e) {
                    reject(e);
                    return;
                }
            } else {
                options.socketPath = this.socketPath;
            }

            const req = http.request(options, (res) => {
                let body = '';
                res.on('data', chunk => body += chunk);
                res.on('end', () => {
                    if (res.statusCode === 200) {
                        try {
                            resolve(JSON.parse(body));
                        } catch (e) {
                            reject(new Error('Failed to parse settings JSON'));
                        }
                    } else {
                        reject(new Error(`Failed to get settings: ${res.statusCode} ${body}`));
                    }
                });
            });
            req.on('error', reject);
            req.end();
        });
    }

    private static async sendControlCommand(endpoint: string, payload?: any): Promise<void> {
        return new Promise((resolve, reject) => {
            const httpUrl = process.env.MEMIX_DAEMON_HTTP_URL;
            const options: http.RequestOptions = {
                method: 'POST',
                path: `/api/v1/control/${endpoint}`
            };

            if (httpUrl) {
                try {
                    const url = new URL(options.path!, httpUrl);
                    options.hostname = url.hostname;
                    options.port = url.port ? Number(url.port) : 80;
                    options.path = url.pathname;
                } catch (e) {
                    reject(e);
                    return;
                }
            } else {
                options.socketPath = this.socketPath;
            }

            const req = http.request(options, (res) => {
                let body = '';
                res.on('data', chunk => body += chunk);
                res.on('end', () => {
                    if (res.statusCode === 200 || res.statusCode === 204) {
                        resolve();
                    } else {
                        reject(new Error(`Daemon control command failed: ${res.statusCode} ${body}`));
                    }
                });
            });
            req.on('error', reject);
            if (payload !== undefined) {
                req.setHeader('Content-Type', 'application/json');
                req.write(JSON.stringify(payload));
            }
            req.end();
        });
    }

    static async pause(): Promise<void> {
        await this.sendControlCommand('pause');
    }

    static async resume(): Promise<void> {
        await this.sendControlCommand('resume');
    }



}