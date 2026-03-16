import * as assert from 'assert';
import * as vscode from 'vscode';
import * as path from 'path';

suite('Memix Extension Test Suite', () => {
    vscode.window.showInformationMessage('Start all tests.');

    test('Extension should be present', () => {
        assert.ok(vscode.extensions.getExtension('digitalvizellc.memix'));
    });

    test('hashProjectId handles consistent path hashing', () => {
        // Mock logic here when crypto utilities are imported
        assert.strictEqual(1, 1);
    });
});
