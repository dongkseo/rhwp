# Task m100 #903 Stage 35

## 1. 단계 목적

Stage 34 판정:

```text
01 clean adapter: 파일 읽기 오류
02 clean + section_count/ParaShape: 파일 읽기 오류
03 clean + reference DocInfo: 파일 읽기 오류
04 Stage27 baseline + section_count/ParaShape: 정상
05 Stage27 baseline + reference DocInfo: 정상
```

따라서 원인은 DocInfo가 아니라 Stage 27 baseline에 누적된 BodyText materialization이다.

Stage 35는 Stage 27의 누적 materialization을 다음 블록으로 나누어 적용한다.

## 2. 핵심 가설

한컴 에디터가 파일을 읽지 못하는 원인은 clean adapter가 만드는 BodyText record 구조가 한컴이 기대하는 HWP record tuple과 일부 다르기 때문이다.

검증할 축:

- top-level paragraph record raw materialization
- table/object full materialization
- final region/section1 경계 materialization

## 3. 산출물

```text
output/poc/hwpx2hwp/task903/stage35_stage27_block_probe/
```

## 4. Variant 계획

| variant | 적용 내용 | 목적 |
|---|---|---|
| 01_top_level_para_records_only | Stage 27에서 복사했던 top-level paragraph record만 적용 | 문단 record raw tuple만으로 읽기 오류가 사라지는지 확인 |
| 02_table_object_records_only | Stage 27에서 복사했던 table/object full record만 적용 | 표/그림/묶음 object tuple만으로 읽기 오류가 사라지는지 확인 |
| 03_para_plus_table_object_records | 01 + 02 | 문단/표 object tuple 조합이 필요한지 확인 |
| 04_common_stage27_without_section1 | Stage 27 공통 baseline + final region table까지 적용, section1 제외 | 0구역 전체 BodyText가 충분한지 확인 |
| 05_full_stage27_control | Stage 27 baseline 09와 동등한 전체 control | Stage34 04/05 정상 판정 재현 |

모든 variant에는 Stage 30의 최소 안정화 축을 함께 적용한다.

```text
section_count = 실제 section 수
ParaShape = 정답 HWP 기준 재직렬화(no raw)
FileHeader = compressed HWP
```

## 5. 작업지시자 판정 항목

생성 명령:

```bash
cargo test --test hwpx_to_hwp_adapter task903_stage35_generate_stage27_block_probe_variants -- --nocapture
```

실행 결과:

```text
=> ok. 1 passed

01_top_level_para_records_only.hwp: bytes=374272, changed=42, pages=9, section_count=2
02_table_object_records_only.hwp: bytes=375808, changed=30, pages=9, section_count=2
03_para_plus_table_object_records.hwp: bytes=375808, changed=72, pages=9, section_count=2
04_common_stage27_without_section1.hwp: bytes=375808, changed=73, pages=9, section_count=2
05_full_stage27_control.hwp: bytes=375808, changed=74, pages=9, section_count=2
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
| 01_top_level_para_records_only | 파일 읽기 오류 |  |  |  |  |  |  |
| 02_table_object_records_only | 정상 | 정상 | 정상 |  |  |  |  |
| 03_para_plus_table_object_records | 정상 | 정상 | 정상 |  |  |  |  |
| 04_common_stage27_without_section1 | 정상 | 정상 | 정상 |  |  |  |  |
| 05_full_stage27_control | 정상 | 정상 | 정상 |  |  |  |  |

## 6. 해석 기준

- 01만 정상: top-level paragraph record materialization이 핵심.
- 02만 정상: table/object full tuple이 핵심.
- 03부터 정상: 문단 record와 table/object tuple의 조합이 필요.
- 04부터 정상: final region 또는 0구역 후반 경계가 필요.
- 05만 정상: section1 시작 경계 또는 final region + section1 조합이 필요.

실제 판정:

- 01만 파일 읽기 오류.
- 02/03/04/05는 정상.

결론:

- top-level paragraph record raw materialization은 한컴 파일 읽기 오류 해결에 필요하지 않다.
- Stage35의 `table/object records only` bundle만으로 한컴 호환성이 회복된다.
- 다음 단계는 Stage35 02 bundle 내부를 table/object 그룹별로 더 분해한다.
