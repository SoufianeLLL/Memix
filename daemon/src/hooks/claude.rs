//! Claude Code hook implementation.
//!
//! Generates the PreToolUse hook script that intercepts Bash commands
//! and rewrites them through the Memix daemon API.

use anyhow::Result;

/// Generate the Claude Code hook script
pub fn generate_hook_script(daemon_url: &str) -> String {
    format!(r#"#!/usr/bin/env bash
# memix-hook-version: 1
# Memix Claude Code hook - rewrites commands for token optimization.
#
# This hook intercepts Bash commands and routes them through the Memix daemon
# for intelligent filtering. Output is reduced by 60-90% while keeping
# essential information.
#
# Exit codes:
#   0 + stdout  Rewrite found, return updated command
#   0           No rewrite needed, pass through unchanged

set -euo pipefail

# Check dependencies
if ! command -v jq &>/dev/null; then
    echo "[memix] WARNING: jq is not installed. Hook cannot rewrite commands." >&2
    exit 0
fi

# Read input from Claude Code
INPUT=$(cat)
CMD=$(echo "$INPUT" | jq -r '.tool_input.command // empty')

if [ -z "$CMD" ]; then
    exit 0
fi

# Call Memix daemon API for rewrite
DAEMON_URL="${{DAEMON_URL:-"{daemon_url}}}"
REWRITE_URL="${{DAEMON_URL}}/api/v1/hooks/rewrite"

RESPONSE=$(curl -s -X POST \
    -H "Content-Type: application/json" \
    -d "$(jq -n --arg cmd "$CMD" '{{command: $cmd}}')" \
    "$REWRITE_URL" 2>/dev/null || echo '{{"error": "daemon not running"}}')

# Parse response
CLASSIFICATION=$(echo "$RESPONSE" | jq -r '.classification // "Unsupported"')

case "$CLASSIFICATION" in
    "Supported")
        REWRITTEN=$(echo "$RESPONSE" | jq -r '.rewritten')
        if [ "$CMD" = "$REWRITTEN" ]; then
            # Already using memix, pass through
            exit 0
        fi
        
        # Return updated command
        ORIGINAL_INPUT=$(echo "$INPUT" | jq -c '.tool_input')
        UPDATED_INPUT=$(echo "$ORIGINAL_INPUT" | jq --arg cmd "$REWRITTEN" '.command = $cmd')
        
        jq -n --argjson updated "$UPDATED_INPUT" \
            '{{tool_input: $updated, permissionDecision: "allow"}}'
        ;;
    
    "Denied")
        REASON=$(echo "$RESPONSE" | jq -r '.reason')
        SUGGESTION=$(echo "$RESPONSE" | jq -r '.suggestion // empty')
        
        # Let Claude Code's native deny handling take over
        echo "[memix] Blocked: $REASON" >&2
        [ -n "$SUGGESTION" ] && echo "[memix] Suggestion: $SUGGESTION" >&2
        exit 0
        ;;
    
    "Ask")
        REWRITTEN=$(echo "$RESPONSE" | jq -r '.rewritten')
        PROMPT=$(echo "$RESPONSE" | jq -r '.prompt')
        
        # Rewrite but don't auto-allow - Claude Code will prompt user
        ORIGINAL_INPUT=$(echo "$INPUT" | jq -c '.tool_input')
        UPDATED_INPUT=$(echo "$ORIGINAL_INPUT" | jq --arg cmd "$REWRITTEN" '.command = $cmd')
        
        jq -n --argjson updated "$UPDATED_INPUT" \
            '{{tool_input: $updated}}'
        ;;
    
    "Ignored"|"Unsupported")
        # Pass through unchanged
        exit 0
        ;;
    
    *)
        # Unknown classification, pass through
        exit 0
        ;;
esac
"#, daemon_url = daemon_url)
}

/// Install the hook script to disk
pub fn install_script(daemon_url: &str) -> Result<std::path::PathBuf> {
    use std::io::Write;
    
    let script = generate_hook_script(daemon_url);
    
    let hook_dir = dirs::data_local_dir()
        .map(|d| d.join("memix").join("hooks"))
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp/memix/hooks"));
    
    std::fs::create_dir_all(&hook_dir)?;
    
    let script_path = hook_dir.join("claude-hook.sh");
    
    let mut file = std::fs::File::create(&script_path)?;
    file.write_all(script.as_bytes())?;
    
    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&script_path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script_path, perms)?;
    }
    
    Ok(script_path)
}
