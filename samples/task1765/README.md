# Task #1765 재현 샘플

## merged_cell_trailing_ls.hwp
- 출처: 국가법령정보센터 `[별표 9] 위험근무수당 등급별 구분표(제13조 관련)(...).hwp`
  (공개 서식, HWP5/OLE, 2쪽)
- 구조: 2쪽 12×4 표, rowspan 병합 셀에 다문단 콘텐츠.
- 결함(수정 전): 병합 셀 경로(height_measurer 2-b)의 trailing ls 포함 측정으로 required 가
  스팬 행 합을 초과 → 마지막 스팬 행 deficit 확장 → 표 전체 1006.8px vs 한글 1001.6px
  (**+5.2px**). #1763(rs=1 경로 가드) 후 잔존분.
- 기대(한글 정합): 초과분이 전적으로 trailing ls 이면 확장 억제 → 표 ≈1001.6px.
- 검증: `rhwp export-render-tree samples/task1765/merged_cell_trailing_ls.hwp -p 1` → Table h.
