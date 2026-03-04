import * as vscode from 'vscode';
import Redis from 'ioredis';

let client: Redis | null = null;
let connectionStatus: 'connected' | 'disconnected' | 'error' = 'disconnected';

const statusEmitter = new vscode.EventEmitter<string>();
export const onStatusChange = statusEmitter.event;

export function getStatus(): string {
    return connectionStatus;
}

export async function connect(url?: string): Promise<Redis> {
    if (client && connectionStatus === 'connected') {
        return client;
    }

    if (!url) {
        throw new Error('No Redis URL configured. Run "Memix: Connect Redis"');
    }

    return new Promise((resolve, reject) => {
        client = new Redis(url, {
            maxRetriesPerRequest: 3,
            retryStrategy(times) {
                if (times > 3) { return null; }
                return Math.min(times * 200, 2000);
            },
            lazyConnect: false
        });

        client.on('connect', () => {
            connectionStatus = 'connected';
            statusEmitter.fire('connected');
            resolve(client!);
        });

        client.on('error', (err) => {
            connectionStatus = 'error';
            statusEmitter.fire('error');
            if (client?.status === 'connecting') {
                reject(err);
            }
        });

        client.on('close', () => {
            connectionStatus = 'disconnected';
            statusEmitter.fire('disconnected');
        });
    });
}

export async function disconnect(): Promise<void> {
    if (client) {
        await client.quit();
        client = null;
        connectionStatus = 'disconnected';
        statusEmitter.fire('disconnected');
    }
}

export function getClient(): Redis {
    if (!client || connectionStatus !== 'connected') {
        throw new Error('Redis not connected. Run "Memix: Connect Redis"');
    }
    return client;
}

// Brain-specific operations
export async function brainGet(prefix: string, key: string): Promise<string | null> {
    const c = getClient();
    return c.get(`${prefix}:${key}`);
}

export async function brainSet(prefix: string, key: string, value: string): Promise<void> {
    const c = getClient();
    await c.set(`${prefix}:${key}`, value);
}

export async function brainDel(prefix: string, key: string): Promise<void> {
    const c = getClient();
    await c.del(`${prefix}:${key}`);
}

export async function brainGetAll(prefix: string): Promise<Record<string, string>> {
    const c = getClient();
    const keys = await c.keys(`${prefix}:*`);
    if (keys.length === 0) { return {}; }

    const pipeline = c.pipeline();
    keys.forEach(k => pipeline.get(k));
    const results = await pipeline.exec();

    const data: Record<string, string> = {};
    keys.forEach((k, i) => {
        const shortKey = k.replace(`${prefix}:`, '');
        const val = results?.[i]?.[1];
        if (typeof val === 'string') {
            data[shortKey] = val;
        }
    });
    return data;
}

export async function brainSize(prefix: string): Promise<number> {
    const data = await brainGetAll(prefix);
    return Object.values(data).reduce((sum, val) => sum + Buffer.byteLength(val, 'utf8'), 0);
}

export async function brainKeys(prefix: string): Promise<string[]> {
    const c = getClient();
    const keys = await c.keys(`${prefix}:*`);
    return keys.map(k => k.replace(`${prefix}:`, ''));
}

export async function infoMemory(): Promise<{ usedBytes: number, maxBytes: number }> {
    const c = getClient();
    const info = await c.info('memory');

    let usedBytes = 0;
    let maxBytes = 0;

    const lines = info.split('\n');
    for (const line of lines) {
        if (line.startsWith('used_memory:')) {
            usedBytes = parseInt(line.split(':')[1].trim(), 10);
        } else if (line.startsWith('maxmemory:')) {
            maxBytes = parseInt(line.split(':')[1].trim(), 10);
        }
    }

    // If Redis has no maxmemory configured (0), default to a reasonable display maximum like 100MB or 30MB
    if (!maxBytes) {
        maxBytes = 30 * 1024 * 1024; // 30 MB as user requested fallback
    }

    return { usedBytes, maxBytes };
}