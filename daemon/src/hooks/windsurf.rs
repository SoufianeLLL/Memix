//! Windsurf hook implementation.
//!
//! Generates the .windsurfrules file for prompt-level guidance.
//! Windsurf doesn't support programmatic hooks, so we use rules files.

use anyhow::Result;

/// Generate the Windsurf rules content
pub fn generate_rules_content() -> String {
    r#"# Memix Token Optimization

When executing shell commands, Memix will automatically optimize output to save tokens.

## How it works
- Commands like `git status`, `cargo test`, `npm install` are filtered
- Output is reduced by 60-90% while keeping essential information
- No workflow changes needed - optimization is transparent

## Supported commands
- Git: status, log, diff, branch, tag
- Package managers: npm, yarn, pnpm, cargo, pip, uv
- Build tools: make, cmake, gradle, turbo, nx
- Docker: ps, images, logs
- Testing: pytest, jest, vitest, cargo test, go test
- Linters: eslint, prettier, ruff, biome, shellcheck
- Files: ls, find, tree, grep, rg

## Token savings examples
| Command | Savings |
|---------|---------|
| `git log --stat` | 87% |
| `cargo test` | 90% |
| `npm install` | 80% |
| `docker ps` | 75% |
| `ls -la` | 70% |

## To disable
Remove this file or set `MEMIX_HOOKS_DISABLED=1` in your environment.

## How to use
Just run commands normally. Memix will automatically optimize when possible.

```bash
# These are automatically optimized:
git status
cargo test
npm install
docker ps

# These pass through unchanged (no filter available):
my-custom-tool --flag
```

## Safety
Dangerous commands are blocked:
- `rm -rf /` - Refused
- `curl ... | bash` - Refused (security risk)
- `git push --force` - Prompts for confirmation
"#.to_string()
}

/// Install the rules file
pub fn install_rules() -> Result<std::path::PathBuf> {
    let content = generate_rules_content();
    let path = std::path::Path::new(".windsurfrules");
    
    std::fs::write(path, content)?;
    
    Ok(path.to_path_buf())
}

/// Uninstall the rules file
pub fn uninstall_rules() -> Result<()> {
    let path = std::path::Path::new(".windsurfrules");
    
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    
    Ok(())
}
