# hwp 스킬 — 기능 점검표 (HWP 전용)

> 이 표는 MCP 래퍼 매트릭스에서 `.hwpx` 열을 제거하고, **rhwp 엔진이 네이티브 `.hwp`에서
> 실제로 지원하는 능력**으로 재정리한 것이다. 생성·편집은 rhwp `document_core`의 `*_native`
> 명령군 + `export_hwp_native()`(document.rs:980) 경로로, 분석·렌더는 CLI로 수행된다.
>
> **인터페이스 표기**
> - `CLI` — `rhwp <subcommand>` 바이너리로 바로 실행
> - `native` — `document_core::commands`의 `*_native` API (Rust 라이브러리 / WASM `exportHwp` / 웹 hwpctl Action).
>   편집 결과는 `export_hwp_native()`로 `.hwp` 라운드트립 저장
> - `⚠️` 실험적 · `❌` 미지원(로드맵) · `–` 대상 없음
>
> ⚠️ **인증 상태**: 본 표는 rhwp 소스(2026-07-15 기준) 독해로 작성. Rust 툴체인 부재로
> 행위 검증(빌드·실행)은 하지 않음. 실사용 전 `cargo build --release` 후 스모크 테스트 권장.

## 읽기 / 추출

| 도구 | HWP | 인터페이스 | 설명 |
|------|-----|-----------|------|
| read_hwp | ✅ | CLI (`dump` + `export-text`) | 본문 + 표(마크다운) + 이미지 목록 한 번에 |
| read_hwp_text | ✅ | CLI (`export-text`) | 본문 + 머리말 + 꼬리말 + 각주 + 수식 통합 텍스트 |
| read_hwp_tables | ✅ | CLI (`export-markdown`) | 표를 GitHub 마크다운으로 (셀 병합 처리) |
| convert_hwp_markdown | ✅ | CLI (`export-markdown`) | 문서 → Markdown (순서 유지, 표 GFM 제자리, 이미지 추출+상대링크, 수식 `$…$`, 각주 문서 끝) |
| list_hwp_images | ✅ | CLI (`dump-records`) | 임베디드 이미지 목록 (mime, 바이트) |
| extract_hwp_images | ✅ | CLI (`dump-records`/export) | 이미지를 디스크로 추출 |

## 메타 / 조회

| 도구 | HWP | 인터페이스 | 설명 |
|------|-----|-----------|------|
| get_hwp_info | ✅ | CLI (`info`) | 버전·페이지·글꼴·표/이미지/각주/수식 통계 |
| get_hwp_page_def | ✅ | CLI (`dump-pages`) | 섹션별 용지 크기·여백·단·헤더/푸터 마진 |
| list_hwp_fields | ✅ | CLI (`field`) | 한컴 필드 목록 |
| get_hwp_field_value | ✅ | CLI (`field`) | 필드 값 조회 |

> 제거됨(hwpx 전용): `list_hwp_bindata` — `.hwpx`의 `BinData/` ZIP 엔트리 개념이라 `.hwp`에 대응 없음.

## 시각 렌더

| 도구 | HWP | 인터페이스 | 설명 |
|------|-----|-----------|------|
| render_hwp_page | ✅ | CLI (`export-svg`) | 특정 페이지 → SVG (인라인/파일) |
| render_hwp_all_pages | ✅ | CLI (`export-svg`) | 전체 페이지 SVG 일괄 |
| render_hwp_html | ✅ | CLI (`export-structure`/svg) | 페이지 → HTML |
| render_hwp_equation_svg | – | – | OWPML 수식 script → SVG (대상 없음) |

## 쓰기 — 텍스트

| 도구 | HWP | 인터페이스 | 설명 |
|------|-----|-----------|------|
| replace_hwp_text | ✅ | native (`text_editing`) | 특정 문자열 찾아 바꾸기 |
| fill_hwp_template | ✅ | native (`text_editing`) | `{{이름}}` 등 다중 자리표시자 |
| set_hwp_paragraph_text | ✅ | native (`text_editing`) | N번째 문단 텍스트 통째 교체 |
| set_hwp_cell_text | ✅ | native (`table_ops`) | 표 셀 (행, 열) 텍스트 직접 설정 |
| set_hwp_field_value | ✅ | native (`text_editing`/field) | 필드 값 설정 |

## 쓰기 — 구조

| 도구 | HWP | 인터페이스 | 설명 |
|------|-----|-----------|------|
| append_hwp_paragraph | ✅ | native (`insert_paragraph_native`) | 본문 끝에 새 문단 |
| delete_hwp_paragraph | ✅ | native (`text_editing`) | N번째 문단 삭제 |
| append_hwp_table_row | ✅ | native (`table_ops`) | 표 마지막에 새 행 |
| delete_hwp_table_row | ✅ | native (`table_ops`) | 표 행 삭제 |
| append_hwp_table_column | ✅ | native (`insert_table_column_native`) | 표 끝에 새 열 (모든 행에) |
| delete_hwp_table_column | ✅ | native (`table_ops`) | 표 열 삭제 |
| merge_hwp_cells_horizontal | ✅ | native (`table_ops`) | 가로 셀 병합 (colSpan) |
| merge_hwp_cells_vertical | ✅ | native (`table_ops`) | 세로 셀 병합 (rowSpan) |
| replace_hwp_image | ✅ | native (`object_ops`) | 임베디드 이미지 교체 |

## 쓰기 — 서식

| 도구 | HWP | 인터페이스 | 설명 |
|------|-----|-----------|------|
| apply_hwp_text_style | ✅ | native (`apply_char_format_native`) | 글자 색·볼드·이탤릭·밑줄·크기 |
| apply_hwp_paragraph_style | ✅ | native (`formatting`) | 문단 정렬·들여쓰기·줄간격 |

## 쓰기 — 이미지 / 표 / 신규

| 도구 | HWP | 인터페이스 | 설명 |
|------|-----|-----------|------|
| insert_hwp_image | ✅ | native (`insert_picture_native`) | 새 이미지 추가. ⚠️ 그룹 내 Picture 등 일부 케이스는 강화 중(#428) |
| delete_hwp_image | ✅ | native (`object_ops`) | 이미지 삭제 |
| insert_hwp_table | ⚠️ | native (`table_ops`) | 새 표 삽입 (실험적 — 파일 valid, 인식 비완전) |
| create_hwp_document | ✅ | native (IR 빌드 → `export_hwp_native`) | 텍스트로 새 `.hwp` 생성 (`gen-table`/`test-field` 계열 경로) |

## 콘텐츠 추출 매트릭스

| 콘텐츠 | 추출 | 비고 |
|--------|:----:|------|
| 본문 문단 텍스트 | ✅ | `export-text` / read_hwp |
| 표 (셀 병합 포함) | ✅ | `export-markdown` |
| 임베디드 이미지 | ✅ | PNG/JPG/BMP 등 |
| 머리말 / 꼬리말 | ✅ | `--- headers --- / --- footers ---` 블록 |
| 각주(footnote) | ✅ | `--- footnotes ---` 블록, `[1] 본문…` |
| 수식(equation) | ✅ | OWPML script (예: `TIMES LEFT ( {a} over {b} RIGHT )`), `--- equations ---` 블록 |
| 페이지 SVG 렌더 | ✅ | `export-svg` |
| 텍스트박스 본문 | ❌ | `createShapeControl`은 생성하나 조회 패턴 비명시 — v0.3 trace |
| 미주(endnote) | – | rhwp 미지원 (footnote만) |
| 차트(chart) | ❌ | v0.3 이후 |

## 작성 매트릭스 (HWP)

| 작업 | HWP | 비고 |
|------|:---:|------|
| 텍스트 단일 치환 | ✅ | replace_hwp_text |
| 다중 자리표시자 채우기 | ✅ | fill_hwp_template |
| 문단 텍스트 통째 교체 | ✅ | set_hwp_paragraph_text |
| 표 셀 직접 수정 | ✅ | set_hwp_cell_text (행·열 지정) |
| 필드 값 설정 | ✅ | set_hwp_field_value |
| 새 문단 추가 / 삭제 | ✅ | append/delete_hwp_paragraph |
| 표 행 추가 / 삭제 | ✅ | append/delete_hwp_table_row |
| 표 열 추가 / 삭제 | ✅ | append/delete_hwp_table_column |
| 셀 병합 (가로·세로) | ✅ | merge_hwp_cells_horizontal/vertical |
| 이미지 교체 / 삭제 | ✅ | replace/delete_hwp_image |
| 새 이미지 삽입 | ✅ | insert_hwp_image (그룹 내 케이스 강화 중 #428) |
| 글자 서식 (색·볼드·이탤릭·밑줄·크기) | ✅ | apply_hwp_text_style |
| 문단 서식 (정렬·들여쓰기·줄간격) | ✅ | apply_hwp_paragraph_style |
| 새 문서 생성 (텍스트) | ✅ | create_hwp_document → `export_hwp_native` |
| 새 표 삽입 (진짜 OWPML) | ⚠️ | insert_hwp_table (실험적) |
| 머리말/꼬리말 신규 삽입 | ❌ | v0.3 |
| 차트·북마크·스타일 정의 | ❌ | v0.3 |

## 저장·검증 원칙

- 편집(`*_native`) 후에는 반드시 `export_hwp_native()`로 `.hwp` 라운드트립 저장.
- 저장 산출물은 **`scripts/verify_hwp.sh`로 검증**(CFB 시그니처 + `dump` + `convert --verify`).
- HWP-출처 라운드트립은 어댑터 no-op(원본 구조 보존), HWPX→HWP는 `convert`가 어댑터 적용.
  두 경우 모두 `--verify`(IR 무차이, exit 3) / `--verify-pages`(exit 4)로 손상 게이트.
