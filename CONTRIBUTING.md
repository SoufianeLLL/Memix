# Contributing to Memix

First off, thank you for considering contributing to Memix! It's people like you that make Memix such a powerful tool for the community.

Memix is an open-core project. We welcome contributions to both our open-source core (MIT) and our commercial features (BSL 1.1). By participating in this project, you agree to abide by our [Code of Conduct](CODE_OF_CONDUCT.md).

## Project Structure & Architecture

Memix is composed of two main layers:

1. **The Rust Daemon (`./daemon`)**: The local intelligence layer that manages the Redis brain, builds AST metadata, and executes vector searches.
2. **The VS Code Extension (`./extension`)**: The TypeScript layer that bridges the IDE to the Rust daemon.

### Open Core Licensing

- **MIT License:** The core Rust daemon, single-player features, and the VS Code extension are fully open-source.
- **BSL 1.1:** The team-sync module and commercial features (primarily within `./daemon/src/sync/team.rs` and related enterprise logic) are source-available but require a commercial license for production use.

## Local Development Setup

### 1. The Rust Daemon

To work on the daemon, you will need [Rust and Cargo installed](https://rustup.rs/).

```bash
cd ./daemon
# Run tests
cargo test
# Build the daemon
cargo build
```

*Note: You must have a local Redis server running on `127.0.0.1:6379` for most daemon features to operate correctly.*

### 2. The VS Code Extension

To work on the extension UI or IDE integration:

```bash
cd ./extension
npm install
npm run compile
```

To test your changes, open the `./extension` folder in VS Code and press **F5**. This will launch an Extension Development Host with your modified Memix extension loaded.

#### Developing with an External Daemon

If you are simultaneously working on the Rust daemon and the VS Code extension, you do not need to rebuild the release binary every time. You can run the daemon locally via `cargo run` and tell the extension to connect to it by setting this environment variable:

```bash
MEMIX_DEV_EXTERNAL_DAEMON=true
```

This prevents the extension from spawning its own bundled daemon and instead connects to your locally running instance.

## How to Contribute

### Reporting Bugs

We use GitHub issues to track public bugs. Before filing a new issue, please ensure it hasn't already been reported.

A great bug report includes:
- Your OS and VS Code version
- The specific version of Memix
- Concrete steps to reproduce the issue
- Any relevant logs from the "Memix Panel" output channel in VS Code

### Suggesting Enhancements

Have an idea for a new code observer, DNA rule, or AI workflow? We'd love to hear it. Open an issue and use the **Enhancement** label. Provide as much context as possible on why the feature would be broadly useful.

### Pull Requests

1. Fork the repository and create your branch from `main`.
2. If you've added code that should be tested, add unit or integration tests.
3. Ensure the test suite passes (`cargo test` for Rust, `npm run test` for TS).
4. Update any relevant documentation.
5. Issue the pull request!

## Styleguide

### Commit Messages

We follow [Conventional Commits](https://www.conventionalcommits.org/):

- `feat:` for new features
- `fix:` for bug fixes
- `docs:` for documentation changes
- `refactor:` for code refactoring
- `chore:` for routine tasks, dependency updates, etc.

*Example:* `feat(daemon): add support for Python AST parsing`

## Getting Help

If you need help setting up your environment or understanding the architecture, feel free to open a Discussion on GitHub or reach out to us at <support@memix.dev>.

Thank you for helping us build the ultimate AI memory bridge!
