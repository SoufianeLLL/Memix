import fs from 'fs';

// Change 'main' to 'master' if your default branch is named master.
const REPO_URL = 'https://github.com/SoufianeLLL/Memix/blob/main';

// 1. Copy and fix README.md
if (fs.existsSync('../README.md')) {
    let readme = fs.readFileSync('../README.md', 'utf8');
    
    // Find all markdown links pointing to 'docs/' or './docs/' and replace 
    // them with absolute GitHub URLs so they work on the Marketplace.
    // E.g., ](docs/FILE.md) -> ](https://github.com/SoufianeLLL/Memix/blob/main/docs/FILE.md)
    readme = readme.replace(/\]\((?:\.\/)?docs\//g, `](${REPO_URL}/docs/`);
    
    // Write it directly into the extension folder
    fs.writeFileSync('README.md', readme);
    console.log('✅ README.md copied and links rewritten for Marketplace.');
}

// 2. Copy LICENSE
if (fs.existsSync('../LICENSE')) {
    fs.copyFileSync('../LICENSE', 'LICENSE');
    console.log('✅ LICENSE copied.');
}

// 3. Copy CHANGELOG (so the Marketplace "Changelog" tab works)
if (fs.existsSync('../docs/CHANGELOG.md')) {
    fs.copyFileSync('../docs/CHANGELOG.md', 'CHANGELOG.md');
    console.log('✅ CHANGELOG.md copied.');
}