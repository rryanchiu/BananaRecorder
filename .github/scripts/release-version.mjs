// Run only in the release runner; keep development checkout versions unchanged.
import fs from 'node:fs';
const version = process.env.GITHUB_REF_NAME?.replace(/^v/, '');
if (!/^\d+\.\d+\.\d+$/.test(version ?? '')) throw new Error('Expected a stable vX.Y.Z release tag');
for (const file of ['package.json', 'src-tauri/tauri.conf.json']) {
  const data = JSON.parse(fs.readFileSync(file, 'utf8'));
  data.version = version;
  fs.writeFileSync(file, JSON.stringify(data, null, 2) + '\n');
}
const lock = JSON.parse(fs.readFileSync('package-lock.json', 'utf8'));
lock.version = version;
lock.packages[''].version = version;
fs.writeFileSync('package-lock.json', JSON.stringify(lock, null, 2) + '\n');
const cargo = fs.readFileSync('src-tauri/Cargo.toml', 'utf8');
fs.writeFileSync('src-tauri/Cargo.toml', cargo.replace(/(\[package\][\s\S]*?\nversion\s*=\s*)"[^"]+"/, `$1"${version}"`));
console.log(`Release runner version: ${version}`);
