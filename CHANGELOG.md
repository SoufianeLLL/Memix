# Change Log

All notable changes to the "memix" extension will be documented in this file.

Check [Keep a Changelog](http://keepachangelog.com/) for recommendations on how to structure this file.

## [1.0.0-beta.1] - 2026-03-05
### Features
- Persistent brain storage with Redis
- Session tracking and continuity
- Health monitoring and smart pruning
- Team Sync (push, pull, merge)
- Import / Export brain data
- Secure credential storage via OS keychain
- IDE auto-detection (VS Code, Cursor, Windsurf, Antigravity)
- Brain Monitor sidebar panel with real-time stats
- Auto-generated rules files for all supported IDEs
- File-based brain sync — AI reads/writes local `.memix/brain/*.json` files, extension auto-syncs to Redis
- Built-in voice commands (save brain, brain status, recap, reboot brain, etc.)
- Persistent Task Tracker — append-only task lists that never get lost across sessions

### Security
- Credentials stored via VS Code SecretStorage API (OS keychain)
- Built-in secret detection blocks API keys, tokens, and passwords from brain data
- No telemetry, no external servers, no MCP dependency
