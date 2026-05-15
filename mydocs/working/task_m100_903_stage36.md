# Task m100 #903 Stage 36

> Status: 보류.
>
> Stage30 기준으로 재시작하기로 결정했다. 이 문서는 이후 판단의 주 근거로 사용하지 않는다.

## 1. 단계 목적

Stage 35 판정:

```text
01_top_level_para_records_only: 파일 읽기 오류
02_table_object_records_only: 정상
03_para_plus_table_object_records: 정상
04_common_stage27_without_section1: 정상
05_full_stage27_control: 정상
```

따라서 한컴 파일 읽기 오류 해결의 핵심은 top-level paragraph record가 아니라 table/object materialization bundle이다.

Stage 36은 Stage 35의 `02_table_object_records_only` bundle을 더 잘게 분해한다.

## 2. 분해 대상

Stage 35의 table/object bundle에는 다음이 포함되어 있었다.

```text
- section_def core fields
- first cell LIST_HEADER tail 65
- first table pictures full payload
- first table CTRL_HEADER
- first table cell LIST_HEADER payload
- first table TABLE record payload
- first table cell PARA_HEADER payload
- second table child headers
- chart / industry / country / region / notice / attachment / top-country / year / second-year / final tables full object
- logo group full object
```

## 3. 산출물

```text
output/poc/hwpx2hwp/task903/stage36_table_object_block_probe/
```

## 4. Variant 계획

모든 variant에는 Stage 30 최소 안정화 축을 함께 적용한다.

```text
section_count = 실제 section 수
ParaShape = 정답 HWP 기준 재직렬화(no raw)
FileHeader = compressed HWP
```

| variant | 적용 내용 | 목적 |
|---|---|---|
| 01_section_def_first_cell_tail_only | section_def core + first cell LIST_HEADER tail 65 | structural header 축 확인 |
| 02_first_table_full_only | 첫 표 관련 full materialization만 적용 | 첫 표/그림 tuple이 충분한지 확인 |
| 03_second_table_child_headers_only | 두 번째 표 child headers만 적용 | 두 번째 표 경계 축 확인 |
| 04_chart_to_notice_tables_only | chart/industry/country/region/notice table full objects만 적용 | 1~2페이지 주요 표 tuple 축 확인 |
| 05_logo_attachment_tables_only | logo group + attachment title table full objects만 적용 | 2~3페이지 object/table 축 확인 |
| 06_late_tables_only | top-country/year/second-year/final-industry/final-country table full objects만 적용 | 후반 표 tuple 축 확인 |
| 07_first_second_chart_to_notice | 01~04 조합 | 전반 table/object bundle 최소 후보 |
| 08_all_tables_without_first_table | 첫 표 제외, 나머지 table/object bundle | 첫 표가 필수인지 확인 |
| 09_all_tables_without_late_tables | 후반 표 제외, 전반 table/object bundle | 후반 표가 필수인지 확인 |

## 5. 작업지시자 판정 항목

생성 명령:

```bash
cargo test --test hwpx_to_hwp_adapter task903_stage36_generate_table_object_block_probe_variants -- --nocapture
```

실행 결과:

```text
=> ok. 1 passed

01_section_def_first_cell_tail_only.hwp: bytes=374272, changed=5, pages=9, section_count=2
02_first_table_full_only.hwp: bytes=374784, changed=11, pages=9, section_count=2
03_second_table_child_headers_only.hwp: bytes=374272, changed=3, pages=9, section_count=2
04_chart_to_notice_tables_only.hwp: bytes=374784, changed=5, pages=9, section_count=2
05_logo_attachment_tables_only.hwp: bytes=374272, changed=2, pages=9, section_count=2
06_late_tables_only.hwp: bytes=375296, changed=5, pages=9, section_count=2
07_first_second_chart_to_notice.hwp: bytes=374784, changed=23, pages=9, section_count=2
08_all_tables_without_first_table.hwp: bytes=375296, changed=19, pages=9, section_count=2
09_all_tables_without_late_tables.hwp: bytes=374784, changed=25, pages=9, section_count=2
```

```text
- 한컴 에디터 파일 읽기 오류/파일손상/정상 여부
- 출력 페이지 수: 8페이지에서 멈추는지, 9페이지까지 출력되는지
- 표/셀 배치가 정상인지
- 꼬리말 페이지수 색이 기존 결함인 빨간색인지, 정상 검정색인지
- rhwp-studio에서 9페이지로 재로드되는지
```

판정 기록:

| variant | 한컴 판정 유형 | 한컴 출력 페이지 | 마지막 페이지 출력 | 표/셀 배치 | 꼬리말 페이지수 색 | rhwp-studio 판정 | 비고 |
|---|---|---|---|---|---|---|---|
| 01_section_def_first_cell_tail_only |  |  |  |  |  |  |  |
| 02_first_table_full_only |  |  |  |  |  |  |  |
| 03_second_table_child_headers_only |  |  |  |  |  |  |  |
| 04_chart_to_notice_tables_only |  |  |  |  |  |  |  |
| 05_logo_attachment_tables_only |  |  |  |  |  |  |  |
| 06_late_tables_only |  |  |  |  |  |  |  |
| 07_first_second_chart_to_notice |  |  |  |  |  |  |  |
| 08_all_tables_without_first_table |  |  |  |  |  |  |  |
| 09_all_tables_without_late_tables |  |  |  |  |  |  |  |

## 6. 해석 기준

- 02가 정상이라면 첫 표/그림 tuple이 핵심.
- 04가 정상이라면 차트~공지 표 구간의 table full object가 핵심.
- 06이 정상이라면 후반 표 tuple까지 필요한 전역 구조 문제.
- 07만 정상이라면 전반 table/object 조합 문제.
- 08이 정상이고 02가 실패하면 첫 표는 부수적이다.
- 09가 정상이고 06이 실패하면 후반 표는 부수적이다.
