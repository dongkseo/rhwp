#!/usr/bin/env node
// verify_hwp.mjs — HWP 산출물 검증 게이트 (xlsx 스킬의 scripts/recalc.py 대응)
//
// openpyxl 이 만든 xlsx 가 "계산되지 않은 수식"을 담듯, 편집·생성한 .hwp 도
// 저장 직후에는 한컴에서 열리는지가 확정되지 않는다. 이 스크립트가 그 게이트다.
//
//   node scripts/verify_hwp.mjs <파일.hwp>
//
// 검사 항목:
//   1. CFB 시그니처 — 진짜 HWP 5.0 복합 파일 컨테이너인가
//   2. 재파싱      — 다시 열었을 때 IR 이 깨지지 않는가
//   3. 라운드트립  — exportHwpVerify() 로 자기 재로드 + 페이지 보존 확인
//
// 이 스크립트는 대상 파일을 절대 수정하지 않는다 (읽기 전용).
// 출력: JSON 한 줄. 정상 exit 0, 오류 발견 exit 1.

import { readFileSync } from 'node:fs';
import { loadWasm } from './hwp/loader.mjs';

const CFB_SIGNATURE = 'd0cf11e0a1b11ae1';

const path = process.argv[2];
if (!path) {
  console.error('usage: node scripts/verify_hwp.mjs <파일.hwp>');
  process.exit(64);
}

const result = {
  status: 'success',
  file: path,
  cfb_ok: false,
  reparse_ok: false,
  page_count: null,
  roundtrip: null,
};
const errors = [];

try {
  const bytes = readFileSync(path);

  // 1) CFB 시그니처
  const sig = bytes.subarray(0, 8).toString('hex');
  result.cfb_ok = sig === CFB_SIGNATURE;
  if (!result.cfb_ok) {
    errors.push({
      type: 'not_cfb',
      detail: `CFB 시그니처 불일치 (got=${sig}, want=${CFB_SIGNATURE}) — .hwp 컨테이너가 아니다`,
    });
  }

  // 2) 재파싱
  const { HwpDocument } = await loadWasm();
  let doc;
  try {
    doc = new HwpDocument(bytes);
    result.reparse_ok = true;
    result.page_count = doc.pageCount();
  } catch (e) {
    errors.push({ type: 'reparse_failed', detail: `재파싱 실패 — IR 손상 의심: ${e}` });
  }

  // 3) 라운드트립 자기 재로드 검증
  if (doc) {
    try {
      const v = JSON.parse(doc.exportHwpVerify());
      result.roundtrip = v;
      if (!v.recovered) {
        errors.push({ type: 'not_recovered', detail: '자기 재로드 실패 — 저장 후 다시 열 수 없다' });
      }
      if (v.pageCountBefore !== v.pageCountAfter) {
        errors.push({
          type: 'page_count_mismatch',
          detail: `페이지 수 불일치: ${v.pageCountBefore} → ${v.pageCountAfter}`,
        });
      }
    } catch (e) {
      errors.push({ type: 'verify_failed', detail: `exportHwpVerify 실패: ${e}` });
    }
  }
} catch (e) {
  errors.push({ type: 'io_error', detail: String(e) });
}

if (errors.length) {
  result.status = 'errors_found';
  result.total_errors = errors.length;
  result.error_summary = errors;
}

console.log(JSON.stringify(result, null, 2));
process.exit(errors.length ? 1 : 0);
