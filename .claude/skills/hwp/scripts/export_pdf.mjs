#!/usr/bin/env node
// HWP/HWPX -> PDF gate for the hwp skill.
//
// This intentionally routes through rhwp native PDF export. It does not fall
// back to SVG -> PNG -> image PDF because that path can silently bake missing
// CJK fonts into tofu glyphs while still producing a structurally valid PDF.

import { spawnSync } from 'node:child_process';
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  statSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, extname, join, resolve } from 'node:path';

function usage() {
  console.error(`usage: node scripts/export_pdf.mjs <input.hwp|input.hwpx> [options]

options:
  -o, --output <file.pdf>       output PDF path
  -p, --page <0-based-page>     export one page
      --profile <name>          screen|print|high-quality|fast-preview
      --font-path <dir>         pass through to rhwp export-pdf
      --fallback-serif <name>   pass through
      --fallback-sans <name>    pass through
      --fallback-mono <name>    pass through
      --equation-font <name>    pass through
      --text-as-paths           vector paths instead of embedded text fonts
      --rhwp-bin <path>         rhwp binary path (or RHWP_BIN env)
`);
}

const args = process.argv.slice(2);
if (!args.length || args[0] === '-h' || args[0] === '--help') {
  usage();
  process.exit(args.length ? 0 : 64);
}

const input = args[0];
let output = null;
let rhwpBin = process.env.RHWP_BIN || null;
let textAsPaths = false;
const pass = [];

for (let i = 1; i < args.length; i++) {
  const arg = args[i];
  switch (arg) {
    case '-o':
    case '--output':
    case '-p':
    case '--page':
    case '--profile':
    case '--font-path':
    case '--fallback-serif':
    case '--fallback-sans':
    case '--fallback-mono':
    case '--equation-font':
      if (i + 1 >= args.length) {
        console.error(`missing value for ${arg}`);
        process.exit(64);
      }
      if (arg === '-o' || arg === '--output') output = args[i + 1];
      pass.push(arg, args[i + 1]);
      i += 1;
      break;
    case '--text-as-paths':
      textAsPaths = true;
      pass.push(arg);
      break;
    case '--rhwp-bin':
      if (i + 1 >= args.length) {
        console.error('missing value for --rhwp-bin');
        process.exit(64);
      }
      rhwpBin = args[i + 1];
      i += 1;
      break;
    default:
      if (
        arg.startsWith('--fallback-serif=') ||
        arg.startsWith('--fallback-sans=') ||
        arg.startsWith('--fallback-sans-serif=') ||
        arg.startsWith('--fallback-mono=') ||
        arg.startsWith('--fallback-monospace=') ||
        arg.startsWith('--equation-font=') ||
        arg.startsWith('--equation-font-family=')
      ) {
        pass.push(arg);
      } else {
        console.error(`unknown option: ${arg}`);
        process.exit(64);
      }
  }
}

if (!existsSync(input)) {
  console.error(`input not found: ${input}`);
  process.exit(66);
}

if (!output) {
  const ext = extname(input);
  output = `${input.slice(0, ext ? -ext.length : undefined)}.pdf`;
  pass.push('-o', output);
}

function canRun(cmd, probeArgs = ['--version']) {
  const res = spawnSync(cmd, probeArgs, { encoding: 'utf8' });
  return !res.error && res.status === 0;
}

function findRhwp() {
  const candidates = [];
  if (rhwpBin) candidates.push(rhwpBin);
  candidates.push('rhwp');
  candidates.push(resolve('target/release/rhwp'));
  candidates.push(resolve('target/debug/rhwp'));

  for (const candidate of candidates) {
    if (candidate.includes('/') && !existsSync(candidate)) continue;
    if (canRun(candidate)) return candidate;
  }

  console.error(
    'rhwp binary not found. Build/install rhwp native CLI, or set RHWP_BIN=/path/to/rhwp.\n' +
      'Do not fall back to soffice or SVG->PNG image PDF for HWP->PDF.'
  );
  process.exit(69);
}

const bin = findRhwp();
mkdirSync(dirname(output), { recursive: true });

const convert = spawnSync(bin, ['export-pdf', input, ...pass], { stdio: 'inherit' });
if (convert.error) {
  console.error(String(convert.error));
  process.exit(70);
}
if (convert.status !== 0) process.exit(convert.status ?? 70);

if (!existsSync(output) || statSync(output).size < 5) {
  console.error(`PDF was not created: ${output}`);
  process.exit(70);
}

const header = readFileSync(output, { length: 5 }).subarray(0, 5).toString('ascii');
if (header !== '%PDF-') {
  console.error(`invalid PDF header for ${output}`);
  process.exit(70);
}

function commandOutput(cmd, cmdArgs) {
  const res = spawnSync(cmd, cmdArgs, { encoding: 'utf8' });
  if (res.error || res.status !== 0) return null;
  return res.stdout;
}

function textBearingInput() {
  const dir = mkdtempSync(join(tmpdir(), 'rhwp-text-'));
  const res = spawnSync(bin, ['export-text', input, '-o', dir], { encoding: 'utf8' });
  if (res.error || res.status !== 0) return null;
  const files = readdirSync(dir).filter((name) => name.endsWith('.txt'));
  let text = '';
  for (const file of files) text += readFileSync(join(dir, file), 'utf8');
  return /\S/.test(text);
}

const fonts = commandOutput('pdffonts', [output]);
const hasFontRows = fonts
  ? fonts
      .split(/\r?\n/)
      .slice(2)
      .some((line) => line.trim())
  : null;
const hasText = textBearingInput();

if (hasText === true && hasFontRows === false && !textAsPaths) {
  console.error(
    'PDF verification failed: input contains text, but pdffonts found no embedded fonts.\n' +
      'This looks like an image-only PDF, which is not an acceptable automatic HWP->PDF fallback.'
  );
  process.exit(70);
}

if (textAsPaths) {
  console.error('WARN: --text-as-paths was used; PDF text selection/search is unavailable.');
}

console.log(
  JSON.stringify(
    {
      status: 'success',
      input,
      output,
      rhwp: bin,
      bytes: statSync(output).size,
      embedded_fonts_detected: hasFontRows,
      text_detected_in_input: hasText,
    },
    null,
    2
  )
);
