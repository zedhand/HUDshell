// Copies the pinned xterm.js files and the app's own web files into web/dist.
// Run with: npm run build   (the Rust host serves web/dist via a virtual host name).
import { cpSync, mkdirSync, rmSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const dist = join(root, 'web', 'dist');
rmSync(dist, { recursive: true, force: true });
mkdirSync(dist, { recursive: true });

const files = [
  ['node_modules/@xterm/xterm/lib/xterm.js', 'xterm.js'],
  ['node_modules/@xterm/xterm/css/xterm.css', 'xterm.css'],
  ['node_modules/@xterm/addon-fit/lib/addon-fit.js', 'addon-fit.js'],
  ['node_modules/@xterm/addon-serialize/lib/addon-serialize.js', 'addon-serialize.js'],
  ['node_modules/@xterm/addon-unicode11/lib/addon-unicode11.js', 'addon-unicode11.js'],
  ['node_modules/@xterm/addon-unicode-graphemes/lib/addon-unicode-graphemes.js', 'addon-unicode-graphemes.js'],
  ['web/index.html', 'index.html'],
  ['web/app.js', 'app.js'],
];
for (const [from, to] of files) {
  cpSync(join(root, from), join(dist, to));
  console.log('copied', to);
}
