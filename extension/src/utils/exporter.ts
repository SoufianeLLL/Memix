import * as fs from 'fs';
import * as path from 'path';
import { BrainManager } from '../core/brain';

export async function exportBrain(brain: BrainManager, workspaceRoot: string): Promise<string> {
    const allData = await brain.getAll();

    const exportData = {
        memix_version: '1.4.4',
        exported_at: new Date().toISOString(),
        brain: allData
    };

    const filename = `memix-brain-export-${Date.now()}.json`;
    const filePath = path.join(workspaceRoot, filename);

    fs.writeFileSync(filePath, JSON.stringify(exportData, null, 2), 'utf8');

    return filePath;
}

export async function importBrain(brain: BrainManager, filePath: string): Promise<string[]> {
    const raw = fs.readFileSync(filePath, 'utf8');
    const data = JSON.parse(raw);

    if (!data.memix_version || !data.brain) {
        throw new Error('Invalid Memix brain export file');
    }

    const imported: string[] = [];

    for (const [key, value] of Object.entries(data.brain)) {
        await brain.set(key, value);
        imported.push(key);
    }

    return imported;
}