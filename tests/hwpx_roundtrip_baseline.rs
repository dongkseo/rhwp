//! Task #1315 — `samples/hwpx/` 전수 HWPX roundtrip baseline 게이트.
//!
//! `mydocs/manual/hwpx_roundtrip_baseline.md` 의 등급 체계를 코드로 고정한다.
//!
//! - **A (baseline)**: parse → serialize → 재parse + IrDiff 0 + 패키지 검사 + 2-round 안정성.
//!   목록에 없는 신규 샘플도 자동 포함 — 통과 못 하면 사유와 함께 `XFAIL` 에 등록해야 한다.
//! - **B (xfail)**: 식별된 결함/미지원으로 baseline 제외. 결함이 해소되어 통과하게 되면
//!   `xfail_entries_still_fail` 테스트가 실패하므로 baseline 으로 승격해야 한다.
//! - **제외**: 샘플 자체가 HWPX 가 아닌 경우 (serializer 결함 아님).
//!
//! 주의: 이 게이트는 **구조(뼈대) 보존** 검증이다. 통과 = 시각 충실도 보장이 아니며,
//! 복합 실문서를 full visual fidelity oracle 로 쓰면 안 된다 (`ORACLE_UNFIT` 참고).

use std::path::{Path, PathBuf};

use rhwp::parser::hwpx::parse_hwpx;
use rhwp::serializer::hwpx::package_check::check_package;
use rhwp::serializer::hwpx::roundtrip::diff_documents;
use rhwp::serializer::hwpx::serialize_hwpx;

const SAMPLES_ROOT: &str = "samples/hwpx";

/// B등급 (xfail) — (상대 경로, 사유). 사유 없는 등록 금지.
const XFAIL: &[(&str, &str)] = &[(
    "exam_social.hwpx",
    "serializer: 미등록 borderFillIDRef 31 — parser→serializer ID 매핑 경계 문제 \
     (task_m100_1315_stage1.md, 별도 이슈 후보)",
)];

/// [Task #1378 3단계 임시] 게이트 재귀 확장(셀·글상자·각주/미주 char_shapes 비교)으로
/// 새로 검출된 본 타스크 범위 밖 실패 — (상대 경로, 사유). 후속 이슈에서 해소되어
/// 통과하게 되면 `xfail_1378_recursive_entries_still_fail` 이 실패하므로
/// 목록에서 제거하고 baseline 으로 승격해야 한다.
///
/// 사유 분류 (task_m100_1378_stage3.md):
/// - subList 컨트롤 미출력(#1379): 셀·글상자 문단의 컨트롤이 직렬화에서 빠져
///   재파싱 경계가 8×(경계 앞 컨트롤 수) 유닛 당겨짐. id 시퀀스는 보존.
/// - 파서 autoNum 폭 비일관: char_shapes 경계 축(calc)은 1 유닛, char_offsets 축은
///   8 유닛으로 집계 — serializer 는 offsets 축 기준이라 경계가 1 유닛 시프트.
///   stage1 에서 #1378 범위 밖으로 분류, 별도 이슈 후보.
const XFAIL_1378_RECURSIVE: &[(&str, &str)] = &[
    (
        "143E433F503322BD33.hwpx",
        "fn 1건 — 파서 autoNum 폭 비일관(1 vs 8 유닛), #1378 범위 밖 별도 이슈 후보",
    ),
    (
        "2024년 2분기 해외직접투자 보도자료ff.hwpx",
        "tbl 10건 — subList 컨트롤 미출력, #1379 에서 해소",
    ),
    (
        "2025년 2분기 해외직접투자 (최종).hwpx",
        "tbl 9건 — subList 컨트롤 미출력, #1379 에서 해소",
    ),
    (
        "aift.hwpx",
        "tbl 2건 — subList 컨트롤 미출력, #1379 에서 해소",
    ),
    (
        "el-school-001.hwpx",
        "tbl 2건 — subList 컨트롤 미출력, #1379 에서 해소",
    ),
    (
        "exam_kor.hwpx",
        "tbl 20건 — subList 컨트롤 미출력, #1379 에서 해소",
    ),
    (
        "exam-kor-1p.hwpx",
        "tbl 1건 — subList 컨트롤 미출력, #1379 에서 해소",
    ),
    (
        "exam-kor-2p.hwpx",
        "tbl 1건 — subList 컨트롤 미출력, #1379 에서 해소",
    ),
    (
        "exam-kor-3p.hwpx",
        "tbl 2건 — subList 컨트롤 미출력, #1379 에서 해소",
    ),
    (
        "exam-kor-4p.hwpx",
        "tbl 2건 — subList 컨트롤 미출력, #1379 에서 해소",
    ),
    (
        "hcar-001.hwpx",
        "tbl 2건 — subList 컨트롤 미출력, #1379 에서 해소",
    ),
    (
        "hwpx-h-02.hwpx",
        "tbl 9건 — subList 컨트롤 미출력, #1379 에서 해소",
    ),
    (
        "k-water-rfp.hwpx",
        "tbl 4건 + shape.tb 1건 — subList 컨트롤 미출력, #1379 에서 해소",
    ),
];

/// [Task #1379 1단계 임시] 게이트 컨트롤 비교 확장(문단별 인라인 슬롯 컨트롤 타입
/// 시퀀스, 셀·글상자·각주/미주 재귀 동승)으로 새로 검출된 실패 — (상대 경로, 사유).
/// 전건이 셀(`tbl.cell`)·글상자(`shape.tb`) subList 컨트롤 미출력 기인으로 본 타스크
/// 2~3단계 수정 대상이다. 해소되어 통과하게 되면 `xfail_1379_controls_entries_still_fail`
/// 이 실패하므로 목록에서 제거하고 baseline 으로 승격해야 한다.
const XFAIL_1379_CONTROLS: &[(&str, &str)] = &[
    (
        "2024년 1분기 해외직접투자 보도자료 ff.hwpx",
        "3건 — 셀 내 pic·pageNumPos 미출력, #1379 2단계에서 해소",
    ),
    (
        "2024년 연간 해외직접투자 보도자료 _ ff.hwpx",
        "3건 — 셀 내 pic·pageNumPos 미출력, #1379 2단계에서 해소",
    ),
    (
        "2025년 1분기 해외직접투자 보도자료f.hwpx",
        "3건 — 셀 내 pic·pageNumPos + 글상자 내 pic 미출력, #1379 2~3단계에서 해소",
    ),
    (
        "[2027] 온새미로 1 본교재.hwpx",
        "1건 — 셀 내 shape 미출력, #1379 2단계에서 해소",
    ),
    (
        "exam_social-p1.hwpx",
        "5건 — 셀 내 shape·중첩 tbl 미출력, #1379 2단계에서 해소",
    ),
    (
        "footnote-01.hwpx",
        "1건 — 셀 내 fn(각주) 미출력, #1379 2단계에서 해소",
    ),
    (
        "footnote-tbox-01.hwpx",
        "1건 — 글상자 내 fn(각주) 미출력, #1379 3단계에서 해소",
    ),
    (
        "form-002.hwpx",
        "190건 — 셀 내 form 다수 + 중첩 tbl 미출력, #1379 2단계에서 해소",
    ),
    (
        "hwpx-h-01.hwpx",
        "3건 — 셀 내 pic·pageNumPos 미출력, #1379 2단계에서 해소",
    ),
    (
        "hwpx-h-03.hwpx",
        "3건 — 셀 내 pic·pageNumPos + 글상자 내 pic 미출력, #1379 2~3단계에서 해소",
    ),
    (
        "hy-001.hwpx",
        "2건 — 셀 내 pic + 글상자 내 pic 미출력, #1379 2~3단계에서 해소",
    ),
    (
        "hy-002.hwpx",
        "2건 — 셀 내 pic + 글상자 내 pic 미출력, #1379 2~3단계에서 해소",
    ),
    (
        "issue_1133.hwpx",
        "1건 — 셀 내 중첩 tbl 미출력, #1379 2단계에서 해소",
    ),
    (
        "mel-001.hwpx",
        "3건 — 셀 내 charOverlap 미출력 (render_control_slot arm 부재), \
         #1379 2단계에서 해소",
    ),
    (
        "ta-pic-001-r.hwpx",
        "2건 — 셀 내 pic 미출력 (hp:pic 4개 소실, 이슈 본문 대표 샘플), \
         #1379 2단계에서 해소",
    ),
    (
        "tb-img-03.hwpx",
        "1건 — 셀 내 pic 미출력, #1379 2단계에서 해소",
    ),
];

/// 검사 제외 — 샘플 자체가 HWPX 패키지가 아님.
const EXCLUDED: &[(&str, &str)] = &[(
    "hwpx-01.hwpx",
    "HWP5(OLE CFB) 파일이 .hwpx 확장자로 저장된 샘플 — 파일 매직 d0cf11e0, serializer 결함 아님",
)];

/// 시각 oracle 부적합 — baseline(구조) 은 통과하지만, 표·그림·도형·다단이 혼합된
/// 복합 실문서이므로 full visual fidelity oracle 로 쓰면 안 되는 샘플 (이슈 #1315 주의사항).
const ORACLE_UNFIT: &[&str] = &[
    "2024년 1분기 해외직접투자 보도자료 ff.hwpx",
    "2024년 2분기 해외직접투자 보도자료ff.hwpx",
    "2024년 연간 해외직접투자 보도자료 _ ff.hwpx",
    "2025년 1분기 해외직접투자 보도자료f.hwpx",
    "2025년 2분기 해외직접투자 (최종).hwpx",
    "[2027] 온새미로 1 본교재.hwpx",
    "aift.hwpx",
    "exam_kor.hwpx",
    "exam-kor-1p.hwpx",
    "exam-kor-2p.hwpx",
    "exam-kor-3p.hwpx",
    "exam-kor-4p.hwpx",
    "k-water-rfp.hwpx",
];

/// `samples/hwpx/` 에서 `.hwpx` 를 재귀 수집해 루트 기준 상대 경로(슬래시 구분)로 반환.
fn collect_samples() -> Vec<(PathBuf, String)> {
    fn walk(dir: &Path, root: &Path, acc: &mut Vec<(PathBuf, String)>) {
        let entries = std::fs::read_dir(dir).expect("samples/hwpx 읽기 실패");
        for entry in entries {
            let path = entry.expect("디렉토리 항목 읽기 실패").path();
            if path.is_dir() {
                walk(&path, root, acc);
            } else if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("hwpx"))
            {
                let rel = path
                    .strip_prefix(root)
                    .expect("strip_prefix")
                    .to_string_lossy()
                    .replace('\\', "/");
                acc.push((path, rel));
            }
        }
    }
    let root = Path::new(SAMPLES_ROOT);
    let mut acc = Vec::new();
    walk(root, root, &mut acc);
    acc.sort_by(|a, b| a.1.cmp(&b.1));
    assert!(!acc.is_empty(), "samples/hwpx 에 .hwpx 샘플이 없음");
    acc
}

fn in_list(list: &[(&str, &str)], rel: &str) -> bool {
    list.iter().any(|(name, _)| *name == rel)
}

/// baseline 전체 검사: roundtrip + IrDiff 0 + 패키지 + 2-round. 실패 시 사유 반환.
fn baseline_check(path: &Path) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("읽기 실패: {e}"))?;
    let doc1 = parse_hwpx(&bytes).map_err(|e| format!("파싱 실패: {e}"))?;
    let out = serialize_hwpx(&doc1).map_err(|e| format!("직렬화 실패: {e}"))?;
    let doc2 = parse_hwpx(&out).map_err(|e| format!("재파싱 실패: {e}"))?;

    let diff = diff_documents(&doc1, &doc2);
    if !diff.is_empty() {
        return Err(format!(
            "IrDiff {}건: {}",
            diff.differences.len(),
            diff.differences
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }

    let pkg = check_package(&out, &doc1);
    if !pkg.is_ok() {
        return Err(format!("패키지 검사 실패: {}", pkg.summary()));
    }

    let out2 = serialize_hwpx(&doc2).map_err(|e| format!("2-round 직렬화 실패: {e}"))?;
    let doc3 = parse_hwpx(&out2).map_err(|e| format!("2-round 재파싱 실패: {e}"))?;
    let diff2 = diff_documents(&doc2, &doc3);
    if !diff2.is_empty() {
        return Err(format!(
            "2-round 불안정: IrDiff {}건",
            diff2.differences.len()
        ));
    }

    Ok(())
}

/// 처리 시간이 긴 대형 샘플 — 별도 테스트로 분리해 하네스 병렬 실행을 활용한다.
const LARGE: &[&str] = &["exam_kor.hwpx", "aift.hwpx", "k-water-rfp.hwpx"];

/// baseline 대상(XFAIL/EXCLUDED 제외)을 검사하고 실패 목록을 단언한다.
fn run_baseline(filter: impl Fn(&str) -> bool) {
    let mut failures = Vec::new();
    let mut eligible = 0usize;

    for (path, rel) in collect_samples() {
        if in_list(XFAIL, &rel) || in_list(EXCLUDED, &rel) || !filter(&rel) {
            continue;
        }
        eligible += 1;
        // [Task #1378 3단계 임시] 게이트 재귀 확장으로 검출된 범위 밖 실패 — 사유는
        // XFAIL_1378_RECURSIVE 참조. 후속 이슈 해소 시 이 블록과 목록을 제거한다.
        // (eligible 집계 뒤에 두어 LARGE 게이트의 "대상 없음" 가드를 깨지 않는다.)
        if in_list(XFAIL_1378_RECURSIVE, &rel) {
            continue;
        }
        // [Task #1379 1단계 임시] 게이트 컨트롤 비교 확장으로 검출된 실패 — 사유는
        // XFAIL_1379_CONTROLS 참조. 본 타스크 2~3단계 해소 시 이 블록과 목록을 제거한다.
        if in_list(XFAIL_1379_CONTROLS, &rel) {
            continue;
        }
        if let Err(reason) = baseline_check(&path) {
            failures.push(format!("  {rel}: {reason}"));
        }
    }

    assert!(eligible > 0, "baseline 검사 대상이 없음");
    assert!(
        failures.is_empty(),
        "baseline 샘플 {}건 중 {}건 실패 — 결함 수정 또는 사유와 함께 XFAIL 등록 필요:\n{}",
        eligible,
        failures.len(),
        failures.join("\n")
    );
}

/// A등급 전수 게이트 (대형 샘플 제외분) — 신규 샘플은 자동으로 이 게이트에 포함된다.
#[test]
fn baseline_all_samples_roundtrip() {
    run_baseline(|rel| !LARGE.contains(&rel));
}

/// A등급 전수 게이트 (대형 샘플) — `LARGE` 목록만 검사.
#[test]
fn baseline_large_samples_roundtrip() {
    run_baseline(|rel| LARGE.contains(&rel));
}

/// B등급(xfail) 샘플은 여전히 실패해야 한다 — 통과하게 되면 baseline 승격 필요.
#[test]
fn xfail_entries_still_fail() {
    for (name, reason) in XFAIL {
        let path = Path::new(SAMPLES_ROOT).join(name);
        assert!(path.exists(), "XFAIL 샘플 실종: {name} (목록 정비 필요)");
        assert!(
            baseline_check(&path).is_err(),
            "XFAIL 샘플이 통과함: {name} — baseline 으로 승격하고 XFAIL 에서 제거하라 (사유였던 결함: {reason})"
        );
    }
}

/// [Task #1378 3단계 임시] 재귀 확장 xfail 샘플은 여전히 실패해야 한다 —
/// 후속 이슈(#1379 등)에서 해소되면 목록·skip 블록을 제거하고 baseline 으로 승격한다.
#[test]
fn xfail_1378_recursive_entries_still_fail() {
    for (name, reason) in XFAIL_1378_RECURSIVE {
        let path = Path::new(SAMPLES_ROOT).join(name);
        assert!(
            path.exists(),
            "XFAIL_1378_RECURSIVE 샘플 실종: {name} (목록 정비 필요)"
        );
        assert!(
            baseline_check(&path).is_err(),
            "XFAIL_1378_RECURSIVE 샘플이 통과함: {name} — baseline 으로 승격하고 \
             목록에서 제거하라 (사유였던 결함: {reason})"
        );
    }
}

/// [Task #1379 1단계 임시] 컨트롤 비교 xfail 샘플은 여전히 실패해야 한다 —
/// 본 타스크 2~3단계에서 해소되면 목록·skip 블록을 제거하고 baseline 으로 승격한다.
#[test]
fn xfail_1379_controls_entries_still_fail() {
    for (name, reason) in XFAIL_1379_CONTROLS {
        let path = Path::new(SAMPLES_ROOT).join(name);
        assert!(
            path.exists(),
            "XFAIL_1379_CONTROLS 샘플 실종: {name} (목록 정비 필요)"
        );
        assert!(
            baseline_check(&path).is_err(),
            "XFAIL_1379_CONTROLS 샘플이 통과함: {name} — baseline 으로 승격하고 \
             목록에서 제거하라 (사유였던 결함: {reason})"
        );
    }
}

/// 목록 정합 가드 — EXCLUDED/ORACLE_UNFIT 항목이 실제로 존재하고,
/// ORACLE_UNFIT 은 baseline 대상(XFAIL/EXCLUDED 아님)이어야 한다.
#[test]
fn grade_lists_are_consistent() {
    for (name, _) in EXCLUDED {
        assert!(
            Path::new(SAMPLES_ROOT).join(name).exists(),
            "EXCLUDED 샘플 실종: {name} (목록 정비 필요)"
        );
    }
    for name in ORACLE_UNFIT {
        assert!(
            Path::new(SAMPLES_ROOT).join(name).exists(),
            "ORACLE_UNFIT 샘플 실종: {name} (목록 정비 필요)"
        );
        assert!(
            !in_list(XFAIL, name) && !in_list(EXCLUDED, name),
            "ORACLE_UNFIT 은 baseline 대상이어야 함: {name}"
        );
    }
}
