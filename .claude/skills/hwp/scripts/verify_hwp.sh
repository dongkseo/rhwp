#!/usr/bin/env bash
# verify_hwp.sh — HWP 산출물 검증 게이트 (xlsx 스킬의 scripts/recalc.py 대응)
#
# 목적: 생성/편집한 .hwp 가 "손상 없이 한컴에서 열리는가"를 기계적으로 점검한다.
#   1) CFB(복합 파일) 시그니처 확인 — 진짜 .hwp 컨테이너인지
#   2) rhwp dump — IR 파싱이 깨지지 않는지 (구조 sanity)
#   3) (source 제공 시) rhwp convert <source> <out.hwp> --verify --verify-pages
#      → 저장-재로딩 IR 무차이(exit 3=IR diff) / 페이지 수 일치(exit 4) 게이트
#
# 사용법:
#   verify_hwp.sh <out.hwp> [source.hwp|source.hwpx]
#
# rhwp 바이너리 탐색 순서: $RHWP → PATH의 rhwp → ./target/release/rhwp → cargo run.
# rhwp 를 못 찾으면 status=rhwp_not_found 로 안내만 하고 비파괴적으로 종료(0).
#
# 출력: 마지막에 JSON 한 줄(status/cfb_ok/dump_ok/verify_exit/notes).
set -u

OUT="${1:-}"
SRC="${2:-}"

if [ -z "$OUT" ]; then
  echo "usage: verify_hwp.sh <out.hwp> [source.hwp|source.hwpx]" >&2
  exit 64
fi

notes=""
add_note() { notes="${notes}${notes:+; }$1"; }

# --- rhwp 바이너리 해석 -------------------------------------------------------
RHWP_BIN=""
if [ -n "${RHWP:-}" ] && command -v "$RHWP" >/dev/null 2>&1; then
  RHWP_BIN="$RHWP"
elif command -v rhwp >/dev/null 2>&1; then
  RHWP_BIN="rhwp"
elif [ -x "./target/release/rhwp" ]; then
  RHWP_BIN="./target/release/rhwp"
elif command -v cargo >/dev/null 2>&1; then
  RHWP_BIN="cargo run --release --quiet --bin rhwp --"
fi

# --- 1) CFB 시그니처 확인 (D0 CF 11 E0 A1 B1 1A E1) --------------------------
cfb_ok=false
if [ -f "$OUT" ]; then
  magic=$(head -c 8 "$OUT" | od -An -tx1 | tr -d ' \n')
  if [ "$magic" = "d0cf11e0a1b11ae1" ]; then
    cfb_ok=true
  else
    add_note "CFB 시그니처 불일치(got=$magic) — .hwp 컨테이너가 아닐 수 있음"
  fi
else
  add_note "출력 파일 없음: $OUT"
fi

# --- rhwp 미탐색 시 조기 종료 ------------------------------------------------
if [ -z "$RHWP_BIN" ]; then
  add_note "rhwp 바이너리를 찾지 못함 — 'cargo build --release' 후 재실행 권장"
  printf '{"status":"rhwp_not_found","cfb_ok":%s,"dump_ok":null,"verify_exit":null,"notes":"%s"}\n' \
    "$cfb_ok" "$notes"
  exit 0
fi

# --- 2) rhwp dump — 구조 파싱 sanity ----------------------------------------
dump_ok=false
if $RHWP_BIN dump "$OUT" >/dev/null 2>&1; then
  dump_ok=true
else
  add_note "rhwp dump 실패 — IR 파싱 단계에서 손상 의심"
fi

# --- 3) source 제공 시 convert --verify 게이트 -------------------------------
verify_exit="null"
if [ -n "$SRC" ]; then
  if [ ! -f "$SRC" ]; then
    add_note "source 파일 없음: $SRC (verify 게이트 건너뜀)"
  else
    $RHWP_BIN convert "$SRC" "$OUT" --verify --verify-pages
    verify_exit=$?
    case "$verify_exit" in
      0) add_note "convert --verify 통과: IR/페이지 무차이" ;;
      3) add_note "IR 차이 감지(exit 3) — 산출물이 원본과 다름" ;;
      4) add_note "페이지 수 불일치(exit 4)" ;;
      *) add_note "convert 실패(exit $verify_exit)" ;;
    esac
  fi
else
  add_note "source 미제공 — convert --verify 게이트 생략(dump/CFB만 확인)"
fi

# --- 종합 판정 ---------------------------------------------------------------
status="ok"
if [ "$cfb_ok" != "true" ] || [ "$dump_ok" != "true" ]; then
  status="errors_found"
fi
if [ "$verify_exit" != "null" ] && [ "$verify_exit" != "0" ]; then
  status="errors_found"
fi

printf '{"status":"%s","cfb_ok":%s,"dump_ok":%s,"verify_exit":%s,"notes":"%s"}\n' \
  "$status" "$cfb_ok" "$dump_ok" "$verify_exit" "$notes"

if [ "$status" = "ok" ]; then exit 0; else exit 1; fi
