import { spawnSync } from 'node:child_process';
import { mkdirSync, copyFileSync, chmodSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const root = fileURLToPath(new URL('../', import.meta.url));
const output = path.join(root, 'src-tauri/resources/encoder');
mkdirSync(output, { recursive: true });
const binary = process.platform === 'win32' ? 'skydek-mp3-encoder.exe' : 'skydek-mp3-encoder';
const targetDir = path.join(root, 'encoder-helper/target');
const run = (program, args) => {
  const result = spawnSync(program, args, { cwd: root, stdio: 'inherit', env: {...process.env, CARGO_TARGET_DIR:targetDir} });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
};
if (process.platform === 'darwin' && process.env.SKYDEK_MAC_UNIVERSAL === '1') {
  const targets = ['aarch64-apple-darwin', 'x86_64-apple-darwin'];
  for (const target of targets) run('cargo', ['build', '--manifest-path', 'encoder-helper/Cargo.toml', '--release', '--locked', '--target', target]);
  run('lipo', ['-create', ...targets.map(target => path.join(targetDir, target, 'release', binary)), '-output', path.join(output, binary)]);
} else {
  run('cargo', ['build', '--manifest-path', 'encoder-helper/Cargo.toml', '--release', '--locked']);
  copyFileSync(path.join(targetDir, 'release', binary), path.join(output, binary));
}
if (process.platform !== 'win32') chmodSync(path.join(output, binary), 0o755);
console.log('Bundled MP3 encoder prepared.');
