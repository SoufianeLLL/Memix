import * as CryptoJS from 'crypto-js';

// This key is embedded in compiled JS — not perfect security
// but prevents casual copy-paste of templates
const TEMPLATE_KEY = 'memix-' + Buffer.from('dGVtcGxhdGUtdjE=', 'base64').toString();

export function encryptTemplate(plaintext: string): string {
    return CryptoJS.AES.encrypt(plaintext, TEMPLATE_KEY).toString();
}

export function decryptTemplate(ciphertext: string): string {
    const bytes = CryptoJS.AES.decrypt(ciphertext, TEMPLATE_KEY);
    return bytes.toString(CryptoJS.enc.Utf8);
}

// For user data encryption in Redis
export function encryptBrainData(data: string, userKey: string): string {
    return CryptoJS.AES.encrypt(data, userKey).toString();
}

export function decryptBrainData(encrypted: string, userKey: string): string {
    const bytes = CryptoJS.AES.decrypt(encrypted, userKey);
    return bytes.toString(CryptoJS.enc.Utf8);
}

export function hashProjectId(projectName: string): string {
    return CryptoJS.SHA256(projectName).toString().substring(0, 12);
}