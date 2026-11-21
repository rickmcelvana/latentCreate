#!/usr/bin/env node
// Generates THIRD-PARTY-LICENSES.md (T-509): every Rust crate and npm package
// the shipped app is built from, with the licence texts their published
// packages carry, plus the licences of the curated models the app can install
// on request (none are bundled).
//
//   node scripts/third-party-licenses.mjs           write the file
//   node scripts/third-party-licenses.mjs --check   fail if it is stale
//
// No dependencies: it reads `cargo metadata`, package-lock.json, node_modules
// and profiles/*.json. Only *normal* dependencies are walked -- dev and build
// dependencies are not in the binary. Every target platform is included, so
// one file serves every installer.

import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const OUT = join(ROOT, 'THIRD-PARTY-LICENSES.md');
const ROOT_CRATE = 'app';
const LICENSE_FILE = /^(licen[cs]e|copying|notice|unlicense)([-._].*)?$/i;

const normalize = (s) => s.replace(/\r\n?/g, '\n').replace(/[ \t]+$/gm, '').trim();

function licenseFiles(dir) {
  if (!existsSync(dir)) return [];
  return readdirSync(dir, { withFileTypes: true })
    .filter((e) => e.isFile() && LICENSE_FILE.test(e.name))
    .map((e) => e.name)
    .sort()
    .map((name) => ({ name, text: normalize(readFileSync(join(dir, name), 'utf8')) }))
    .filter((f) => f.text.length > 0);
}

function rustPackages() {
  const raw = execFileSync('cargo', ['metadata', '--format-version', '1', '--locked'], {
    cwd: ROOT,
    encoding: 'utf8',
    maxBuffer: 256 * 1024 * 1024,
  });
  const meta = JSON.parse(raw);
  const byId = new Map(meta.packages.map((p) => [p.id, p]));
  const nodes = new Map(meta.resolve.nodes.map((n) => [n.id, n]));
  const workspace = new Set(meta.workspace_members);
  const root = meta.packages.find((p) => p.name === ROOT_CRATE && workspace.has(p.id));
  if (!root) throw new Error(`workspace crate "${ROOT_CRATE}" not found`);

  const seen = new Set();
  const stack = [root.id];
  while (stack.length > 0) {
    const id = stack.pop();
    if (seen.has(id)) continue;
    seen.add(id);
    for (const dep of nodes.get(id).deps) {
      if (dep.dep_kinds.some((k) => k.kind === null)) stack.push(dep.pkg);
    }
  }
  return [...seen]
    .filter((id) => !workspace.has(id))
    .map((id) => byId.get(id))
    .map((p) => {
      const dir = dirname(p.manifest_path);
      const files = licenseFiles(dir);
      if (p.license_file && !files.some((f) => f.name === p.license_file)) {
        const path = join(dir, p.license_file);
        if (existsSync(path)) {
          files.push({ name: p.license_file, text: normalize(readFileSync(path, 'utf8')) });
        }
      }
      return {
        name: p.name,
        version: p.version,
        license: p.license ?? `see ${p.license_file ?? 'package'}`,
        url: p.repository ?? `https://crates.io/crates/${p.name}`,
        files,
      };
    });
}

function npmPackages() {
  const lock = JSON.parse(readFileSync(join(ROOT, 'package-lock.json'), 'utf8'));
  return Object.entries(lock.packages)
    .filter(([path, info]) => path.includes('node_modules/') && !info.link && !info.dev && !info.devOptional)
    .map(([path]) => {
      const dir = join(ROOT, path);
      const pkg = JSON.parse(readFileSync(join(dir, 'package.json'), 'utf8'));
      const repo = typeof pkg.repository === 'string' ? pkg.repository : pkg.repository?.url;
      return {
        name: pkg.name,
        version: pkg.version,
        license: typeof pkg.license === 'string' ? pkg.license : (pkg.license?.type ?? 'see package'),
        url: (repo ?? `https://www.npmjs.com/package/${pkg.name}`).replace(/^git\+/, '').replace(/\.git$/, ''),
        files: licenseFiles(dir),
      };
    });
}

function models() {
  const dir = join(ROOT, 'profiles');
  return readdirSync(dir)
    .filter((f) => f.endsWith('.json'))
    .sort()
    .map((f) => JSON.parse(readFileSync(join(dir, f), 'utf8')))
    .map((p) => {
      const repos = new Set(
        (p.comfy?.models ?? [])
          .map((m) => /^https:\/\/huggingface\.co\/([^/]+\/[^/]+)\//.exec(m.source_url ?? ''))
          .filter(Boolean)
          .map((m) => `https://huggingface.co/${m[1]}`),
      );
      return { name: p.display_name, id: p.id, license: p.license, notes: p.license_notes, repos: [...repos] };
    });
}

const byName = (a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version, undefined, { numeric: true });

// Identical texts are printed once, with every package that carries them.
function groupTexts(packages) {
  const groups = new Map();
  for (const p of packages) {
    for (const f of p.files) {
      const key = createHash('sha256').update(f.text).digest('hex');
      if (!groups.has(key)) groups.set(key, { text: f.text, users: [] });
      groups.get(key).users.push(`${p.name} ${p.version} (${f.name})`);
    }
  }
  return [...groups.values()].sort((a, b) => a.users[0].localeCompare(b.users[0]));
}

function fence(text) {
  let ticks = '~~~';
  while (text.includes(ticks)) ticks += '~';
  return `${ticks}text\n${text}\n${ticks}`;
}

function render() {
  const rust = rustPackages().sort(byName);
  const npm = npmPackages().sort(byName);
  const all = [...rust, ...npm];
  const out = [];
  out.push('# Third-party licences');
  out.push('');
  out.push('<!-- Generated by scripts/third-party-licenses.mjs. Do not edit by hand: run');
  out.push('     `npm run licenses`. `npm run gate` fails when this file is stale. -->');
  out.push('');
  out.push('latentCreate is licensed under Apache-2.0 (see LICENSE and NOTICE). The application is built');
  out.push(`from the ${rust.length} Rust crates and ${npm.length} npm packages listed below, each under its own licence.`);
  out.push('Where a package offers a choice of licences (`MIT OR Apache-2.0`), latentCreate uses it under');
  out.push('the permissive option. A handful of crates pulled in by Tauri itself (`cssparser`, `selectors`,');
  out.push('`dtoa-short`, `option-ext`) are under the file-level MPL-2.0; they are used unmodified and their');
  out.push('source is available from crates.io at the versions listed.');
  out.push('');
  out.push('## Models the app can install');
  out.push('');
  out.push('**No model is bundled with latentCreate.** Setup offers to download these into your own ComfyUI,');
  out.push('only when you click Install. Each carries its own licence, which you accept with the download;');
  out.push('the app shows it wherever the model is chosen or installed.');
  out.push('');
  out.push('| Model | Licence | Terms in brief | Source |');
  out.push('|---|---|---|---|');
  for (const m of models()) {
    out.push(`| ${m.name} | ${m.license} | ${m.notes ?? ''} | ${m.repos.map((r) => `<${r}>`).join('<br>')} |`);
  }
  out.push('');
  for (const [title, list] of [
    ['Rust crates', rust],
    ['npm packages', npm],
  ]) {
    out.push(`## ${title}`);
    out.push('');
    out.push('| Package | Version | Licence | Source |');
    out.push('|---|---|---|---|');
    for (const p of list) {
      const note = p.files.length === 0 ? ' (no licence file in the published package)' : '';
      out.push(`| ${p.name} | ${p.version} | ${p.license}${note} | <${p.url}> |`);
    }
    out.push('');
  }
  out.push('## Licence texts');
  out.push('');
  out.push('Each text appears once, followed by the packages whose published source carries it. A package');
  out.push('marked "no licence file in the published package" is covered by the standard text of the');
  out.push('licence it names, which appears below under another package.');
  out.push('');
  for (const g of groupTexts(all)) {
    out.push(`### ${g.users[0]}${g.users.length > 1 ? ` and ${g.users.length - 1} more` : ''}`);
    out.push('');
    out.push(`Used by: ${g.users.join(', ')}`);
    out.push('');
    out.push(fence(g.text));
    out.push('');
  }
  return out.join('\n');
}

const text = render();
if (process.argv.includes('--check')) {
  const current = existsSync(OUT) ? normalize(readFileSync(OUT, 'utf8')) : '';
  if (current !== normalize(text)) {
    console.error('THIRD-PARTY-LICENSES.md is stale: run `npm run licenses` and commit the result.');
    process.exit(1);
  }
  console.log('THIRD-PARTY-LICENSES.md is up to date.');
} else {
  writeFileSync(OUT, `${text}\n`);
  console.log(`Wrote ${OUT}`);
}
