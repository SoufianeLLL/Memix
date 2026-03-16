import { MAX_KEY_SIZE_BYTES, BRAIN_KEYS } from '../utils/constants';

export interface ValidationResult {
    valid: boolean;
    errors: string[];
    warnings: string[];
    sanitized?: string;
}

export function validateBrainWrite(key: string, value: string): ValidationResult {
    const errors: string[] = [];
    const warnings: string[] = [];

    // Size check
    const sizeBytes = Buffer.byteLength(value, 'utf8');
    if (sizeBytes > MAX_KEY_SIZE_BYTES) {
        errors.push(`Value exceeds max size: ${sizeBytes}B > ${MAX_KEY_SIZE_BYTES}B`);
    }
    if (sizeBytes > MAX_KEY_SIZE_BYTES * 0.8) {
        warnings.push(`Value at ${Math.round(sizeBytes / MAX_KEY_SIZE_BYTES * 100)}% capacity`);
    }

    // JSON validity check
    let parsed: any;
    try {
        parsed = JSON.parse(value);
    } catch {
        errors.push('Value is not valid JSON');
        return { valid: false, errors, warnings };
    }

    // Key-specific validation
    switch (key) {
        case BRAIN_KEYS.IDENTITY:
            if (!parsed.name) { errors.push('identity.name is required'); }
            if (!parsed.purpose) { errors.push('identity.purpose is required'); }
            if (!parsed.tech_stack) { warnings.push('identity.tech_stack is missing'); }
            break;

        case BRAIN_KEYS.SESSION_STATE:
            if (!parsed.last_updated) { errors.push('session_state.last_updated is required'); }
            if (!parsed.current_task) { warnings.push('session_state.current_task is empty'); }
            if (typeof parsed.session_number !== 'number') {
                warnings.push('session_state.session_number should be a number');
            }
            break;

        case BRAIN_KEYS.SESSION_LOG:
            if (!Array.isArray(parsed)) {
                errors.push('session_log must be a JSON array');
            }
            break;

        case BRAIN_KEYS.DECISIONS:
            if (!Array.isArray(parsed)) {
                errors.push('decisions must be a JSON array');
            }
            break;

        case BRAIN_KEYS.PATTERNS:
            if (typeof parsed !== 'object') {
                errors.push('patterns must be a JSON object');
            }
            break;

        case BRAIN_KEYS.FILE_MAP:
            if (typeof parsed !== 'object') {
                errors.push('file_map must be a JSON object');
            }
            break;

        case BRAIN_KEYS.KNOWN_ISSUES:
            if (!Array.isArray(parsed)) {
                errors.push('known_issues must be a JSON array');
            }
            break;
    }

    // Secret detection — NEVER store these
    const secretPatterns = [
        /sk[-_][a-zA-Z0-9]{20,}/,          // API keys
        /ghp_[a-zA-Z0-9]{36}/,             // GitHub tokens
        /-----BEGIN (RSA |EC )?PRIVATE KEY/,
        /password\s*[:=]\s*["'][^"']+["']/i,
        /Bearer\s+[a-zA-Z0-9\-._~+\/]+=*/,
        /AKIA[0-9A-Z]{16}/,                // AWS keys
    ];

    const valueStr = JSON.stringify(parsed);
    for (const pattern of secretPatterns) {
        if (pattern.test(valueStr)) {
            errors.push('BLOCKED: Value contains what appears to be a secret/credential');
            break;
        }
    }

    // Sanitize — trim whitespace in string values
    const sanitized = JSON.stringify(parsed);

    return {
        valid: errors.length === 0,
        errors,
        warnings,
        sanitized
    };
}