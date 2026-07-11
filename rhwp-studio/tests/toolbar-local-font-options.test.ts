import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const source = readFileSync(new URL('../src/ui/toolbar.ts', import.meta.url), 'utf8');

test('로컬 글꼴 option은 문서 초기화가 아니라 글꼴 목록을 열 때 준비한다', () => {
  const initStart = source.indexOf('initFontDropdown(docFonts?: string[]): void');
  const initEnd = source.indexOf('private refreshFontDropdown()', initStart);
  const initMethod = source.slice(initStart, initEnd);

  assert.match(source, /localFontOptionsPrepared = false/);
  assert.match(source, /fontName\.addEventListener\('pointerdown', \(\) => this\.populateLocalFontOptions\(\)\)/);
  assert.match(source, /event\.key === 'ArrowDown'/);
  assert.doesNotMatch(initMethod, /this\.populateLocalFontOptions\(\)/);
});

test('로컬 글꼴 option 생성은 한 번만 수행하고 fragment로 묶는다', () => {
  assert.match(source, /if \(this\.localFontOptionsPrepared\) return;/);
  assert.match(source, /this\.localFontOptionsPrepared = true;/);
  assert.match(source, /const options = document\.createDocumentFragment\(\)/);
  assert.match(source, /options\.appendChild\(opt\)/);
  assert.match(source, /group\.appendChild\(options\)/);
});
