# Security Policy

## Supported Versions

| Version | Supported |
|---|---|
| 0.1.x | ✅ Currently supported |

## How Memix Handles Security

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
- **No external servers** — All data flows between your VS Code editor and the Redis instance you configure. Memix does not connect to any third-party services.
- **Your data, your control** — Brain data is stored in your Redis instance. You have full control over access, backups, and deletion.

### Network Communication

Memix only communicates with the single Redis endpoint you provide. It does not make HTTP requests, phone home, or contact any other external server.

## Reporting a Vulnerability

If you discover a security vulnerability in Memix, please report it responsibly:

1. **Email**: security@digitalvize.com
2. **Subject**: `[Memix Security] Brief description`
3. **Include**: Steps to reproduce, potential impact, and your suggested fix (if any)

We will acknowledge your report within **48 hours** and provide a detailed response within **7 business days**.

**Please do NOT:**
- Open a public GitHub issue for security vulnerabilities
- Share details publicly before the issue is resolved

We appreciate your help in keeping Memix and its users safe.

---

© 2026 DigitalVize LLC. All rights reserved.
