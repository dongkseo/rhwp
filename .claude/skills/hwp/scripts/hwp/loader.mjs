// rhwp WASM 로더 — 모든 HWP 생성/편집 스크립트의 공통 진입점.
//
// pkg/ 는 `wasm-pack build --target web` 산출물이라 node 에서는 wasm 바이트를
// 직접 주입해야 한다 (node 의 fetch 는 file:// 를 지원하지 않는다).

import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

let _mod = null;

/** pkg/ 디렉터리를 찾는다: $RHWP_PKG → 스킬 위치에서 상위로 탐색 */
function findPkg() {
  if (process.env.RHWP_PKG) return resolve(process.env.RHWP_PKG);
  let dir = dirname(fileURLToPath(import.meta.url));
  for (let i = 0; i < 8; i++) {
    const p = join(dir, 'pkg');
    if (existsSync(join(p, 'rhwp.js'))) return p;
    dir = dirname(dir);
  }
  throw new Error(
    'pkg/ 를 찾을 수 없다. 저장소 루트에서 WASM 을 빌드하라:\n' +
      '  cp .env.docker.example .env.docker   # 최초 1회 (내용 수정 금지 — UID/GID 는 1000 그대로)\n' +
      '  docker compose --env-file .env.docker run --rm wasm\n' +
      '또는 RHWP_PKG=/path/to/pkg 로 지정하라.'
  );
}

/** WASM 초기화 (프로세스당 1회). HwpDocument 클래스를 반환한다. */
export async function loadWasm() {
  if (_mod) return _mod;
  const pkg = findPkg();
  const glue = await import(pathToFileURL(join(pkg, 'rhwp.js')).href);
  await glue.default({ module_or_path: readFileSync(join(pkg, 'rhwp_bg.wasm')) });
  _mod = glue;
  return glue;
}

/** 기존 .hwp 파일을 연다. */
export async function openHwp(path) {
  const { HwpDocument } = await loadWasm();
  return new HwpDocument(readFileSync(path));
}

/** 빈 .hwp 문서를 만든다 (blank2010.hwp 내장 템플릿). */
export async function createHwp() {
  const { HwpDocument } = await loadWasm();
  return HwpDocument.createEmpty();
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
