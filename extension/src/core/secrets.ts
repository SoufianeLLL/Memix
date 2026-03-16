import * as vscode from 'vscode';

export class SecretManager {
    static instance: SecretManager;

    constructor(private context: vscode.ExtensionContext) {
        SecretManager.instance = this;
    }

    static getInstance(): SecretManager {
        return SecretManager.instance;
    }

    async getSecret(key: string): Promise<string | undefined> {
        return await this.context.secrets.get(key);
    }

    async storeSecret(key: string, value: string): Promise<void> {
        await this.context.secrets.store(key, value);
    }

    async deleteSecret(key: string): Promise<void> {
        await this.context.secrets.delete(key);
    }
}
