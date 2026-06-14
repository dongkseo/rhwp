# HWPX serializer — newNum 슬롯이 텍스트 뒤가 아니라 앞으로 방출 (Task #1407)

## 증상

`143E433F503322BD33.hwpx` roundtrip 후 ir-diff:

```
--- 문단 0.14 --- "김영훈 기자(jcomm@sanggongnews.com)"
  [차이] char_offsets[3]: A=27 vs B=35
```

dump 로는 컨트롤 3개(머리말·Hyperlink 필드·새번호)의 종류·순서·char_shapes 가
원본·RT 동일 → 차이는 **컨트롤의 inline 위치(char_offsets)** 에만 있다.

## 근본 원인 — 메인 루프가 fieldEnd 자리를 newNum 으로 가로챔

문단 0.14 원본 IR:

- text = "김영훈 기자(jcomm@…)" (30 chars)
- char_offsets = `[16,17,18, 27,28,…,53]`
- controls = `[Header(0), Field/Hyperlink(1), NewNumber(2)]`
- field_ranges = `[start=0 end=3 control_idx=1]` (하이퍼링크가 "김영훈" 3글자 래핑)

위치 해석:
- 0~15: Header 슬롯(8) + fieldBegin(8) = 16유닛 → "김영훈" 16,17,18
- "김영훈"(18) 다음 갭 19→27 = **8유닛 = fieldEnd**(field_ranges 유래, controls 에 없음)
- 27부터 " 기자(…)" 텍스트
- newNum 은 원본 XML 상 **텍스트 끝**(pos 54) — `<hp:t>…</hp:t><hp:ctrl><hp:newNum/></hp:ctrl>`

`render_runs` 메인 루프(`section.rs:431~`) 슬롯 방출 조건:

```rust
while slot_idx < slots.len() && char_pos >= expected_utf16_pos.saturating_add(8) {
    render_control_slot(... slots[slot_idx] ...);  // slot_idx 순서대로 소비
}
```

idx=3(" ") 에서 char_pos=27 ≥ expected(16)+8 → **slots[2]=NewNumber 를 방출**.
그러나 그 27 자리에 와야 할 것은 **fieldEnd**(별도 처리, line 525)다. 슬롯 방출(437)
이 fieldEnd 방출(525)보다 먼저라 newNum 이 fieldEnd 자리(27)를 가로채고, 이후 모든
텍스트가 +8 밀려 char_offsets[3] 27→35.

핵심: 메인 루프는 "다음 8유닛 갭이 보이면 controls 순서대로 다음 슬롯 방출" 방식이라,
**텍스트 끝에 위치한 슬롯(newNum)** 과 **텍스트 중간의 fieldEnd 갭**을 구분하지 못한다.
autoNum(#1382)은 placeholder 공백으로 슬롯 위치가 char_offsets 에 명시돼 정확히 잡히나,
newNum 은 placeholder 가 없어(파서 `section.rs:3891` 주석: newNum 은 text/offsets 미 push)
위치 정보가 char_offsets 에 남지 않는다 → controls 배열 순서 + 8유닛 갭 추론에만 의존.

## RT 페이지 수 1→2 (증상 ②)

원본 1페이지가 RT 에서 2페이지. tbl pageBreak(#1393) 패치로도 불변. newNum(PAGE 새번호 2)
의 적용 위치가 텍스트 앞으로 이동하면서 페이지 번호/머리말 적용에 영향을 줄 가능성 —
슬롯 위치 정정 후 재현 여부로 귀속 확정 대상.

## 재발 방지 체크리스트

- [ ] placeholder 없는 inline 슬롯(newNum/pageHide/pageNum 등)이 **텍스트 끝**에 올 때,
      메인 루프가 텍스트 중간 갭(fieldEnd 등)으로 가로채지 않는지 점검
- [ ] field_ranges(fieldEnd) 갭과 controls 슬롯의 우선순위 — fieldEnd 가 먼저 소비돼야
      하는 위치인지 char_offsets 로 판별

## 관련

- Task #1407 (본 건), #1382(autoNum placeholder 해소 — 별개 계열), #1379/#1380(RT 인프라)
- `mydocs/report/task_m100_1382_report.md` 4절 (시대별 RT 대조 실증)
