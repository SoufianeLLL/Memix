// ════════════════════════════════════════════════════════════════════════════════
// GENERATED_DIRS — Directories to exclude from indexing and context compilation
// ════════════════════════════════════════════════════════════════════════════════
//
// These directories contain generated output, dependencies, or cache files that
// should never be indexed or included in structural context. They are noise that
// pollutes the dependency graph, skews importance scores, and wastes tokens.
//
// Used by:
//   - background_indexer.rs: filters files during workspace walking
//   - context/mod.rs: filters stale skeleton entries during compilation
//
// ════════════════════════════════════════════════════════════════════════════════

/// Directories that should be excluded from indexing and context compilation.
/// These are build outputs, dependency folders, caches, and generated code.
/// The paths use forward slashes and are designed to be matched with `contains()`.
pub const GENERATED_DIRS: &[&str] = &[
    // =========================================================================
    // JAVASCRIPT / TYPESCRIPT – Generated outputs only
    // =========================================================================
    "/node_modules/",           // dependencies (never source)
    "/.next/",                  // Next.js build output
    "/dist/",                   // bundled output
    "/build/",                  // compiled output
    "/out/",                    // static export output
    "/.output/",                // Nitro/Nuxt output
    "/.nuxt/",                  // Nuxt generated
    "/.svelte-kit/",            // SvelteKit generated
    "/.astro/",                 // Astro build cache
    "/.solid/",                 // SolidStart generated
    "/.qwik/",                  // Qwik generated
    "/.angular/",               // Angular cache
    "/.vuepress/dist/",         // VuePress output
    "/.vitepress/dist/",        // VitePress output
    "/.docusaurus/",            // Docusaurus build
    "/.gatsby/",                // Gatsby cache
    "/.redwood/",               // RedwoodJS generated
    "/.blitz/",                 // Blitz.js generated
    "/.expo/",                  // Expo build
    "/.turbo/",                 // Turborepo cache
    "/.parcel-cache/",          // Parcel cache
    "/.vite/",                  // Vite cache
    "/.rollup.cache/",          // Rollup cache
    "/.esbuild/",               // esbuild cache
    "/coverage/",               // test coverage output
    "/.nyc_output/",            // nyc coverage
    "/.jest-cache/",            // Jest cache
    "/.vitest/",                // Vitest cache
    "/.playwright/",            // Playwright test artifacts
    "/.cypress/",               // Cypress test artifacts
    "/.storybook-static/",      // Storybook static build
    "/.storybook-dist/",        // Storybook dist
    "/storybook-static/",       // Storybook output
    "/.fusebox/",               // FuseBox cache
    "/.webpack/",               // Webpack cache
    "/.cache/",                 // generic cache
    "/.temp/",                  // temp files
    "/.tmp/",                   // temp files
    
    // =========================================================================
    // PYTHON – Cache and virtual environments only
    // =========================================================================
    "/__pycache__/",            // Python bytecode cache
    "/.pytest_cache/",          // pytest cache
    "/.mypy_cache/",            // mypy type check cache
    "/.ruff_cache/",            // ruff linter cache
    "/.tox/",                   // tox test environments
    "/.nox/",                   // nox sessions
    "/.venv/",                  // Python virtual environment
    "/venv/",                   // Python virtual environment
    "/env/",                    // Python virtual environment
    "/.env/",                   // environment (not .env file)
    "/site-packages/",          // installed packages
    "/.eggs/",                  // setuptools eggs
    "/.egg-info/",              // egg metadata
    "/dist-info/",              // wheel metadata
    "/htmlcov/",                // coverage HTML report
    "/.ipynb_checkpoints/",     // Jupyter checkpoints
    "/.jupyter/",               // Jupyter config/cache
    
    // =========================================================================
    // RUST – Build artifacts only
    // =========================================================================
    "/target/",                 // Cargo build output
    "/target-dir/",             // custom target dir
    "/.cargo/registry/",        // Cargo registry cache
    "/.cargo/git/",             // Cargo git cache
    
    // =========================================================================
    // GO – Build and module cache only
    // =========================================================================
    "/vendor/",                 // Go vendor directory
    "/.bin/",                   // Go bin output
    
    // =========================================================================
    // JAVA / JVM – Build artifacts only
    // =========================================================================
    "/target/",                 // Maven target
    "/build/",                  // Gradle build
    "/out/",                    // IDE output
    "/.gradle/",                // Gradle cache
    "/.mvn/",                   // Maven wrapper
    "/.m2/",                    // Maven local repo
    "/.idea/",                  // IntelliJ IDE
    "/.vscode/",                // VS Code config (not source)
    "/.settings/",              // Eclipse settings
    "/.project/",               // Eclipse project
    "/.classpath/",             // Eclipse classpath
    "/bin/",                    // compiled classes
    
    // =========================================================================
    // C / C++ – Build artifacts only
    // =========================================================================
    "/cmake-build-",            // CMake build directories
    "/CMakeFiles/",             // CMake internal
    "/CMakeCache.txt",          // CMake cache file
    "/.cmake/",                 // CMake cache
    "/.ninja_deps",             // Ninja deps
    "/.ninja_log",              // Ninja log
    "/build/",                  // build output
    "/.objs/",                  // object files
    "/.deps/",                  // dependency files
    "/.dSYM/",                  // macOS debug symbols
    "/.su.o",                   // intermediate objects
    
    // =========================================================================
    // C# / .NET – Build artifacts only
    // =========================================================================
    "/bin/",                    // compiled output
    "/obj/",                    // intermediate files
    "/.vs/",                    // Visual Studio config
    "/packages/",               // NuGet packages
    
    // =========================================================================
    // PHP – Dependencies and cache only
    // =========================================================================
    "/vendor/",                 // Composer dependencies
    "/.phpunit.cache/",         // PHPUnit cache
    "/.phpcs-cache/",           // PHPCS cache
    "/.phpstorm.meta.php",      // PhpStorm metadata
    
    // =========================================================================
    // RUBY – Dependencies and cache only
    // =========================================================================
    "/vendor/bundle/",          // Bundler gems
    "/.gem/",                   // gem files
    "/.ruby-lsp/",              // Ruby LSP cache
    "/.rspec_status/",          // RSpec status
    
    // =========================================================================
    // SWIFT / iOS – Build artifacts only
    // =========================================================================
    "/.build/",                 // SwiftPM build
    "/.swiftpm/",               // SwiftPM cache
    "/DerivedData/",            // Xcode derived data
    "/Pods/",                   // CocoaPods dependencies
    "/Carthage/",               // Carthage dependencies
    
    // =========================================================================
    // DATABASES – Generated migrations/cache only
    // =========================================================================
    "/.prisma/",                // Prisma generated client
    "/.drizzle/",               // Drizzle generated
    "/.sqlx/",                  // SQLx compile-time checks
    "/.sqlc/",                  // sqlc generated
    "/.lancedb/",               // LanceDB vector index
    
    // =========================================================================
    // INFRASTRUCTURE – Cache only
    // =========================================================================
    "/.terraform/",             // Terraform cache
    "/.serverless/",            // Serverless Framework cache
    "/.cdk.staging/",           // CDK staging
    "/.pulumi/",                // Pulumi cache
    "/.chalice/",               // AWS Chalice cache
    "/.sam/",                   // SAM CLI cache
    
    // =========================================================================
    // MONOREPO – Cache only
    // =========================================================================
    "/.changeset/",             // Changeset cache
    "/.yarn/",                  // Yarn cache (but keep .yarnrc.yml)
    "/.pnp.cjs/",               // Yarn PnP cache
    "/.nx/",                    // Nx cache
    "/.rush/",                  // Rush cache
    "/.lage/",                  // Lage cache
    
    // =========================================================================
    // OS / SYSTEM – System garbage only
    // =========================================================================
    "/.DS_Store/",              // macOS Finder metadata
    "/Thumbs.db/",              // Windows thumbnail cache
    "/.Spotlight-V100/",        // Spotlight index
    "/.Trashes/",               // Trash folder
    "/.fseventsd/",             // File system events
    "/.TemporaryItems/",        // Temp files
    
    // =========================================================================
    // MISCELLANEOUS – Cache/temp only
    // =========================================================================
    "/.log/",                   // log files
    "/logs/",                   // log directory
    "/tmp/",                    // temp
    "/temp/",                   // temp
    "/.temp/",                  // temp
    "/.backup/",                // backups
    "/.old/",                   // old versions
    "/.archive/",               // archives
    "/.swp/",                   // vim swap
    "/.swo/",                   // vim swap
    "/.swn/",                   // vim swap
];

/// Check if a path contains any generated directory component.
/// Returns true if the path should be excluded from indexing/compilation.
#[inline]
pub fn is_generated_path(path: &str) -> bool {
    GENERATED_DIRS.iter().any(|dir| path.contains(dir))
}
