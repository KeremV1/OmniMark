// Prints the npm (production only) section of THIRD-PARTY-NOTICES.md to stdout.
// Usage: node scripts/licenses/npm-notices.mjs [path-to-node_modules]
// Package list and license identifiers come from package-lock.json; license texts are read from
// node_modules (run `npm ci` first, or pass the path of an installed copy).
import { readFileSync, readdirSync, existsSync } from 'node:fs';
import { join } from 'node:path';

const nm = process.argv[2] ?? 'node_modules';
const lock = JSON.parse(readFileSync('package-lock.json', 'utf8'));
const pkgs = Object.entries(lock.packages)
  .filter(([k, v]) => k && !v.dev && !v.devOptional)
  .map(([k, v]) => ({ name: k.replace(/^node_modules\//, ''), version: v.version, license: v.license ?? 'UNKNOWN' }))
  .sort((a, b) => a.name.localeCompare(b.name));

// Dual-licensed packages are used under the permissive option.
const chosen = { dompurify: 'Apache-2.0' };

const out = ['## npm dependencies (production only)', '', 'These are bundled into the user interface of the app.', ''];
for (const p of pkgs) {
  const used = chosen[p.name] ? `${p.license}; used under ${chosen[p.name]}` : p.license;
  out.push(`### ${p.name} ${p.version}`, '', `License: ${used}`, '');
  const dir = join(nm, p.name);
  const file = existsSync(dir) ? readdirSync(dir).find((f) => /^(license|licence|copying)/i.test(f)) : undefined;
  if (file) out.push('```text', readFileSync(join(dir, file), 'utf8').trim(), '```', '');
  else out.push(`(license text: see the package ${p.name} on npmjs.com)`, '');
}
process.stdout.write(out.join('\n'));
