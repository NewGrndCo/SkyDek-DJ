import { spawnSync } from 'node:child_process';
import { mkdirSync, copyFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const root = fileURLToPath(new URL('../', import.meta.url));
const result = spawnSync('cargo', ['build', '--manifest-path', 'encoder-helper/Cargo.toml', '--release', '--locked'], { cwd: root, stdio: 'inherit', env: {...process.env, CARGO_TARGET_DIR:path.join(root,'encoder-helper/target')} });
if (result.error) throw result.error;
if (result.status !== 0) process.exit(result.status ?? 1);
const output = path.join(root, 'src-tauri/resources/encoder');
mkdirSync(output, { recursive: true });
const binary = process.platform === 'win32' ? 'skydek-mp3-encoder.exe' : 'skydek-mp3-encoder';
copyFileSync(path.join(root, 'encoder-helper/target/release', binary), path.join(output, binary));
console.log('Bundled MP3 encoder prepared.');
