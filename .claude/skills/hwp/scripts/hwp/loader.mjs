// rhwp WASM 로더 — 모든 HWP 생성/편집 스크립트의 공통 진입점.
//
// rhwp WASM 은 `wasm-pack build --target web` 산출물이라 node 에서는 wasm 바이트를
// 직접 주입해야 한다 (node 의 fetch 는 file:// 를 지원하지 않는다).

import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

const PKG = '@dongkseo/rhwp-core';

let _mod = null;

/**
 * glue(js) 와 wasm 바이트의 경로를 찾는다.
 *   1. $RHWP_PKG — pkg/ 디렉터리를 직접 지정한 경우
 *   2. 설치된 @dongkseo/rhwp-core — 보통 이 경로다
 *   3. 상위 폴더의 pkg/ — rhwp 저장소 안에서 개발할 때
 */
function findPkg() {
  if (process.env.RHWP_PKG) {
    const p = resolve(process.env.RHWP_PKG);
    if (!existsSync(join(p, 'rhwp.js'))) throw new Error(`RHWP_PKG=${p} 에 rhwp.js 가 없다.`);
    return { js: join(p, 'rhwp.js'), wasm: join(p, 'rhwp_bg.wasm') };
  }

  // cwd 기준으로 찾아야 스킬 파일이 어디에 있든 소비자의 node_modules 를 본다.
  const req = createRequire(join(process.cwd(), 'noop.js'));
  try {
    return { js: req.resolve(PKG), wasm: req.resolve(`${PKG}/wasm`) };
  } catch { /* 미설치 — 아래로 */ }

  // rhwp 저장소 안에서 개발 중일 때.
  let dir = dirname(fileURLToPath(import.meta.url));
  for (let i = 0; i < 8; i++) {
    const p = join(dir, 'pkg');
    if (existsSync(join(p, 'rhwp.js'))) return { js: join(p, 'rhwp.js'), wasm: join(p, 'rhwp_bg.wasm') };
    dir = dirname(dir);
  }

  throw new Error(
    `${PKG} 가 설치되어 있지 않다. 설치하라:\n\n` +
      `  npm i github:dongkseo/rhwp\n\n` +
      `차트를 만들 거라면 래스터라이저도 함께 설치한다:\n\n` +
      `  npm i @resvg/resvg-js\n\n` +
      `(설치 위치가 특이하면 RHWP_PKG=/path/to/pkg 로 지정한다.)`
  );
}

/** WASM 초기화 (프로세스당 1회). HwpDocument 클래스를 반환한다. */
export async function loadWasm() {
  if (_mod) return _mod;
  const { js, wasm } = findPkg();
  const glue = await import(pathToFileURL(js).href);
  await glue.default({ module_or_path: readFileSync(wasm) });
  _mod = glue;
  return glue;
}

/** 기존 .hwp 파일을 연다. */
export async function openHwp(path) {
  const { HwpDocument } = await loadWasm();
  return new HwpDocument(readFileSync(path));
}

/**
 * 빈 .hwp 문서를 만든다 (blank2010.hwp 내장 템플릿).
 *
 * createEmpty() 만 부르면 Document::default() — 맨바닥 껍데기가 나온다.
 * FileHeader version 이 0.0.0.0 이고 요약정보 스트림이 없어 **한컴/뷰어가 열지 못한다**.
 * char_shapes 도 비어 있어 applyCharFormat 이 0번 글자모양을 덮어쓴다 (범위 무시).
 * createBlankDocument() 가 내장 blank2010.hwp 를 실어 version 5.1.0.1 로 만든다.
 * 두 호출은 반드시 붙어 있어야 한다.
 */
export async function createHwp() {
  const { HwpDocument } = await loadWasm();
  const doc = HwpDocument.createEmpty();
  doc.createBlankDocument();   // ← 이게 없으면 뷰어가 못 여는 파일이 나온다
  return doc;
}

/**
 * 문서 안에서 편집 가능한 텍스트 지점을 모두 찾는다.
 *
 * 양식(블로그/신청서) 문서는 본문 문단이 비어 있고 내용이 **표 안의 표**나 글상자에 있다.
 * 최상위만 훑으면 편집 지점을 하나도 못 찾는다. 이 함수는 cell path 로 깊이 우선 탐색한다.
 *
 * 반환: [{ para, path, text }] — path 는 insertTextInCellByPath 에 그대로 넘긴다.
 *   path 형식: [{controlIndex, cellIndex, cellParaIndex}, ...] (중첩 시 배열이 길어진다)
 */
export function findTextTargets(doc, sec = 0, maxDepth = 3, maxControls = 24) {
  const out = [];

  const walk = (para, prefix, depth) => {
    if (depth > maxDepth) return;
    for (let c = 0; c < maxControls; c++) {
      const probe = [...prefix, { controlIndex: c, cellIndex: 0, cellParaIndex: 0 }];
      let dim = null;
      try {
        const j = depth === 0
          ? doc.getTableDimensions(sec, para, c)
          : doc.getTableDimensionsByPath(sec, para, JSON.stringify(probe));
        if (j) dim = JSON.parse(j);
      } catch { /* 표가 아님 — 글상자일 수 있다 */ }

      // 표가 아닌 컨트롤(글상자 등)도 cell path 로 텍스트가 읽힌다.
      if (!dim) {
        try {
          const t = doc.getTextInCellByPath(sec, para, JSON.stringify(probe), 0, 200);
          if (t && t.trim()) out.push({ para, path: probe, text: t });
        } catch {}
        continue;
      }

      for (let ci = 0; ci < dim.cellCount; ci++) {
        let nPara = 1;
        try { nPara = doc.getCellParagraphCountByPath(sec, para, JSON.stringify([...prefix, { controlIndex: c, cellIndex: ci, cellParaIndex: 0 }])) || 1; } catch {}
        for (let cp = 0; cp < nPara; cp++) {
          const path = [...prefix, { controlIndex: c, cellIndex: ci, cellParaIndex: cp }];
          try {
            const t = doc.getTextInCellByPath(sec, para, JSON.stringify(path), 0, 200);
            if (t && t.trim()) out.push({ para, path, text: t });
          } catch {}
        }
        walk(para, [...prefix, { controlIndex: c, cellIndex: ci, cellParaIndex: 0 }], depth + 1);
      }
    }
  };

  for (let para = 0; para < doc.getParagraphCount(sec); para++) {
    const t = doc.getTextRange(sec, para, 0, 200);
    if (t && t.trim()) out.push({ para, path: null, text: t });   // path=null → 본문 문단
    walk(para, [], 0);
  }
  return out;
}

/** findTextTargets 결과 하나에 텍스트를 넣는다 (본문/셀 자동 분기). */
export function insertAt(doc, target, charOffset, text, sec = 0) {
  return target.path === null
    ? doc.insertText(sec, target.para, charOffset, text)
    : doc.insertTextInCellByPath(sec, target.para, JSON.stringify(target.path), charOffset, text);
}

/**
 * 검증 후 저장한다. 검증 실패 시 파일을 쓰지 않고 throw 한다.
 *
 * exportHwpVerify() 는 어댑터 적용 + 직렬화 + 자기 재로드 검증을 수행하고
 * {bytesLen, pageCountBefore, pageCountAfter, recovered} 를 반환한다.
 * 이 게이트를 통과하지 못한 산출물은 한컴에서 열리지 않을 수 있다.
 */
export async function saveHwp(doc, outPath) {
  const v = JSON.parse(doc.exportHwpVerify());
  if (!v.recovered) {
    throw new Error(`검증 실패: 자기 재로드 불가 — ${JSON.stringify(v)}`);
  }
  if (v.pageCountBefore !== v.pageCountAfter) {
    throw new Error(
      `검증 실패: 페이지 수 불일치 ${v.pageCountBefore} → ${v.pageCountAfter} — ${JSON.stringify(v)}`
    );
  }
  const bytes = doc.exportHwp();
  writeFileSync(outPath, Buffer.from(bytes));
  return { ...v, path: outPath, bytesWritten: bytes.length };
}
