# Intelligent Decision Detection

Memix automatically observes your codebase and records architectural decisions — capturing the **why** behind code evolution, not just the **what**.

## Why Decision Detection Matters

Without decisions, Memix only knows **what changed**, not **why it changed**:

| Without Decisions | With Decisions |
|-------------------|-----------------|
| `package.json` changed | "Added Prisma for database ORM" |
| File created at `src/services/auth.ts` | "Created AuthService using singleton pattern" |
| Config `strict: true` added | "Enabled TypeScript strict mode for type safety" |
| Directory `api/` created | "Organized API routes under /api following Next.js convention" |

## How It Works

### Multi-Signal Detection

The engine processes multiple signal types:

| Signal | Trigger | Example Decision |
|--------|---------|------------------|
| **DependencyAdded** | New package in `package.json` | "Use React for UI framework" |
| **DirectoryCreated** | New directory structure | "Organize components under /components" |
| **FileSaved** | Code changes with AST patterns | "Use Singleton pattern for AuthService" |
| **ConfigChanged** | Config file modifications | "Enable TypeScript strict mode" |
| **EndpointCreated** | New API routes | "Create GET /api/users endpoint" |
| **FileMoved** | File reorganization | "Move utils for better organization" |
| **GitCommit** | Commit with decision keywords | "Migrate to SWR for data fetching" |

### Three Detection Layers

```
┌─────────────────────────────────────────────────────────────────┐
│  Layer 1: TOML Rules (Explicit)                                 │
│  - 70+ pre-defined rules in daemon/rules/decisions.toml        │
│  - Covers common patterns, frameworks, conventions              │
│  - Hot-reloadable without daemon restart                        │
├─────────────────────────────────────────────────────────────────┤
│  Layer 2: AST Pattern Matching                                  │
│  - Tree-sitter queries detect structural patterns               │
│  - Singleton, Repository, Factory, Middleware detection         │
│  - Language-agnostic across 13 supported languages              │
├─────────────────────────────────────────────────────────────────┤
│  Layer 3: Embedding Similarity                                  │
│  - AllMiniLM-L6-v2 vector embeddings                            │
│  - Detects patterns not covered by explicit rules               │
│  - ≥92% similarity threshold for high confidence               │
└─────────────────────────────────────────────────────────────────┘
```

## Configuration

### Rule File Location

Rules are stored in `daemon/rules/decisions.toml` and loaded at daemon startup.

### Rule Structure

```toml
[[rule]]
id = "dep_react"
name = "React UI Framework"
trigger = "dependency_added"
condition = { dependency_pattern = "^(react|react-dom|next|remix|gatsby)$" }
template.title = "Use {name} for UI framework"
template.rationale = "Added {name}@{version} to {file}. This establishes {name} as the primary UI framework..."
template.tags = ["dependency", "ui", "framework"]
template.alternatives = ["Vue", "Svelte", "Angular"]
confidence = 0.95
```

### Placeholders

Available placeholders in templates:

| Placeholder | Description | Example |
|-------------|-------------|---------|
| `{name}` | Dependency or pattern name | `react` |
| `{version}` | Dependency version | `^18.0.0` |
| `{file}` | File path | `package.json` |
| `{path}` | Directory path | `src/components` |
| `{old_path}` | Original path (moves) | `src/utils.ts` |
| `{new_path}` | New path (moves) | `src/lib/utils.ts` |
| `{method}` | HTTP method | `GET` |
| `{key}` | Config key | `compilerOptions.strict` |
| `{value}` | Config value | `true` |

### Trigger Types

| Trigger | Description |
|---------|-------------|
| `dependency_added` | New package added to package.json |
| `directory_created` | New directory created |
| `file_save` | File saved with content changes |
| `config_changed` | Configuration file modified |
| `file_moved` | File moved or renamed |
| `endpoint_created` | API endpoint created |
| `git_commit` | Commit with decision keywords |

### Condition Fields

| Field | Type | Description |
|-------|------|-------------|
| `file_pattern` | Regex | Match file path |
| `path_pattern` | Regex | Match directory path |
| `language` | String | Required language |
| `ast_pattern` | String | Tree-sitter pattern ID |
| `dependency_pattern` | Regex | Match dependency name |
| `file` | Regex | Config file name |
| `key` | String | Config key path |
| `value` | String | Config value |
| `min_confidence` | Float | Minimum confidence threshold |

## User Feedback

### Feedback API

```bash
# Mark decision as useful
curl -X POST http://localhost:9527/api/v1/decisions/decision_dep_react/feedback \
  -H "Content-Type: application/json" \
  -d '{"feedback": "useful"}'

# Dismiss decision
curl -X POST http://localhost:9527/api/v1/decisions/decision_dep_react/feedback \
  -H "Content-Type: application/json" \
  -d '{"feedback": "dismissed"}'

# Mark as incorrect
curl -X POST http://localhost:9527/api/v1/decisions/decision_dep_react/feedback \
  -H "Content-Type: application/json" \
  -d '{"feedback": "incorrect", "comment": "This is a dev dependency only"}'
```

### Self-Improving Confidence

Rules automatically adjust based on feedback:

| Feedback | Confidence Adjustment |
|----------|----------------------|
| `useful` | +0.05 |
| `dismissed` | -0.02 |
| `incorrect` | -0.10 |

Confidence is clamped between 0.0 and 1.0. Rules below 0.60 may stop triggering.

## Cross-Feature Enhancement

Decisions enhance other Memix features:

| Feature | Enhancement |
|---------|-------------|
| **Warnings** | Detect code that contradicts recorded decisions |
| **Prompt Pack** | Include decisions in context for better LLM responses |
| **Code Review** | Flag PRs introducing dependencies conflicting with decisions |
| **Onboarding** | Show new developers architectural decision history |
| **Health Monitor** | Verify code still follows recorded decisions |

## Example Decisions

### Dependency Decision

```json
{
  "id": "decision_dep_prisma",
  "title": "Use prisma for Database ORM",
  "rationale": "Added prisma@^5.0.0 to package.json for Database ORM functionality. This establishes prisma as the standard solution for this concern in the project.",
  "tags": ["dependency", "database", "persistence"],
  "confidence": 0.95,
  "evidence": ["package.json: prisma@^5.0.0"],
  "rule_id": "dep_database",
  "triggered_by": "DependencyAdded"
}
```

### Pattern Decision

```json
{
  "id": "decision_pattern_singleton_authservice",
  "title": "Use Singleton pattern for AuthService",
  "rationale": "Detected singleton pattern implementation. This ensures a single instance for shared resources like authentication state.",
  "tags": ["pattern", "singleton", "creational"],
  "confidence": 0.88,
  "evidence": ["src/services/auth.service.ts: static instance field detected"],
  "rule_id": "pattern_singleton",
  "triggered_by": "FileSave"
}
```

### Embedding-Based Decision

```json
{
  "id": "decision_embedding_repository_1234567890",
  "title": "Repository Pattern detected in user.repository.ts",
  "rationale": "Detected Repository pattern via embedding similarity (94%). Code closely resembles known repository pattern implementations.",
  "tags": ["pattern", "repository", "architecture"],
  "confidence": 0.94,
  "evidence": [
    "File: src/repositories/user.repository.ts",
    "Pattern: Repository (similarity: 0.94)"
  ],
  "rule_id": "embedding_repository",
  "triggered_by": "EmbeddingSimilarity"
}
```

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    FILE WATCHER EVENT                            │
│                  (package.json changed)                          │
└──────────────────────────┬──────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────────┐
│                  DECISION SIGNAL CREATION                        │
│  DecisionSignal::DependencyAdded {                               │
│    name: "prisma", version: "^5.0.0", file: "package.json"       │
│  }                                                               │
└──────────────────────────┬──────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────────┐
│                    RULE MATCHING                                 │
│  - Iterate all rules with trigger=DependencyAdded               │
│  - Evaluate condition.dependency_pattern regex                   │
│  - Rule "dep_database" matches pattern "prisma"                  │
└──────────────────────────┬──────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────────┐
│                  DECISION BUILDING                               │
│  - Fill template with placeholders                               │
│  - Set confidence from rule                                      │
│  - Extract evidence (file, version)                              │
└──────────────────────────┬──────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────────┐
│                  BRAIN STORAGE                                   │
│  MemoryEntry {                                                   │
│    kind: MemoryKind::Decision,                                   │
│    source: MemorySource::AgentExtracted,                         │
│    content: { title, rationale, evidence, ... }                  │
│  }                                                               │
└─────────────────────────────────────────────────────────────────┘
```

## Extending the System

### Adding New Rules

1. Edit `daemon/rules/decisions.toml`
2. Add new `[[rule]]` entry with trigger, condition, and template
3. Restart daemon (or use hot-reload API when available)

### Adding Pattern Embeddings

```rust
// In daemon initialization
decision_detector.add_pattern_embedding(
    PatternReference {
        id: "custom_middleware".to_string(),
        name: "Custom Middleware Pattern".to_string(),
        category: "structural".to_string(),
        description: "Chain-of-responsibility middleware pattern".to_string(),
        example_files: vec!["src/middleware/auth.ts".to_string()],
        decision_tags: vec!["pattern".to_string(), "middleware".to_string()],
        decision_title: "Use Middleware pattern for {file}".to_string(),
    },
    embedding_vector, // 384-dimensional f32 vector
);
```

## Performance

- **Rule matching:** O(R × C) where R = rules, C = condition complexity
- **Embedding search:** O(N × D) where N = patterns, D = 384 dimensions
- **Memory overhead:** ~1KB per decision entry
- **Latency:** < 5ms for rule-based, < 50ms for embedding-based detection
