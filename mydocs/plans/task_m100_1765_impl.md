# Task #1765 구현계획서 — 병합 셀 가드 (3단계)

수행계획서: `mydocs/plans/task_m100_1765.md`

## Stage 1 — 재현 동결 + 실패 확인
- `samples/task1765/merged_cell_trailing_ls.hwp` (17931383) + README.
- render-tree/하니스로 dh +5.2 확인.

## Stage 2 — 가드 확장 + 테스트
- `height_measurer.rs` 2-b 경로에 #1763 동일 가드(combined 기준).
- 통합테스트 `tests/issue_1765_merged_cell_trailing_ls.rs` (표 전체 높이 단언).

## Stage 3 — 회귀 검증 + 최종보고
- lib/표 계열 통합/페이지 게이트 10종, #1759 하니스 재측정, 한글 OLE 대조
  (mismatch 잔여 + MATCH 150) → 보고서 → squash → PR (#1764 스택).
