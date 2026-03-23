import * as vscode from 'vscode';
import { LicenseBillingInterval, MemoryClient, LicenseStatusResponse } from './client';
import { SecretManager } from './core/secrets';

export class LicenseManager {
	static readonly LICENSE_SECRET = 'licenseKey';
	static readonly DEVICE_ID_SECRET = 'licenseDeviceId';

	constructor(
		private secrets: SecretManager,
		private statusBarItem: vscode.StatusBarItem,
	) {}

	async restoreOnStartup(): Promise<void> {
		const key = await this.secrets.getSecret(LicenseManager.LICENSE_SECRET);
		if (key) {
			try {
				await this.activateWithKey(key, false, false);
				return;
			} catch {
			}
		}
		await this.refreshStatusBar();
	}

	async refreshStatusBar(): Promise<void> {
		try {
			const status = await MemoryClient.getLicenseStatus(await this.getDeviceId());
			this.statusBarItem.text = this.getStatusBarText(status);
		} catch {
			this.statusBarItem.text = 'Memix';
		}
	}

	async startActivationFlow(): Promise<boolean> {
		const email = await vscode.window.showInputBox({
			title: 'Activate Memix Pro',
			prompt: 'Enter the email address you use for Memix Pro',
			placeHolder: 'you@company.com',
			ignoreFocusOut: true,
			validateInput: (value) => value.includes('@') ? null : 'Please enter a valid email',
		});
		if (!email) {
			return false;
		}

		const billingOption = await vscode.window.showQuickPick(
			[
				{ label: 'Monthly', billingInterval: 'monthly' as LicenseBillingInterval, description: 'Pay monthly for Memix Pro' },
				{ label: 'Yearly', billingInterval: 'yearly' as LicenseBillingInterval, description: 'Pay yearly for Memix Pro' },
			],
			{
				title: 'Choose Memix Pro billing',
				placeHolder: 'Select a billing interval',
				ignoreFocusOut: true,
			}
		);
		if (!billingOption) {
			return false;
		}

		await vscode.window.withProgress({
			location: vscode.ProgressLocation.Notification,
			title: 'Memix Pro',
			cancellable: false,
		}, async (progress) => {
			progress.report({ message: 'Preparing activation...' });
			const initiation = await MemoryClient.initiateLicense(email, billingOption.billingInterval);
			if (initiation.key) {
				await this.activateWithKey(initiation.key, true, true);
				return;
			}

			if (initiation.checkout_url) {
				progress.report({ message: 'Opening checkout...' });
				await vscode.env.openExternal(vscode.Uri.parse(initiation.checkout_url));
			}

			if (initiation.token) {
				progress.report({ message: 'Waiting for license confirmation...' });
				const key = await this.pollForLicense(initiation.token);
				if (key) {
					await this.activateWithKey(key, true, true);
					return;
				}
			}

			throw new Error(initiation.message || 'License activation did not complete in time');
		});
		return true;
	}

	async promptAndActivate(): Promise<boolean> {
		const key = await vscode.window.showInputBox({
			title: 'Activate Memix Pro',
			prompt: 'Enter your Memix Pro license key',
			ignoreFocusOut: true,
			password: false,
			validateInput: (value) => value.trim() ? null : 'License key is required',
		});
		if (!key) {
			return false;
		}
		await this.activateWithKey(key, true, true);
		return true;
	}

	async ensureProLicense(): Promise<boolean> {
		const status = await MemoryClient.getLicenseStatus(await this.getDeviceId());
		if (status.active && status.tier === 'pro') {
			await this.refreshStatusBar();
			return true;
		}
		const choice = await vscode.window.showWarningMessage(
			status.message || 'Team sync requires Memix Pro.',
			'Activate Memix Pro',
			'Enter License Key',
			'Cancel',
		);
		if (choice === 'Activate Memix Pro') {
			return this.startActivationFlow();
		}
		if (choice !== 'Enter License Key') {
			return false;
		}
		return this.promptAndActivate();
	}

	private async activateWithKey(key: string, persist: boolean, notify: boolean): Promise<LicenseStatusResponse> {
		const status = await MemoryClient.activateLicense(key, await this.getDeviceId());
		if (!status.active || status.tier !== 'pro') {
			throw new Error(status.message || 'License is not active for Memix Pro');
		}
		if (persist) {
			await this.secrets.storeSecret(LicenseManager.LICENSE_SECRET, key);
		}
		await this.refreshStatusBar();
		if (notify) {
			vscode.window.showInformationMessage('Memix Pro activated successfully.');
		}
		return status;
	}

	private getStatusBarText(status: LicenseStatusResponse): string {
		if (status.active && status.tier === 'pro') {
			return 'Memix Pro';
		}
		return 'Memix';
	}

	private async pollForLicense(token: string): Promise<string | null> {
		const deadline = Date.now() + 10 * 60 * 1000;
		while (Date.now() < deadline) {
			const pending = await MemoryClient.getPendingLicense(token);
			if (pending.ready && pending.key) {
				return pending.key;
			}
			await new Promise((resolve) => setTimeout(resolve, 3000));
		}
		return null;
	}

	private async getDeviceId(): Promise<string> {
		const existing = await this.secrets.getSecret(LicenseManager.DEVICE_ID_SECRET);
		if (existing) {
			return existing;
		}
		const generated = `${vscode.env.machineId}:${vscode.env.sessionId}`;
		await this.secrets.storeSecret(LicenseManager.DEVICE_ID_SECRET, generated);
		return generated;
	}
}
