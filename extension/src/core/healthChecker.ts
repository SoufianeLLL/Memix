/**
 * Brain Health Checker - Periodically verifies brain integrity and auto-repairs
 * 
 * Runs every 60 seconds (configurable) to check:
 * 1. Database file exists
 * 2. Required keys exist in database
 * 3. JSON mirror directory has files
 */

import * as fs from 'fs';
import * as path from 'path';
import { BrainManager } from './brain';
import { MemoryClient } from '../client';
import { BRAIN_KEYS } from '../utils/constants';

export interface HealthCheckResult {
    healthy: boolean;
    issues: string[];
    repaired: string[];
}

export class BrainHealthChecker {
    private interval: NodeJS.Timeout | null = null;
    private brain: BrainManager;
    private workspaceRoot: string;
    private projectId: string;
    private checkIntervalMs: number;
    private lastCheckTime: number = 0;
    private isChecking: boolean = false;
    
    constructor(
        brain: BrainManager,
        workspaceRoot: string,
        projectId: string,
        checkIntervalMs: number = 60000
    ) {
        this.brain = brain;
        this.workspaceRoot = workspaceRoot;
        this.projectId = projectId;
        this.checkIntervalMs = checkIntervalMs;
    }
    
    /**
     * Start periodic health checks
     */
    start(): void {
        if (this.interval) {
            return; // Already running
        }
        
        // Delay first check by 5 seconds to avoid race with manual init on startup
        setTimeout(() => {
            this.check().catch(err => {
                console.warn('Memix: Initial health check failed', err);
            });
        }, 5000);
        
        // Schedule periodic checks
        this.interval = setInterval(() => {
            this.check().catch(err => {
                console.warn('Memix: Periodic health check failed', err);
            });
        }, this.checkIntervalMs);
        
        console.log(`Memix: Health checker started (interval: ${this.checkIntervalMs}ms, first check in 5s)`);
    }
    
    /**
     * Stop health checks
     */
    stop(): void {
        if (this.interval) {
            clearInterval(this.interval);
            this.interval = null;
        }
    }
    
    /**
     * Run a single health check
     */
    async check(): Promise<HealthCheckResult> {
        // Prevent concurrent checks
        if (this.isChecking) {
            return { healthy: true, issues: [], repaired: [] };
        }
        
        this.isChecking = true;
        this.lastCheckTime = Date.now();
        
        const issues: string[] = [];
        const repaired: string[] = [];
        
        try {
            // 1. Check if brain.db exists
            const dbPath = path.join(this.workspaceRoot, '.memix', 'brain.db');
            if (!fs.existsSync(dbPath)) {
                issues.push('Database file missing');
                await this.repairDatabase();
                repaired.push('Database file recreated');
            }
            
            // 2. Check if .memix directory exists
            const memixDir = path.join(this.workspaceRoot, '.memix');
            if (!fs.existsSync(memixDir)) {
                issues.push('.memix directory missing');
                fs.mkdirSync(memixDir, { recursive: true });
                repaired.push('.memix directory created');
            }
            
            // 3. Check if required keys exist in database
            const missingKeys = await this.checkRequiredKeys();
            if (missingKeys.length > 0) {
                issues.push(`Missing keys: ${missingKeys.join(', ')}`);
                await this.brain.init();
                repaired.push('Missing keys restored');
            }
            
            // 4. Check if JSON mirror directory has files
            const brainDir = path.join(this.workspaceRoot, '.memix', 'brain');
            const mirrorMissing = !fs.existsSync(brainDir);
            const mirrorEmpty = fs.existsSync(brainDir) && fs.readdirSync(brainDir).length === 0;
            
            if (mirrorMissing || mirrorEmpty) {
                issues.push(mirrorMissing ? 'JSON mirror directory missing' : 'JSON mirror directory empty');
                await this.repairMirror();
                repaired.push('JSON mirror files recreated');
            }
            
            return {
                healthy: issues.length === 0,
                issues,
                repaired
            };
        } finally {
            this.isChecking = false;
        }
    }
    
    /**
     * Check if all required keys exist
     */
    private async checkRequiredKeys(): Promise<string[]> {
        const requiredKeys = [
            BRAIN_KEYS.IDENTITY,
            BRAIN_KEYS.SESSION_STATE,
            BRAIN_KEYS.PATTERNS,
            BRAIN_KEYS.TASKS,
            BRAIN_KEYS.DECISIONS,
            BRAIN_KEYS.FILE_MAP,
            BRAIN_KEYS.KNOWN_ISSUES,
            BRAIN_KEYS.META
        ];
        
        const missing: string[] = [];
        
        for (const key of requiredKeys) {
            const value = await this.brain.get(key);
            if (!value) {
                missing.push(key);
            }
        }
        
        return missing;
    }
    
    /**
     * Repair database by reinitializing
     */
    private async repairDatabase(): Promise<void> {
        // Ensure directory exists
        const memixDir = path.join(this.workspaceRoot, '.memix');
        if (!fs.existsSync(memixDir)) {
            fs.mkdirSync(memixDir, { recursive: true });
        }
        
        // Reinitialize brain - this will create the database via daemon
        await this.brain.init();
    }
    
    /**
     * Repair JSON mirror by exporting from database
     */
    private async repairMirror(): Promise<void> {
        try {
            await MemoryClient.exportBrainMirror(this.projectId, this.workspaceRoot);
        } catch (err) {
            console.warn('Memix: Failed to repair mirror', err);
            // Try to init first, then export
            await this.brain.init();
            await MemoryClient.exportBrainMirror(this.projectId, this.workspaceRoot);
        }
    }
    
    /**
     * Get status info
     */
    getStatus(): { running: boolean; lastCheck: number | null } {
        return {
            running: this.interval !== null,
            lastCheck: this.lastCheckTime || null
        };
    }
}
