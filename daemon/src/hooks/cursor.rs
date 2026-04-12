//! Cursor hook implementation.
//!
//! Generates the preToolUse hook script for Cursor IDE.

use anyhow::Result;

/// Generate the Cursor hook script
pub fn generate_hook_script(daemon_url: &str) -> String {
    format!(r#"#!/usr/bin/env bash
# memix-hook-version: 1
# Memix Cursor hook - rewrites commands for token optimization.
#
# Cursor uses a similar PreToolUse hook format to Claude Code.
# This script intercepts Bash commands and routes them through Memix.

set -euo pipefail

if ! command -v jq &>/dev/null; then
    exit 0
fi

INPUT=$(cat)
CMD=$(echo "$INPUT" | jq -r '.tool_input.command // .command // empty')

if [ -z "$CMD" ]; then
    exit 0
fi

# Call Memix daemon API
DAEMON_URL="${{DAEMON_URL:-"{daemon_url}}}"
REWRITE_URL="${{DAEMON_URL}}/api/v1/hooks/rewrite"

RESPONSE=$(curl -s -X POST \
    -H "Content-Type: application/json" \
    -d "$(jq -n --arg cmd "$CMD" '{{command: $cmd}}')" \
    "$REWRITE_URL" 2>/dev/null || echo '{{"classification": "Unsupported"}}')

CLASSIFICATION=$(echo "$RESPONSE" | jq -r '.classification')

case "$CLASSIFICATION" in
    "Supported")
        REWRITTEN=$(echo "$RESPONSE" | jq -r '.rewritten')
        [ "$CMD" = "$REWRITTEN" ] && exit 0
        
        # Cursor expects updated_input field
        jq -n --arg cmd "$REWRITTEN" '{{updated_input: {{command: $cmd}}}}'
        ;;
    
    "Ask")
        REWRITTEN=$(echo "$RESPONSE" | jq -r '.rewritten')
        jq -n --arg cmd "$REWRITTEN" '{{updated_input: {{command: $cmd}}}}'
        ;;
    
    *)
        # Return empty object for pass-through
        echo '{{}}'
        ;;
esac
"#, daemon_url = daemon_url)
}

/// Install the hook script
pub fn install_script(daemon_url: &str) -> Result<std::path::PathBuf> {
    use std::io::Write;
    
    let script = generate_hook_script(daemon_url);
    
    let hook_dir = dirs::data_local_dir()
        .map(|d| d.join("memix").join("hooks"))
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp/memix/hooks"));
    
    std::fs::create_dir_all(&hook_dir)?;
    
    let script_path = hook_dir.join("cursor-hook.sh");
    
    let mut file = std::fs::File::create(&script_path)?;
    file.write_all(script.as_bytes())?;
    
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&script_path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script_path, perms)?;
    }
    
    Ok(script_path)
}
