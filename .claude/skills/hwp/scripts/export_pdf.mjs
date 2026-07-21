#!/usr/bin/env node
// HWP/HWPX/HML -> PDF gate for the hwp skill.
//
// This script is intentionally self-contained at the skill/runtime boundary:
// it uses the bundled rhwp WASM package and never shells out to the native
// `rhwp` CLI. If the WASM package is too old to expose PDF export, fail loudly
// instead of silently depending on a host binary.

import { existsSync, mkdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { dirname, extname, join, resolve } from 'node:path';
import { openHwp } from './hwp/loader.mjs';

function usage() {
  console.error(`usage: node scripts/export_pdf.mjs <input.hwp|input.hwpx|input.hml> [options]

Options:
  -o, --output <file>      output PDF file (default: output/<input>.pdf)
  -p, --page <n>           export one 0-based page
      --profile <name>     screen|print|high-quality|fast-preview
      --text-as-paths      accepted for CLI parity; WASM PDF uses this mode

Unsupported in self-contained WASM export:
      --rhwp-bin, --font-path, --fallback-*, --equation-font, --embed-text`);
}

const args = process.argv.slice(2);
if (args.length === 0 || args[0] === '-h' || args[0] === '--help') {
  usage();
  process.exit(args.length === 0 ? 64 : 0);
}

const input = resolve(args[0]);
let output = '';
let page = null;
let profile = null;
let textAsPaths = true;

function requireValue(arg, i) {
  if (i + 1 >= args.length) {
    console.error(`missing value for ${arg}`);
    process.exit(64);
  }
  return args[i + 1];
}

for (let i = 1; i < args.length; ) {
  const arg = args[i];
  switch (arg) {
    case '-o':
    case '--output':
      output = resolve(requireValue(arg, i));
      i += 2;
      break;
    case '-p':
    case '--page': {
      const raw = requireValue(arg, i);
      page = Number.parseInt(raw, 10);
      if (!Number.isInteger(page) || page < 0) {
        console.error(`invalid page: ${raw}`);
        process.exit(64);
      }
      i += 2;
      break;
    }
    case '--profile':
      profile = requireValue(arg, i);
      i += 2;
      break;
    case '--text-as-paths':
      textAsPaths = true;
      i += 1;
      break;
    case '--embed-text':
    case '--rhwp-bin':
    case '--font-path':
    case '--fallback-serif':
    case '--fallback-sans':
    case '--fallback-sans-serif':
    case '--fallback-mono':
    case '--fallback-monospace':
    case '--equation-font':
    case '--equation-font-family':
      console.error(`${arg} is not supported by self-contained WASM PDF export`);
      process.exit(64);
      break;
    default:
      if (
        arg.startsWith('--fallback-serif=') ||
        arg.startsWith('--fallback-sans=') ||
        arg.startsWith('--fallback-sans-serif=') ||
        arg.startsWith('--fallback-mono=') ||
        arg.startsWith('--fallback-monospace=') ||
        arg.startsWith('--equation-font=') ||
        arg.startsWith('--equation-font-family=') ||
        arg.startsWith('--rhwp-bin=')
      ) {
        console.error(`${arg.split('=')[0]} is not supported by self-contained WASM PDF export`);
        process.exit(64);
      }
      console.error(`unknown option: ${arg}`);
      usage();
      process.exit(64);
  }
}

if (!existsSync(input)) {
  console.error(`input not found: ${input}`);
  process.exit(66);
}

if (!output) {
  const stem = input.slice(0, input.length - extname(input).length).split(/[\\/]/).pop() || 'output';
  output = resolve(join('output', `${stem}.pdf`));
}

const doc = await openHwp(input);
if (typeof doc.exportPdf !== 'function' || typeof doc.exportPagePdf !== 'function') {
  throw new Error(
    'rhwp WASM package does not expose exportPdf/exportPagePdf. ' +
      'Rebuild or update @dongkseo/rhwp-core; native rhwp CLI fallback is intentionally disabled.'
  );
}

const pageCount = doc.pageCount();
if (page !== null && page >= pageCount) {
  console.error(`page out of range: ${page} (valid: 0..${Math.max(0, pageCount - 1)})`);
  process.exit(65);
}

const pdfBytes =
  page === null
    ? profile
      ? doc.exportPdfWithProfile(profile, textAsPaths)
      : doc.exportPdf(textAsPaths)
    : profile
      ? doc.exportPagePdfWithProfile(page, profile, textAsPaths)
      : doc.exportPagePdf(page, textAsPaths);

mkdirSync(dirname(output), { recursive: true });
writeFileSync(output, Buffer.from(pdfBytes));

const header = readFileSync(output).subarray(0, 5).toString('utf8');
if (!header.startsWith('%PDF-')) {
  console.error(`invalid PDF header for ${output}`);
  process.exit(70);
}

console.log(
  JSON.stringify(
    {
      status: 'success',
      engine: 'rhwp-wasm',
      input,
      output,
      bytes: statSync(output).size,
      page_count: pageCount,
      exported_pages: page === null ? pageCount : 1,
      text_as_paths: textAsPaths,
      profile,
    },
    null,
    2
  )
);
