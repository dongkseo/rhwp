---
name: hwp
description: "Use this skill any time a HWP document (한글 문서) is the primary input or output. This means any task where the user wants to: open, read, edit, or fix an existing .hwp file (e.g., replacing text, filling templates, editing tables, applying formatting, inserting images); create a new .hwp from scratch or from other content; or extract text/tables/structure from a .hwp. Trigger especially when the user references a .hwp file by name or path — even casually (like \"내려받은 한글 파일\", \"the hwp in my downloads\") — and wants something done to it or produced from it. The deliverable must be a .hwp file or its contents. Do NOT trigger when the primary deliverable is a Word/Excel/PDF file or an HTML report; when the task is CLI-based rendering, layout debugging, or IR diffing (use the rhwp-cli skill); or when converting exam papers into HWPX (use the rhwp-exam-ingest skill)."
license: 저장소 라이선스(MIT)를 따른다.
---

# 산출물 요구사항

## 모든 HWP 파일

### 손상 0 (ZERO corruption)
- 모든 산출 `.hwp`는 **한컴오피스에서 경고 없이 열려야** 한다. xlsx가 "수식 오류 0"을 강제하듯, HWP는 "손상 0"을 강제한다.
- 저장 전 반드시 `exportHwpVerify()` 게이트를 통과시킨다. 이 검증은 선택이 아니다.

### 일관된 글꼴
- 별도 지시가 없으면 한컴 표준 글꼴(함초롬바탕/함초롬돋움)을 일관되게 사용한다.

### 기존 템플릿 보존 (편집 시)
- 기존 `.hwp`를 수정할 때는 원본의 서식·스타일·관례를 **그대로 유지**하고 표준 서식을 강요하지 않는다.
- 기존 템플릿의 관례가 이 가이드보다 **항상 우선**한다.
- HWP 출처 문서는 내보내기 시 어댑터가 no-op이므로 원본 구조가 보존된다.

# HWP 생성 · 편집 · 분석

## Overview

사용자가 `.hwp` 파일을 만들거나, 편집하거나, 내용을 추출하도록 돕는다. 모든 작업은 node에서 rhwp WASM 모듈의 `HwpDocument` 클래스로 수행한다.

기본 흐름은 항상 같다: **열기(또는 생성) → 편집 → 검증 → 저장**.

## 중요 전제 조건

**WASM 빌드 필요**: 이 스킬은 저장소 루트의 `pkg/`(rhwp WASM 산출물)를 사용한다. 없으면 Docker로 빌드한다.

```bash
cp .env.docker.example .env.docker            # 최초 1회. 내용을 수정하지 마라 — UID/GID는 1000 그대로 둔다
docker compose --env-file .env.docker run --rm wasm   # → pkg/ 생성 (수 분 소요)
```

> `.env.docker`의 UID/GID를 호스트 실제 값으로 바꾸지 마라. macOS의 staff GID(20)는 컨테이너 Ubuntu의 `dialout`과 충돌해 이미지 빌드가 실패한다.

Docker 데몬이 없다는 오류가 나면 데몬부터 띄운다 (colima 사용 시 `colima start`). Docker는 **WASM 빌드 전용**이며 네이티브 빌드·테스트에는 쓰지 않는다.

`pkg/`가 저장소 루트가 아닌 곳에 있으면 `RHWP_PKG=/path/to/pkg`로 지정한다.

## CRITICAL: 검증 없이 저장하지 마라

`exportHwp()`는 바이트를 돌려줄 뿐, 그 결과가 한컴에서 열리는지는 보장하지 않는다. openpyxl이 계산되지 않은 수식을 담은 채 저장되는 것과 같다. **반드시 `exportHwpVerify()`로 자기 재로드 검증을 통과시킨 뒤 저장한다.**

### ❌ 잘못된 방법 — 검증 없이 저장
```js
doc.insertText(0, 0, 0, '내용');
writeFileSync('out.hwp', Buffer.from(doc.exportHwp()));  // 손상 여부 미확정
```

### ✅ 올바른 방법 — 검증 후 저장
```js
doc.insertText(0, 0, 0, '내용');
const v = JSON.parse(doc.exportHwpVerify());
if (!v.recovered || v.pageCountBefore !== v.pageCountAfter) throw new Error(JSON.stringify(v));
writeFileSync('out.hwp', Buffer.from(doc.exportHwp()));
```

`scripts/hwp/loader.mjs`의 `saveHwp()`가 이 게이트를 강제한다. **직접 `exportHwp()`를 쓰지 말고 `saveHwp()`를 써라.**

## CRITICAL: 원본을 덮어쓰지 마라

편집 결과는 **항상 새 경로로 저장**한다. 원본은 사용자의 유일한 사본일 수 있고, 편집이 잘못되면 되돌릴 수 없다.

```js
await saveHwp(doc, 'output/edited.hwp');   // ✅ 새 경로
await saveHwp(doc, inputPath);             // ❌ 원본 파괴
```

## CRITICAL: 컨트롤 인덱스를 캐싱하지 마라

**저장 → 재열기 후 `control_idx`가 바뀐다.** 표와 이미지 모두에서 확인됐다 (`ctrl=0` → `ctrl=1`).

```js
const { controlIdx } = JSON.parse(doc.createTable(0, 0, 0, 3, 2));   // controlIdx = 0
await saveHwp(doc, 'out.hwp');
const re = await openHwp('out.hwp');
re.getTableDimensions(0, 0, controlIdx);   // ❌ "지정된 컨트롤이 표가 아닙니다" — 이제 1 이다
```

파일을 다시 열었다면 **인덱스를 다시 탐색하라**. 문단 인덱스도 편집으로 밀릴 수 있으니 같은 원칙을 적용한다.

## 공통 워크플로

1. **열기 / 생성**: `openHwp(path)` 또는 `createHwp()`
2. **구조 파악**: `getSectionCount()`, `getParagraphCount(sec)`, `getTextRange(...)`로 어디를 고칠지 먼저 확인
3. **편집**: `insertText`, `applyCharFormat`, `insertTableRow` 등
4. **검증 + 저장 (필수)**: `saveHwp(doc, outPath)` — 검증 실패 시 파일을 쓰지 않고 throw한다
5. **독립 검증**: `node scripts/verify_hwp.mjs out.hwp`
6. **오류 수정**: `status`가 `errors_found`면 `error_summary`를 보고 고친 뒤 재검증

### 새 HWP 만들기

```js
import { createHwp, saveHwp } from './scripts/hwp/loader.mjs';

const doc = await createHwp();                    // blank2010.hwp 내장 템플릿
doc.insertText(0, 0, 0, '안녕하세요. 보고서입니다.');

// ⚠️ createEmpty() 문서에서 applyCharFormat 은 범위를 무시하고 문서 전체에 적용된다 (아래 "알려진 버그").
//    문단 전체를 같은 서식으로 둘 때만 안전하다.
doc.applyCharFormat(0, 0, 0, 13, JSON.stringify({ bold: true, fontSize: 1400, textColor: '#FF0000' }));
doc.applyParaFormat(0, 0, JSON.stringify({ alignment: 'center' }));

console.log(await saveHwp(doc, 'output/new.hwp'));
// → { bytesLen: 3584, pageCountBefore: 1, pageCountAfter: 1, recovered: true, path: ..., bytesWritten: 3584 }
```

### 기존 HWP 편집

```js
import { openHwp, saveHwp } from './scripts/hwp/loader.mjs';

const doc = await openHwp('samples/복학원서.hwp');
console.log(doc.getSectionCount(), doc.getParagraphCount(0), doc.pageCount());

// 고치기 전에 현재 내용부터 확인한다
console.log(doc.getTextRange(0, 0, 0, 100));

doc.insertText(0, 0, 0, '[수정] ');
await saveHwp(doc, 'output/edited.hwp');   // 원본이 아닌 새 경로
```

### 텍스트 읽기 / 추출

```js
const doc = await openHwp('input.hwp');
for (let s = 0; s < doc.getSectionCount(); s++) {
  for (let p = 0; p < doc.getParagraphCount(s); p++) {
    const line = doc.getTextRange(s, p, 0, 10000);
    if (line) console.log(line);
  }
}
```

### 표 편집

표는 `(para_idx, control_idx)`로 지정한다. **인덱스를 가정하지 말고 탐색하라.**

```js
// 문서 안의 표를 모두 찾는다
const scanTables = (doc) => {
  const out = [];
  for (let p = 0; p < doc.getParagraphCount(0); p++)
    for (let c = 0; c < 4; c++) {
      try { const j = doc.getTableDimensions(0, p, c); if (j) out.push({ p, c, ...JSON.parse(j) }); } catch {}
    }
  return out;
};

const doc = await openHwp('samples/복학원서.hwp');
const t = scanTables(doc)[0];        // → { p:2, c:0, rowCount:5, colCount:4, cellCount:15 }

doc.insertTextInCell(0, t.p, t.c, 0, 0, 0, '셀 내용');    // cellIdx=0 의 문단0, offset0
doc.insertTableRow(0, t.p, t.c, 0, true);                 // 0행 아래에 행 추가
doc.insertTableColumn(0, t.p, t.c, 0, true);              // 0열 오른쪽에 열 추가
doc.mergeTableCells(0, t.p, t.c, 1, 0, 1, 1);             // (1,0)~(1,1) 가로 병합

await saveHwp(doc, 'output/table_edited.hwp');
```

새 표는 `createTable(sec, para, offset, rows, cols)`로 만든다 — 반환 JSON에 `paraIdx`/`controlIdx`가 담긴다.

```js
const doc = await createHwp();
const { paraIdx, controlIdx } = JSON.parse(doc.createTable(0, 0, 0, 3, 2));
doc.insertTextInCell(0, paraIdx, controlIdx, 0, 0, 0, 'R1C1');
await saveHwp(doc, 'output/table_new.hwp');
```

병합 전에는 `getCellInfo(sec, para, ctrl, cellIdx)`로 `rowSpan`/`colSpan`을 확인하라. 이미 병합된 셀을
다시 병합하면 "병합 범위를 벗어납니다" 오류가 난다.

### 이미지 삽입

```js
import { readFileSync } from 'node:fs';

const png = readFileSync('logo.png');
const natW = png.readUInt32BE(16), natH = png.readUInt32BE(20);   // PNG IHDR
const wHU = Math.round((natW / 96) * 7200);   // px → HWPUNIT (1인치 = 96px = 7200HU)
const hHU = Math.round((natH / 96) * 7200);

const doc = await openHwp('input.hwp');
doc.insertPicture(0, 0, 0, '[]', png, wHU, hHU, natW, natH, 'png', '설명');
await saveHwp(doc, 'output/with_image.hwp');
```

`cell_path_json`이 `'[]'`(또는 빈 문자열)이면 **본문 인라인** 삽입이다. 표 셀 안에 띄우려면
`'[{"controlIndex":0,"cellIndex":2,"cellParaIndex":0}]'` 형태로 경로를 준다.

이미지 바이트는 재인코딩 없이 그대로 보존된다. 확인:

```js
const data = doc.getControlImageData(0, para, '[]', ctrl);   // Uint8Array
const mime = doc.getControlImageMime(0, para, '[]', ctrl);   // "image/png"
```

SVG/PNG 렌더, Markdown 내보내기, IR 비교, 레이아웃 디버깅은 `rhwp` CLI의 영역이다 → **rhwp-cli 스킬**을 사용하라 (CLI는 `cargo build --release`가 필요하다).

## 검증하기

```bash
node scripts/verify_hwp.mjs <파일.hwp>
```

대상 파일을 **읽기만 한다** (수정하지 않는다). 검사 항목:

1. **CFB 시그니처** — 진짜 HWP 5.0 복합 파일 컨테이너인가 (`D0CF11E0A1B11AE1`)
2. **재파싱** — 다시 열었을 때 IR이 깨지지 않는가
3. **라운드트립** — `exportHwpVerify()`로 자기 재로드 + 페이지 수 보존 확인

정상이면 exit 0:
```json
{
  "status": "success",
  "cfb_ok": true,
  "reparse_ok": true,
  "page_count": 1,
  "roundtrip": { "bytesLen": 3584, "pageCountBefore": 1, "pageCountAfter": 1, "recovered": true }
}
```

손상이면 exit 1 + `error_summary`:

| `type` | 의미 | 조치 |
|--------|------|------|
| `not_cfb` | HWP 컨테이너가 아님 | 저장 경로/바이트 확인 |
| `reparse_failed` | 재파싱 불가 — IR 손상 | 편집 단계 되짚기 |
| `not_recovered` | 자기 재로드 실패 | 해당 편집 연산 제거 후 이분 탐색 |
| `page_count_mismatch` | 페이지 수 변동 | 페이지네이션 영향 편집 검토 |

## 알려진 버그 — createEmpty() 문서의 글자 서식

**`createEmpty()`로 만든 문서에서는 `applyCharFormat`의 start/end 범위가 무시되고 문서 전체에 적용된다.**

빈 템플릿은 `char_shapes` 목록이 비어 있다. 이때 `find_or_create_char_shape`이 새 ID 대신 `0`을 반환해
**0번 글자모양을 덮어쓰고**, 모든 글자가 0번을 참조하므로 전부 물든다.
(HWPX로 내보내 확인하면 `charPr id=0`의 `textColor`가 바뀌어 있다.)

| 대상 | 범위 서식 | 비고 |
|------|:---------:|------|
| 기존 `.hwp` (실제 문서) | ✅ 정상 | char_shapes 보유 → 새 ID 생성 |
| `createEmpty()` 빈 문서 | ❌ 전체 물듦 | char_shapes 비어 있음 → 0번 덮어씀 |

**우회 방법**: 부분 서식이 필요하면 빈 문서에서 시작하지 말고, 서식이 정의된 기존 `.hwp`를 템플릿으로 열어 편집하라.

서식을 적용했다면 **렌더로 확인하라** — `{"ok":true}`는 이 버그를 잡아내지 못한다:

```js
const svg = doc.renderPageSvg(0);
const reds = [...svg.matchAll(/<text[^>]*fill="#ff0000"[^>]*>([^<]*)</gi)].map(m => m[1]);
console.log(reds.join(''));   // 의도한 글자만 나오는가?
```

## 검증 체크리스트

- [ ] `saveHwp()`로 저장했는가 (`exportHwp()` 직접 호출 금지)
- [ ] `verify_hwp.mjs`가 `status: "success"`를 반환하는가
- [ ] 원본이 아닌 **새 경로**에 저장했는가
- [ ] 편집 결과를 재열기해 `getTextRange()`로 **내용이 실제로 들어갔는지** 확인했는가
- [ ] 서식·표·이미지를 건드렸다면 `renderPageSvg(0)`로 렌더해 확인했는가 (cargo 없이 node에서 가능)
- [ ] 재열기 후 컨트롤 인덱스를 **재탐색**했는가 (저장하면 밀린다)
- [ ] 한컴오피스에서 경고 없이 열리는가 (최종 확인은 사람이 한다)

## 핵심 API

`new HwpDocument(bytes)`로 열고, `HwpDocument.createEmpty()`로 만든다. 인덱스는 모두 **0부터**.

| 목적 | 시그니처 |
|------|----------|
| 빈 문서 생성 | `static createEmpty(): HwpDocument` |
| 구조 조회 | `getSectionCount()`, `getParagraphCount(sec)`, `pageCount()`, `getDocumentInfo()` |
| 텍스트 읽기 | `getTextRange(sec, para, offset, count): string` |
| 텍스트 삽입 | `insertText(sec, para, offset, text): string` |
| 텍스트 삭제 | `deleteText(sec, para, offset, count): string` |
| 글자 서식 | `applyCharFormat(sec, para, start, end, props_json): string` |
| 문단 서식 | `applyParaFormat(sec, para, props_json): string` |
| 표 탐색 | `getTableDimensions(sec, para, ctrl): string` (JSON `{rowCount, colCount, cellCount}`) |
| 표 생성 | `createTable(sec, para, offset, rows, cols): string` (JSON `{paraIdx, controlIdx}`) |
| 셀 정보 | `getCellInfo(sec, para, ctrl, cellIdx): string` (JSON `{row, col, rowSpan, colSpan}`) |
| 표 행 | `insertTableRow(sec, parentPara, ctrl, rowIdx, below)` / `deleteTableRow(sec, parentPara, ctrl, rowIdx)` |
| 표 열 | `insertTableColumn(sec, parentPara, ctrl, colIdx, right)` / `deleteTableColumn(sec, parentPara, ctrl, colIdx)` |
| 셀 병합 | `mergeTableCells(sec, parentPara, ctrl, startRow, startCol, endRow, endCol)` |
| 셀 텍스트 | `insertTextInCell(sec, parentPara, ctrl, cellIdx, cellPara, offset, text)` / `getTextInCell(...)` |
| 이미지 삽입 | `insertPicture(sec, para, offset, cellPathJson, bytes, wHU, hHU, natW, natH, ext, desc)` |
| 이미지 조회 | `getControlImageData(sec, para, cellPathJson, ctrl)` / `getControlImageMime(...)` |
| 렌더 (시각 확인) | `renderPageSvg(page): string`, `renderPageHtml(page): string` |
| 검증 | `exportHwpVerify(): string` (JSON) |
| 내보내기 | `exportHwp(): Uint8Array`, `exportHwpx(): Uint8Array` |

편집 명령은 대부분 `{"ok":true, ...}` JSON **문자열**을 반환한다 — 필요하면 `JSON.parse`로 확인한다.

`applyCharFormat`의 `props_json` 키: `bold`(bool), `italic`(bool), `underline`(bool), `strikethrough`(bool), `fontSize`(정수, 1/100pt — 1400 = 14pt), `fontId`(정수), `textColor`/`shadeColor`/`underlineColor`(CSS 문자열, 예: `"#FF0000"`).

`applyParaFormat`의 `props_json` 키: `alignment`(문자열: `left`|`right`|`center`|`justify`|`distribute`), `lineSpacing`(정수), `lineSpacingType`(`Percent`|`Fixed`|`SpaceOnly`|`Minimum`), `indent`·`marginLeft`·`marginRight`·`spacingBefore`·`spacingAfter`(정수, HWPUNIT), `keepWithNext`·`widowOrphan`(bool).

> ⚠️ **키 이름이 틀리면 조용히 무시되고 `{"ok":true}`가 그대로 돌아온다.** (예: `align`은 무시된다 — 올바른 키는 `alignment`)
> 서식이 안 먹으면 먼저 키 철자를 의심하고 `src/document_core/helpers.rs`의 `parse_char_shape_mods` / `parse_para_shape_mods`에서 실제 키를 확인하라.

**여기 없는 API가 필요하면 `pkg/rhwp.d.ts`에서 확인하라** (332개 export의 자동 생성 명세, 한국어 doc 주석 포함). **API 이름을 추측하지 마라** — 존재하지 않는 이름은 조용히 실패하거나 예외를 던진다.

## 모범 사례

- **편집 전에 읽어라**: `getTextRange()`로 대상 문단의 현재 내용을 확인한 뒤 인덱스를 정한다. 문서 구조를 가정하지 마라.
- **작게 시작하라**: 문단 하나에 먼저 적용해 보고 전체로 확장한다.
- **인덱스는 0부터**: 섹션·문단·페이지 모두.
- **단위**: 1인치 = 7200 HWPUNIT = 96px. 글자 크기는 1/100pt.
- **출력 폴더**: `output/` 아래에 쓴다 (`.gitignore` 등록됨).
- **WASM 초기화는 프로세스당 1회**: `loader.mjs`가 캐시한다.
- **반환값을 믿지 말고 결과를 확인하라**: `{"ok":true}`는 명령이 접수됐다는 뜻이지, 문서가 의도대로 됐다는 뜻이 아니다. 저장 후 재열기해 확인한다.

## 흔한 실수

| 실수 | 바로잡기 |
|------|----------|
| 검증 없이 `exportHwp()`로 저장 | `saveHwp()` 사용 — "저장됨"과 "한컴에서 열림"은 다르다 |
| 원본 경로에 덮어쓰기 | 항상 새 출력 경로로 저장 |
| `{"ok":true}`만 보고 성공으로 간주 | 재열기 후 `getTextRange()`로 실제 내용 확인 |
| API 이름을 추측 | `pkg/rhwp.d.ts`에서 확인 — 없는 이름은 실패한다 |
| `.env.docker`의 UID/GID를 실제 값으로 수정 | 예제 그대로(1000) 둔다 — GID 20은 컨테이너에서 충돌 |
| CLI(`rhwp`)를 이 스킬에서 호출 | CLI는 `cargo` 빌드 필요 — 렌더/디버깅은 rhwp-cli 스킬 |
| HWPX를 거쳐 편집 | HWP 출처는 직접 편집해야 어댑터 no-op으로 원본 보존 |
| 저장 전 컨트롤 인덱스를 재열기 후 재사용 | 재열기 후 반드시 재탐색 — 인덱스가 밀린다 |
| 이미 병합된 셀을 다시 병합 | `getCellInfo`로 `colSpan`/`rowSpan` 먼저 확인 |

## 참조

- WASM API 전체 명세: `pkg/rhwp.d.ts` (빌드 후 생성) 또는 `rhwp-studio/public/rhwp.d.ts` (커밋본)
- 공유 로더: `scripts/hwp/loader.mjs` — `loadWasm` / `openHwp` / `createHwp` / `saveHwp`
- 검증 게이트: `scripts/verify_hwp.mjs`
- 형제 스킬: `.claude/skills/rhwp-cli/`(CLI 분석·렌더·디버깅), `.claude/skills/rhwp-exam-ingest/`(시험지→HWPX)
- 핵심 소스: `src/wasm_api.rs`(WASM 바인딩), `src/document_core/commands/`(편집 구현)
