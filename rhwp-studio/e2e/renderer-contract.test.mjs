import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const studioRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const canvaskitPath = path.join(studioRoot, 'src/view/canvaskit-renderer.ts');
const canvaskitDirectory = path.join(studioRoot, 'src/view/canvaskit');
const layerTypesPath = path.join(studioRoot, 'src/core/types.ts');

const canvaskitSource = fs.readFileSync(canvaskitPath, 'utf8');
const layerTypesSource = fs.readFileSync(layerTypesPath, 'utf8');

function extractBlockBody(source, signatureIndex, blockName) {
  let bodyStart = -1;
  for (let index = signatureIndex; index < source.length; index += 1) {
    if (source[index] === '{') {
      bodyStart = index;
      break;
    }
  }
  assert.notEqual(bodyStart, -1, `missing body for ${blockName}`);

  let depth = 0;
  for (let index = bodyStart; index < source.length; index += 1) {
    const char = source[index];
    if (char === '{') {
      depth += 1;
    } else if (char === '}') {
      depth -= 1;
      if (depth === 0) {
        return source.slice(bodyStart + 1, index);
      }
    }
  }

  throw new Error(`unterminated body for ${blockName}`);
}

function extractMethodBody(source, methodName) {
  let signatureIndex = source.indexOf(`private ${methodName}(`);
  if (signatureIndex === -1) {
    signatureIndex = source.indexOf(`${methodName}(`);
  }
  assert.notEqual(signatureIndex, -1, `missing method ${methodName}`);

  return extractBlockBody(source, signatureIndex, methodName);
}

function caseLabels(methodBody) {
  return [...methodBody.matchAll(/case\s+'([^']+)':/g)].map((match) => match[1]);
}

function tsFilesUnder(directory) {
  return fs.readdirSync(directory, { withFileTypes: true })
    .flatMap((entry) => {
      const entryPath = path.join(directory, entry.name);
      if (entry.isDirectory()) {
        return tsFilesUnder(entryPath);
      }
      return entry.name.endsWith('.ts') ? [entryPath] : [];
    })
    .sort();
}

function layerPaintOpTypes() {
  const unionMatch = layerTypesSource.match(/export type LayerPaintOp =([\s\S]*?);/);
  assert.notEqual(unionMatch, null, 'missing LayerPaintOp union');
  const interfaceNames = [...unionMatch[1].matchAll(/\|\s*(Layer[A-Za-z0-9]+Op)\b/g)]
    .map((match) => match[1]);
  assert.ok(interfaceNames.length > 0, 'LayerPaintOp union has no variants');

  return interfaceNames.map((interfaceName) => {
    const interfacePattern = new RegExp(`export interface ${interfaceName} \\{[\\s\\S]*?type:\\s*'([^']+)'`);
    const interfaceMatch = layerTypesSource.match(interfacePattern);
    assert.notEqual(interfaceMatch, null, `missing literal type for ${interfaceName}`);
    return interfaceMatch[1];
  }).sort();
}

const renderOpBody = extractMethodBody(canvaskitSource, 'renderOp');
const renderOpCases = caseLabels(renderOpBody).sort();
const layerOpTypes = layerPaintOpTypes();
const canvaskitSourceFiles = [
  { label: path.relative(studioRoot, canvaskitPath), source: canvaskitSource },
  ...tsFilesUnder(canvaskitDirectory).map((filePath) => ({
    label: path.relative(studioRoot, filePath),
    source: fs.readFileSync(filePath, 'utf8'),
  })),
];
const forbiddenCanvas2dApiPatterns = [
  [/document\s*\.\s*createElement\b/, 'document.createElement'],
  [/\.getContext\s*\(/, 'HTMLCanvasElement.getContext'],
  [/\bCanvasRenderingContext2D\b/, 'CanvasRenderingContext2D'],
  [/\bPath2D\b/, 'Path2D'],
  [/\.measureText\s*\(/, 'CanvasRenderingContext2D.measureText'],
  [/\bOffscreenCanvas\b/, 'OffscreenCanvas'],
  [/\bImageData\b/, 'ImageData'],
  [/\bcreateImageBitmap\s*\(/, 'createImageBitmap'],
  [/\bImageBitmap\b/, 'ImageBitmap'],
  [/\bHTMLImageElement\b/, 'HTMLImageElement'],
  [/\bnew\s+Image\s*\(/, 'new Image'],
  [/\bDOMParser\b/, 'DOMParser'],
  [/\bXMLSerializer\b/, 'XMLSerializer'],
  [/\bURL\s*\.\s*createObjectURL\s*\(/, 'URL.createObjectURL'],
  [/\bFileReader\b/, 'FileReader'],
  [/\bCanvas2DLayerRenderer\b/, 'Canvas2DLayerRenderer'],
  [/canvas2d-layer-renderer/, 'canvas2d-layer-renderer import'],
];

assert.deepEqual(
  renderOpCases,
  layerOpTypes,
  'CanvasKit renderOp must explicitly mention every LayerPaintOp variant',
);

const directReplayOps = [
  'ellipse',
  'footnoteMarker',
  'formObject',
  'image',
  'line',
  'pageBackground',
  'path',
  'placeholder',
  'rectangle',
  'textRun',
];
const textRunFallbackOps = [
  'charOverlap',
  'equation',
  'glyphRun',
  'rawSvg',
  'tabLeader',
  'textControlMark',
  'textDecoration',
];

for (const op of directReplayOps) {
  assert.match(
    renderOpBody,
    new RegExp(`case '${op}':[\\s\\S]*?this\\.render[A-Za-z0-9]+\\(canvas,`),
    `${op} should dispatch to a CanvasKit replay method`,
  );
}

for (const op of textRunFallbackOps) {
  assert.match(
    renderOpBody,
    new RegExp(`case '${op}':[\\s\\S]*?this\\.unsupportedOps\\.add\\(op\\.type\\);[\\s\\S]*?return;`),
    `${op} should stay on the declared unsupported/TextRun fallback path`,
  );
}

assert.match(
  renderOpBody,
  /case 'glyphOutline':[\s\S]*?glyphOutlinePayloadStatus\(op,[\s\S]*?this\.renderGlyphOutline\(canvas, op\);[\s\S]*?this\.unsupportedOps\.add\(/,
  'glyphOutline should stay guarded by payload status before direct replay',
);

for (const { label, source } of canvaskitSourceFiles) {
  for (const [pattern, name] of forbiddenCanvas2dApiPatterns) {
    assert.doesNotMatch(
      source,
      pattern,
      `CanvasKit direct replay source ${label} must not depend on ${name}`,
    );
  }
}

console.log('renderer backend contract guard passed');
