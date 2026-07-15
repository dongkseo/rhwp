---
name: hwp
description: >-
  Use when a HWP document (.hwp, 한글 파일) is the primary input or output — 생성/편집/분석
  of Korean HWP files with rhwp. Trigger when the user wants to: 새 .hwp 문서 만들기, 기존
  .hwp 편집(문자열 치환, 템플릿 채우기, 문단·표·이미지·서식 수정), 또는 .hwp 내용 분석·추출
  (텍스트·표·이미지·수식·각주·페이지 렌더). Also trigger when a user references a .hwp file by
  name/path and wants something done to it or produced from it. Do NOT trigger when the
  deliverable is a Word/Excel/PDF file, an HTML report, or when only .hwpx internals are in
  scope (그 경우는 rhwp-exam-ingest / rhwp-cli 참조).
license: 저장소 라이선스(MIT)를 따른다.
---

> ⚠️ **인증 상태 — 소스 독해 기반, 행위 미검증**: 이 스킬은 rhwp 소스(2026-07-15 기준) 독해로
> 작성되었고, Rust 툴체인 부재로 빌드·실행 검증을 하지 않았다. 실사용 전
> `cargo build --release` 후 `scripts/verify_hwp.sh` 스모크 테스트를 반드시 통과시켜라.

# HWP 문서 생성 · 편집 · 분석 (rhwp)

## Overview

사용자가 `.hwp`(한글) 파일을 만들거나, 편집하거나, 내용을 분석·추출하도록 돕는다.
작업 유형별로 사용하는 인터페이스가 다르다:

- **분석·렌더** → `rhwp` CLI 서브커맨드 (읽기 전용, 손상 위험 없음)
- **생성·편집** → `document_core::commands`의 `*_native` API + `export_hwp_native()`
  (Rust 라이브러리 / WASM `exportHwp` / 웹 hwpctl Action) → `.hwp` 라운드트립 저장
- **모든 산출물은 저장 후 반드시 검증**한다 (아래 "검증" 절, xlsx 스킬의 `recalc.py`에 대응)

전체 기능 점검표(HWP 전용, 인터페이스·성숙도 포함)는 → [capabilities.md](capabilities.md)
전체 CLI 레퍼런스는 → `mydocs/manual/cli_commands.md`

# 산출물 요구사항 (Requirements for Outputs)

## 모든 HWP 파일

- **글꼴**: 별도 지시가 없으면 한컴 표준 글꼴(함초롬바탕/함초롬돋움)을 일관되게 사용.
- **무손상(ZERO corruption)**: 산출 `.hwp`는 **한컴오피스에서 경고 없이 깨끗이 열려야** 한다.
  xlsx가 "수식 오류 0"을 강제하듯, HWP는 "손상 0"을 강제한다 — 저장 후 `verify_hwp.sh` 통과 필수.
- **기존 템플릿 보존(편집 시)**: 기존 `.hwp`를 수정할 때는 원본의 서식·스타일·관례를
  **그대로 유지**하고 표준 서식을 강요하지 않는다. HWP-출처 라운드트립은 어댑터가 no-op이라
  원본 구조를 보존한다.

## 알려진 한계 (반드시 caveat로 명시)

- **이미지**: `insert_picture_native`는 동작하나 그룹 내 Picture 등 일부 케이스는 강화 중(#428).
  삽입 후 반드시 시각 확인.
- **새 표 삽입(`insert_hwp_table`)**: 실험적 — 파일은 valid하나 인식이 비완전.
- **수식**: 복잡 수식은 이미지로 캡처해 Picture로 삽입하는 편이 안전.
- **미주(endnote)·차트·머리말/꼬리말 신규 삽입**: 현재 미지원(footnote·기존 헤더 편집은 가능).
- 자체 라운드트립/테스트 통과가 곧 한컴 100% 호환을 뜻하지는 않는다 — 시각 검증을 병행하라.

# 워크플로

## 1) 분석 — .hwp 내용 읽기·추출 (CLI, 읽기 전용)

```bash
rhwp info        input.hwp                    # 버전·페이지·글꼴·표/이미지/각주/수식 통계
rhwp export-text     input.hwp -o out.txt     # 본문+머리말+꼬리말+각주+수식 통합 텍스트
rhwp export-markdown input.hwp -o out.md       # 문서→Markdown (표 GFM, 이미지 추출, 수식 $…$)
rhwp dump        input.hwp                     # IR 구조·조판 마크·문단/표 속성
rhwp dump-pages  input.hwp                     # 섹션별 용지·여백·단·헤더/푸터 마진
rhwp dump-records input.hwp                    # 원시 레코드 (임베디드 이미지 목록 포함)
rhwp export-svg  input.hwp -o out/             # 페이지 → SVG 렌더 (시각 검증용)
rhwp ir-diff     a.hwp b.hwp                   # 두 문서 IR 비교
```

## 2) 생성 — 새 .hwp 만들기

두 경로가 있다. 대부분의 문서형 생성은 (A)가 안전하다.

**(A) ingest → HWPX → convert (권장, CLI만으로 완결)**
```bash
# 1. 콘텐츠를 ingest.json으로 작성 (스키마: tools/rhwp-ingest/schema/ingest_schema_v1.json,
#    샘플: tools/rhwp-ingest/schema/sample_minimal.json)
rhwp build-from-ingest ingest.json --media-dir ./media -o out.hwpx   # → HWPX
rhwp convert out.hwpx out.hwp --verify --verify-pages                # → .hwp (무차이 게이트)
```
`convert` 입력은 `.hwp|.hwpx`, 출력은 항상 `.hwp`. HWPX-출처면 어댑터가 적용되고,
`--verify`(IR 무차이, exit 3) / `--verify-pages`(exit 4)로 손상을 잡는다.

**(B) IR 빌드 → export_hwp_native (프로그래밍/네이티브)**
`document_core`로 Document IR을 구성한 뒤 `document.rs:980 export_hwp_native()`로 직렬화.
CLI의 `gen-table`, `test-field`, `test-shape`, `gen-pua`가 이 경로의 예시다.

## 3) 편집 — 기존 .hwp 수정 (native `*_native` → 라운드트립 저장)

`document_core::commands`의 `*_native` API로 IR을 변형한 뒤 `export_hwp_native()`로 다시 `.hwp` 저장.
(Rust 라이브러리 / WASM `exportHwp` / 웹 hwpctl Action으로 호출)

| 하려는 일 | `*_native` 명령 (모듈) |
|-----------|------------------------|
| 문자열 치환 / 템플릿 채우기 / 문단 텍스트 교체 | `text_editing.rs` |
| 셀 텍스트·행/열 추가·삭제·병합 | `table_ops.rs`, `insert_table_column_native` |
| 새 문단 추가 / 삭제 | `insert_paragraph_native`, `text_editing.rs` |
| 이미지 삽입 / 교체 / 삭제 | `insert_picture_native`, `object_ops/` |
| 글자·문단 서식 | `apply_char_format_native`, `formatting.rs` |
| 각주 / 머리말·꼬리말(기존) | `footnote_ops.rs`, `header_footer_ops.rs` |

편집 대상이 HWP-출처면 어댑터가 no-op → 원본 구조가 그대로 보존된다.

# 검증 (MANDATORY) — xlsx의 recalc.py에 대응

openpyxl로 만든 xlsx가 계산되지 않은 수식을 담듯, 편집·생성한 `.hwp`도 저장 직후엔
손상 여부가 확정되지 않는다. **모든 산출물은 저장 후 반드시 검증한다.**

```bash
bash scripts/verify_hwp.sh out.hwp                 # CFB 시그니처 + rhwp dump
bash scripts/verify_hwp.sh out.hwp source.hwpx     # + convert --verify (IR 무차이 게이트)
```

스크립트는 JSON 한 줄을 반환한다:
```json
{"status":"ok","cfb_ok":true,"dump_ok":true,"verify_exit":0,"notes":"..."}
```
- `status`가 `errors_found`면 `notes`를 보고 원인(비-CFB / dump 실패 / IR 차이 exit 3 /
  페이지 불일치 exit 4)을 고친 뒤 재검증.
- 스크립트가 `rhwp`를 못 찾으면(`rhwp_not_found`) `cargo build --release`로 바이너리를 만든 뒤
  `RHWP=./target/release/rhwp bash scripts/verify_hwp.sh …`로 재실행.

## 검증 체크리스트

- [ ] 산출 `.hwp`가 CFB 시그니처(`D0CF11E0A1B11AE1`)로 시작하는가
- [ ] `rhwp dump`가 오류 없이 완료되는가 (IR 파싱 무손상)
- [ ] `convert --verify`가 IR 무차이(exit 0)인가
- [ ] `convert --verify-pages`가 페이지 수 일치(exit 0)인가
- [ ] 이미지/표를 넣었다면 `export-svg`로 렌더해 시각 확인했는가 (#428/insert_table ⚠️)
- [ ] 한컴오피스 2024(또는 LibreOffice)에서 경고 없이 열리는가

# 모범 사례 / 단위

- **바이너리**: 속도를 위해 `cargo build --release`(→ `target/release/rhwp`) 사용. 개발 중엔 `cargo run`.
  네이티브 빌드는 로컬 cargo만 사용(Docker는 WASM 전용).
- **분석은 비파괴**: 원칙적으로 분석 명령은 파일을 변경하지 않는다. 편집은 항상 새 경로로 저장해 원본 보존.
- **페이지 번호는 0부터** 시작.
- **단위 환산**: 1인치 = 7200 HWPUNIT = 96px.
- 출력 기본 폴더는 `output/`, 분석 산출물은 `output/poc/<topic>/` 아래 권장.

# 흔한 실수

| 실수 | 바로잡기 |
|------|----------|
| 저장 후 검증 생략 | `verify_hwp.sh` 필수 — "저장됨"과 "한컴에서 열림"은 다르다 |
| 원본을 덮어써 편집 | 항상 새 출력 경로로 저장해 원본 보존 |
| `insert_hwp_table`/이미지 결과를 렌더 확인 없이 신뢰 | 실험적/강화 중 — `export-svg`로 시각 확인 |
| `.hwpx` 전용 기능을 `.hwp`에 기대 | `list_hwp_bindata`(BinData/ ZIP)는 `.hwp` 대상 없음 |
| 자체 테스트 통과 = 한컴 호환으로 간주 | 시각 검증 병행 필수 |

# 참조

- 기능 점검표(HWP 전용): [capabilities.md](capabilities.md)
- CLI 전체 매뉴얼: `mydocs/manual/cli_commands.md`
- ingest 스키마/샘플: `tools/rhwp-ingest/schema/{ingest_schema_v1.json,sample_minimal.json}`
- 형제 스킬: `.claude/skills/rhwp-cli/`(분석·디버깅), `.claude/skills/rhwp-exam-ingest/`(시험지→HWPX)
- 핵심 소스: `src/document_core/commands/`(편집), `document.rs:980 export_hwp_native`(직렬화),
  `src/document_core/converters/hwpx_to_hwp.rs`(HWPX→HWP 어댑터)
