# Task m100 #903 Stage 33

## 1. 단계 목적

Stage 32에서 FileHeader/압축 축은 원인이 아니었다.

추가 비교 결과, Stage 32 실패 산출물은 정답 HWP와 TABLE record attr가 체계적으로 달랐다.

예:

```text
Stage 32 표1 attr: 0x04000006
정답 HWP 표1 attr: 0x0000000e

Stage 32 표4 attr: 0x00000004
정답 HWP 표4 attr: 0x0000000c
```

Stage 33은 TABLE record attr/payload 축을 분리한다.

## 2. 산출물

```text
output/poc/hwpx2hwp/task903/stage33_table_attr_probe/
```

## 3. Variant

| variant | 적용 내용 |
|---|---|
| 01_clear_table_attr_high_repeat_bit | Stage 32 기준에서 TABLE attr high repeat bit `0x04000000`만 제거 |
| 02_reference_table_record_attr_only | 정답 HWP의 TABLE `raw_table_record_attr`만 표 순서대로 복사 |
| 03_reference_table_record_payload | 정답 HWP의 TABLE record payload 필드와 tail을 표 순서대로 복사 |

## 4. 작업지시자 판정 요청

내부 생성/재로드:

```text
cargo test --test hwpx_to_hwp_adapter task903_stage33_generate_table_attr_probe_variants -- --nocapture
=> ok. 1 passed

01_clear_table_attr_high_repeat_bit.hwp: bytes=374272, changed=17, pages=9
02_reference_table_record_attr_only.hwp: bytes=374272, changed=26, pages=9
03_reference_table_record_payload.hwp: bytes=374272, changed=26, pages=9
```

다음 파일을 한컴 에디터와 rhwp-studio에서 판정한다.

```text
output/poc/hwpx2hwp/task903/stage33_table_attr_probe/01_clear_table_attr_high_repeat_bit.hwp
output/poc/hwpx2hwp/task903/stage33_table_attr_probe/02_reference_table_record_attr_only.hwp
output/poc/hwpx2hwp/task903/stage33_table_attr_probe/03_reference_table_record_payload.hwp
```

판정 항목:

```text
- 한컴 에디터 파일 읽기 오류가 사라지는 variant가 있는지
- 열리는 경우 9페이지 마지막 페이지가 출력되는지
- 표/셀 배치가 정상인지
- 꼬리말 페이지수가 검정색인지, 기존 결함인 빨간색인지
- rhwp-studio에서 9페이지로 재로드되는지
```

판정 기록:

| variant | 한컴 판정 유형 | 한컴 출력 페이지 | 마지막 페이지 출력 | 표/셀 배치 | 꼬리말 페이지수 색 | rhwp-studio 판정 | 비고 |
|---|---|---|---|---|---|---|---|
| 01_clear_table_attr_high_repeat_bit | 파일 읽기 실패 |  |  |  |  |  |  |
| 02_reference_table_record_attr_only | 파일 읽기 실패 |  |  |  |  |  |  |
| 03_reference_table_record_payload | 파일 읽기 실패 |  |  |  |  |  |  |

## 5. 판정 해석

TABLE attr/payload 축만으로는 한컴 파일 읽기 실패가 해소되지 않았다.

Stage 30의 성공 판정은 실제 구현 경로 전체가 아니라 Stage 27 baseline 위에서 만든 probe 판정이었다.
따라서 Stage 31 이후의 실제 HWPX -> adapter 산출물과 Stage 30 성공 variant를 직접 등치한 것은 부적절했다.

다음 단계는 `실제 구현 경로 산출물`과 `성공한 probe 기준선` 사이의 차이를 다시 정렬한다.
