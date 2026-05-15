# Task m100 #903 Stage 34

## 1. 단계 목적

Stage 33에서 TABLE attr/payload 3개 variant가 모두 한컴 에디터에서 파일 읽기 실패로 판정되었다.

이 결과는 Stage 31 이후의 clean adapter 산출물과 Stage 30 성공 판정을 직접 연결한 접근이 잘못되었음을 의미한다.

핵심 재정렬:

- Stage 30 성공 판정은 clean adapter 산출물이 아니라 Stage 27 baseline 위에서 만든 probe였다.
- Stage 27 baseline에는 여러 BodyText object/table/paragraph raw materialization이 이미 들어가 있었다.
- 따라서 먼저 Stage 30 계열의 성공 조건이 현재 코드에서도 재현되는지 확인해야 한다.

## 2. 산출물

```text
output/poc/hwpx2hwp/task903/stage34_baseline_reconcile/
```

## 3. Variant

| variant | 목적 |
|---|---|
| 01_clean_adapter_compressed | 현재 clean HWPX -> adapter -> compressed HWP 기준선 |
| 02_clean_adapter_plus_section_count_para_shapes_no_raw | clean adapter에 Stage 30 최소 성공 축(`section_count + ParaShape`)만 적용 |
| 03_clean_adapter_plus_reference_docinfo_no_raw | clean adapter에 정답 HWP DocInfo 모델 전체를 재직렬화 적용 |
| 04_stage27_baseline_plus_section_count_para_shapes_no_raw | Stage 27 baseline에 Stage 30 최소 성공 축을 재적용한 control |
| 05_stage27_baseline_plus_reference_docinfo_no_raw | Stage 27 baseline에 정답 HWP DocInfo 모델 전체를 재직렬화 적용한 control |

## 4. 생성 명령

```bash
cargo test --test hwpx_to_hwp_adapter task903_stage34_generate_baseline_reconcile_variants -- --nocapture
```

실행 결과:

```text
=> ok. 1 passed

01_clean_adapter_compressed.hwp: bytes=374272, pages=9, section_count=2
02_clean_adapter_plus_section_count_para_shapes_no_raw.hwp: bytes=374272, pages=9, section_count=2
03_clean_adapter_plus_reference_docinfo_no_raw.hwp: bytes=375808, pages=9, section_count=2
04_stage27_baseline_plus_section_count_para_shapes_no_raw.hwp: bytes=375808, pages=9, section_count=2
05_stage27_baseline_plus_reference_docinfo_no_raw.hwp: bytes=377344, pages=9, section_count=2
```

## 5. 작업지시자 판정 요청

다음 파일을 한컴 에디터와 rhwp-studio에서 판정한다.

```text
output/poc/hwpx2hwp/task903/stage34_baseline_reconcile/01_clean_adapter_compressed.hwp
output/poc/hwpx2hwp/task903/stage34_baseline_reconcile/02_clean_adapter_plus_section_count_para_shapes_no_raw.hwp
output/poc/hwpx2hwp/task903/stage34_baseline_reconcile/03_clean_adapter_plus_reference_docinfo_no_raw.hwp
output/poc/hwpx2hwp/task903/stage34_baseline_reconcile/04_stage27_baseline_plus_section_count_para_shapes_no_raw.hwp
output/poc/hwpx2hwp/task903/stage34_baseline_reconcile/05_stage27_baseline_plus_reference_docinfo_no_raw.hwp
```

판정 항목:

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
| 01_clean_adapter_compressed | 파일 읽기 오류 |  |  |  |  |  |  |
| 02_clean_adapter_plus_section_count_para_shapes_no_raw | 파일 읽기 오류 |  |  |  |  |  |  |
| 03_clean_adapter_plus_reference_docinfo_no_raw | 파일 읽기 오류 |  |  |  |  |  |  |
| 04_stage27_baseline_plus_section_count_para_shapes_no_raw | 정상 | 정상 | 정상 |  |  |  |  |
| 05_stage27_baseline_plus_reference_docinfo_no_raw | 정상 | 정상 | 정상 |  |  |  |  |

## 6. 해석 기준

예상 해석:

- 04/05가 정상이고 01~03이 실패하면, clean adapter BodyText/object materialization이 아직 Stage 27 baseline 수준에 도달하지 못한 것이다.
- 04/05도 실패하면, Stage 30 성공 조건이 현재 코드에서 재현되지 않는 것이므로 이전 성공 기준선부터 다시 고정해야 한다.
- 03이 정상이고 01/02가 실패하면, clean adapter의 주 원인은 BodyText보다 DocInfo 계열이다.

실제 판정:

- 01/02/03은 모두 파일 읽기 오류.
- 04/05는 정상.

결론:

- FileHeader/압축 축 아님.
- DocInfo 단독 축 아님.
- `section_count + ParaShape` 단독 축도 아님.
- Stage 27 baseline에 누적되어 있던 BodyText object/table/paragraph materialization 중 하나 이상의 블록이 한컴 호환성 회복에 필요하다.

다음 단계는 Stage 27 baseline의 누적 materialization을 블록 단위로 분리한다.
