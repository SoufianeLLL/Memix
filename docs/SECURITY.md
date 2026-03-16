# Security Policy

## Supported Versions

| Version | Component | Supported |
|---|---|---|
| 1.0.x | Extension | ✅ Currently supported |
| 0.1.x | Daemon | ✅ Currently supported |

## How Memix Handles Security

### Daemon binary storage

The VS Code extension downloads the daemon binary at runtime when needed.

- The daemon binary is stored in the extension's global storage area managed by VS Code.
- It is not written into your workspace folder.
- On macOS and Linux, the binary is marked executable after download.
- Downloads are verified against the published SHA-256 checksum before the binary is used.

### Credential Storage

Memix stores sensitive information (Redis connection URLs, Team IDs) using the **VS Code SecretStorage API**, which delegates to your operating system's native secure credential manager:

- **macOS** — Keychain Access
- **Windows** — Windows Credential Manager
- **Linux** — libsecret (GNOME Keyring / KDE Wallet)

Credentials are **never** written to plaintext configuration files, `settings.json`, or any file on disk.

### Secret Detection

Memix includes a built-in validator that **blocks writes** to the brain if the data contains patterns that look like:

- API keys (e.g., `sk-...`, `AKIA...`)
- GitHub personal access tokens
- Private keys (RSA, EC)
- Passwords in configuration strings
- Bearer tokens
- AWS access keys

This prevents accidental storage of secrets in your project's brain data.

### Data Privacy

- **No telemetry** — Memix does not collect, transmit, or share any usage data, analytics, or error reports.
- **Local-first by default** — Brain data is stored in your Redis instance and local Memix artifacts. You control access, backups, and deletion.
- **Optional external requests exist** — Depending on the feature you use, Memix may contact:
  - the daemon manifest endpoint to download or update the daemon binary
  - the Memix licensing API for license activation and status checks
- **Your code stays local unless you explicitly share context** — Memix does not upload your codebase to Memix-hosted servers as part of normal memory storage.

### Network Communication

Memix communicates with:

- the Redis endpoint you configure for brain storage
- the local daemon over a Unix socket or localhost HTTP
- the published manifest endpoint when the extension checks for daemon updates
- the Memix licensing endpoints if you activate or validate a license

Outside of those flows, Memix does not send project contents to a Memix-hosted cloud service.

## Reporting a Vulnerability

If you discover a security vulnerability in Memix, please report it responsibly:

1. **Email**: support@memix.dev
2. **Subject**: `[Memix Security] Brief description`
3. **Include**: Steps to reproduce, potential impact, and your suggested fix (if any)

We will acknowledge your report within **48 hours** and provide a detailed response within **7 business days**.

**Please do NOT:**
- Open a public GitHub issue for security vulnerabilities
- Share details publicly before the issue is resolved

We appreciate your help in keeping Memix and its users safe.

---

© 2026 DigitalVize LLC. All rights reserved.
