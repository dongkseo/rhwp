# Task M100-1187 최종 보고서 — BookReview.hwp 글상자 내용 overflow 회귀 수정

## 개요

- 이슈: #1187 `BookReview.hwp 글상자 내용이 영역 밖으로 출력되는 회귀`
- 기준: `upstream/devel` `1eb76529fce21d0a5330720f7b458831c5252fdf`
- 작업 브랜치: `local/task1187`
- 작업 worktree: `/private/tmp/rhwp-task1187`

## 원인

`layout_textbox_content` 는 글상자 내부 영역(`inner_area`)을 계산하지만, 실제 글상자 문단/표/도형 자식은 shape node 직속으로 배치됐다. SVG/paint layer 의 clip 처리는 Body/TableCell 중심이라 `TextBox` 콘텐츠에는 적용되지 않았고, `BookReview.hwp` 의 일부 문단처럼 `line_seg.vertical_pos` 가 내부 높이를 초과하면 글상자 밖까지 그대로 출력됐다.

## 수정 요약

1. `tests/issue_1187_textbox_clip.rs` 를 추가해 `BookReview.hwp` 1쪽의 SVG 글상자 clipPath 회귀를 고정했다.
2. `src/renderer/layout/shape_layout.rs` 에서 글상자 내부 콘텐츠를 `RenderNodeType::TextBox` 노드 아래로 모았다.
3. `src/renderer/svg.rs` 에서 `RenderNodeType::TextBox` 에 `textbox-clip-{id}` clipPath 를 적용했다.
4. `src/paint` layer 경로에 `ClipKind::TextBox` 를 추가하고, `LayerBuilder` 가 TextBox 를 `ClipRect` 로 내리도록 했다.
5. PageLayerTree JSON 에 `"clipKind":"textBox"` 를 추가하면서 `schemaMinorVersion` 을 `14 -> 15` 로 올렸다.
6. `svg_layer`, `web_canvas`, `canvaskit_policy`, `rhwp-studio` 타입 정의를 새 clip kind 에 맞췄다.

## 산출물 확인

최종 SVG:

- `/private/tmp/rhwp-task1187-final-svg/BookReview_001.svg`

확인된 clipPath:

- `textbox-clip-36`
- `textbox-clip-55`
- `textbox-clip-106`

회귀가 발생한 큰 글상자 clip rect:

- `x=47.91999999999997`
- `y=516.5600000000001`
- `width=687.5466666666667`
- `height=487.88`

큰 글상자 콘텐츠는 `<g clip-path="url(#textbox-clip-55)">` 아래로 들어가며, 우측 하단 저자 정보 글상자는 별도 `textbox-clip-106` 으로 유지된다.

## 검증

통과:

```bash
cargo fmt --check
cargo build --bin rhwp
cargo test --test issue_1187_textbox_clip
cargo test --lib paint::builder::tests
cargo test --lib paint::json::tests::serializes_textbox_clip_kind
cargo test --lib paint::schema::tests::layer_tree_schema_constants_match_schema
cargo test --lib renderer::svg_layer::tests
cargo test --test issue_1052_footnote_in_textbox
cargo test --test issue_919_textbox_hit_test
cargo test --test issue_1028_hwpx_textbox_vertical
```

결과 요약:

- #1187 회귀 테스트: 2 passed
- paint builder 테스트: 7 passed
- svg layer 테스트: 3 passed
- 기존 글상자/각주/히트테스트/세로쓰기 관련 테스트: 모두 통과

미완료 검증:

```bash
npm run build
```

- 실패 사유: `tsc: command not found`
- 로컬 worktree 에 `rhwp-studio/node_modules` 가 없고 전역 `tsc` 도 없는 환경이다.
- 네트워크 의존성 설치는 수행하지 않았다.

## 커밋

- `90d16834` Task #1187 Stage 1: BookReview textbox clip regression test
- `fb351794` Task #1187 Stage 2: route textbox content through render node
- `befdc1f7` Task #1187 Stage 3: clip textbox content in SVG
- `ffdd45b4` Task #1187 Stage 4: clip textbox content in paint layer

## 남은 절차

- PR 준비 가능.
- 이슈 #1187 close 는 작업지시자 승인 전에는 수행하지 않는다.
