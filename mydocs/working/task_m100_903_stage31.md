# Task m100 #903 Stage 31

## 1. 단계 목적

Stage 30에서 확정한 최소 조건을 실제 구현에 반영한 뒤, `hwpx-h-01.hwpx` 전체 저장 결과를 검증한다.

구현 반영 항목:

```text
1. HWPX -> HWP adapter에서 DocProperties.section_count를 실제 section 개수로 보정
2. HWPX header parser에서 paraPr/margin 자식 요소형 값을 ParaShape margin 필드로 매핑
```

## 2. 기준 파일

입력 HWPX:

```text
samples/hwpx/hwpx-h-01.hwpx
```

한컴 정답 HWP:

```text
samples/hwpx/hancom-hwp/hwpx-h-01.hwp
```

Stage 31 산출물:

```text
output/poc/hwpx2hwp/task903/stage31_impl_verify/hwpx-h-01.hwp
```

## 3. 내부 검증

Targeted tests:

```text
cargo test --test hwpx_to_hwp_adapter task903_hwpx_h_01 -- --nocapture
cargo test --test hwpx_to_hwp_adapter task903_stage31_generate_impl_verify_hwpx_h_01 -- --nocapture
```

검증 포인트:

```text
- HWPX 원본의 XML entity 텍스트 보존
- embedded BinData 저장/재로드 보존
- 그림/묶음 속성 저장/재로드 보존
- DocProperties.section_count = 2 보정
- ParaShape margin child 값 파싱
- rhwp-studio 재로드 기준 9페이지 유지
```

결과:

```text
cargo test --test hwpx_to_hwp_adapter task903_hwpx_h_01 -- --nocapture
=> ok. 9 passed; 0 failed

cargo test --test hwpx_to_hwp_adapter task903_stage31_generate_impl_verify_hwpx_h_01 -- --nocapture
=> ok. 1 passed; generated hwpx-h-01.hwp, bytes=680448, pages=9

cargo test --test hwpx_to_hwp_adapter task903_stage30_generate_minimal_docinfo_probe_variants -- --nocapture
=> ok. 1 passed

cargo run --bin rhwp -- ir-diff samples/hwpx/hwpx-h-01.hwpx samples/hwpx/hancom-hwp/hwpx-h-01.hwp -s 0 -p 102
=> 비교 완료: 차이 0 건
```

## 4. 작업지시자 판정 요청

다음 파일을 한컴 에디터와 rhwp-studio에서 판정한다.

```text
output/poc/hwpx2hwp/task903/stage31_impl_verify/hwpx-h-01.hwp
```

판정 항목:

```text
- 한컴 에디터 파일손상/파일 읽기 오류가 없는지
- 한컴 에디터에서 9페이지 마지막 페이지가 출력되는지
- 표/셀 세로 배치가 Stage 30 정상 variant와 같은지
- rhwp-studio에서 9페이지로 재로드되는지
- 꼬리말 페이지수가 정상 기준인 검정색인지, 기존 결함인 빨간색이 남는지
```

판정 기록:

| 파일 | 한컴 판정 유형 | 한컴 출력 페이지 | 마지막 페이지 출력 | 표/셀 배치 | rhwp-studio 판정 | 비고 |
|---|---|---|---|---|---|---|
| hwpx-h-01.hwp | 파일 읽기 오류 |  |  |  |  | Stage 31 실제 구현 산출물은 한컴 로더에서 거부됨 |

## 5. 남은 축

꼬리말 페이지수 빨간색 현상은 처음부터 존재한 비정상 상태다.
정상 기준은 검정색이다.

```text
이번 stage에서는 파일 손상, 마지막 페이지 누락, 표/셀 배치를 우선 닫고
꼬리말 색상 문제는 기존 결함으로 별도 후속 축에서 닫는다.
```

## 6. 판정 해석

Stage31은 Stage30에서 확정한 두 구현 항목을 실제 코드 경로에 반영한 검증 단계다.

반영된 항목:

```text
1. HWPX -> HWP adapter에서 DocProperties.section_count를 실제 section 개수로 보정
2. HWPX header parser에서 paraPr/margin 자식 요소형 값을 ParaShape margin 필드로 매핑
```

내부 검증 결과:

```text
- DocProperties.section_count = 2
- ParaShape margin child 값 파싱 확인
- rhwp-studio 재로드 기준 9페이지 유지
- 특정 문단의 ir-diff 결과 차이 0건
```

하지만 작업지시자 판정 결과, 한컴 에디터는 Stage31 산출물을 `파일 읽기 오류`로 거부했다.

이 결과는 Stage30의 결론을 부정하지 않는다.

Stage30이 검증한 범위:

```text
Stage27 baseline 수준의 BodyText/table/object 구조가 이미 갖춰져 있을 때,
section_count + ParaShape 보정으로
마지막 페이지 출력과 표/셀 배치 문제가 해결되는지
```

Stage31이 새로 드러낸 범위:

```text
clean HWPX -> IR -> HWP 저장 경로에서
한컴 에디터가 요구하는 BodyText table/object record tuple이 충분히 직렬화되는지
```

따라서 Stage31 이후의 주 해결 대상은 다음으로 재정의한다.

```text
clean adapter 산출물의 한컴 파일 읽기 오류
= BodyText table/object record 직렬화 호환성 문제
```

## 7. Stage31 재시작 기준

Stage31부터 다시 진행할 때의 기준은 다음과 같다.

```text
- Stage30 구현 항목(section_count, ParaShape margin parsing)은 유지한다.
- Stage31 산출물의 한컴 파일 읽기 오류를 새로운 주 문제로 둔다.
- 꼬리말 페이지수 빨간색은 별도 결함으로 분리한다.
- rhwp-studio 내부 재로드 성공만으로 한컴 호환성을 판단하지 않는다.
```

다음 단계의 목표:

```text
Stage31 실패 산출물과 한컴 정답 HWP를 비교하여,
한컴 로더가 요구하는 table/object record tuple 중 clean adapter가 빠뜨린 최소 필드군을 식별한다.
```
