/**
 * Lightweight token estimation.
 * We avoid bundling tiktoken (huge) — use approximation instead.
 * Rule of thumb: 1 token ≈ 4 characters for English text / code.
 * JSON is slightly higher due to syntax chars.
 */

const CHARS_PER_TOKEN = 3.5; // conservative for JSON/code

export function getTokenCount(text: string): number {
    return Math.ceil(text.length / CHARS_PER_TOKEN);
}

export function trimToTokenBudget(text: string, maxTokens: number): string {
    const maxChars = Math.floor(maxTokens * CHARS_PER_TOKEN);
    if (text.length <= maxChars) { return text; }

    // Try to trim at a clean JSON boundary
    const trimmed = text.substring(0, maxChars);

    // Find last complete line
    const lastNewline = trimmed.lastIndexOf('\n');
    if (lastNewline > maxChars * 0.7) {
        return trimmed.substring(0, lastNewline) + '\n... [trimmed to fit token budget]';
    }

    return trimmed + '... [trimmed]';
}

export function estimateTokens(obj: any): number {
    return getTokenCount(JSON.stringify(obj));
}