//! HWPX → HWP IR 어댑터 통합 테스트 (#178)
//!
//! Stage 1: 베이스라인 측정 (페이지 폭주 + 영역별 차이 인벤토리).
//!         아직 어댑터 본체가 동작하지 않으므로 회복 검증 없음 — 측정만.

use rhwp::document_core::converters::diagnostics::diff_hwpx_vs_serializer_assumptions;
use rhwp::document_core::converters::hwpx_to_hwp::{
    convert_hwpx_to_hwp_ir, convert_if_hwpx_source,
};
use rhwp::document_core::DocumentCore;
use rhwp::model::bin_data::BinDataType;
use rhwp::model::control::Control;
use rhwp::model::paragraph::Paragraph;
use rhwp::model::shape::{
    CommonObjAttr, GroupShape, HorzRelTo, ShapeComponentAttr, ShapeObject, TextWrap, VertRelTo,
};
use rhwp::model::style::FillType;
use rhwp::model::table::{Table, TablePageBreak};
use rhwp::parser::cfb_reader::LenientCfbReader;
use rhwp::serializer::mini_cfb;

fn load_sample(name: &str) -> Vec<u8> {
    let path = format!("samples/hwpx/{}", name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("샘플 로드 실패 {}: {}", path, e))
}

fn page_count_after_hwp_export(hwpx_bytes: &[u8]) -> (u32, u32) {
    let core = DocumentCore::from_bytes(hwpx_bytes).expect("HWPX 로드 실패");
    let original_pages = core.page_count();

    let hwp_bytes = core.export_hwp_native().expect("HWP 직렬화 실패");

    let reloaded = DocumentCore::from_bytes(&hwp_bytes).expect("HWP 재로드 실패");
    let reloaded_pages = reloaded.page_count();

    (original_pages, reloaded_pages)
}

/// 베이스라인 측정: 현 단계는 페이지 폭주 (reloaded > orig) 가 발생하는 것이 "정상".
/// 어댑터 영역별 매핑이 누적되면서 폭주 비율이 줄고, Stage 5 완료 시점에는
/// reloaded == orig 가 되도록 게이트가 강화된다.
fn assert_explosion_baseline(name: &str, bytes: &[u8]) {
    let (orig, reloaded) = page_count_after_hwp_export(bytes);
    eprintln!(
        "[#178 baseline] {}: orig={}, reloaded={}",
        name, orig, reloaded
    );
    assert!(orig >= 1, "{}: 원본 페이지 수 측정 실패", name);
    assert!(
        reloaded > orig,
        "{}: 현 단계는 폭주가 발생해야 정상 (어댑터 미적용). orig={}, reloaded={}",
        name,
        orig,
        reloaded
    );
}

#[test]
fn baseline_page_count_explosion_hwpx_h_01() {
    assert_explosion_baseline("hwpx-h-01", &load_sample("hwpx-h-01.hwpx"));
}

#[test]
fn baseline_page_count_explosion_hwpx_h_02() {
    assert_explosion_baseline("hwpx-h-02", &load_sample("hwpx-h-02.hwpx"));
}

#[test]
fn baseline_page_count_explosion_hwpx_h_03() {
    let bytes = load_sample("hwpx-h-03.hwpx");
    let (orig, reloaded) = page_count_after_hwp_export(&bytes);
    eprintln!(
        "[#178 baseline] hwpx-h-03: orig={}, reloaded={}",
        orig, reloaded
    );
    // hwpx-h-03 은 폭주 여부 자체가 미확정 — 측정만 기록.
    assert!(orig >= 1);
    assert!(reloaded >= 1);
}

#[test]
fn baseline_diff_inventory_hwpx_h_01() {
    let bytes = load_sample("hwpx-h-01.hwpx");
    let core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");
    let summary = diff_hwpx_vs_serializer_assumptions(core.document());
    eprintln!("[#178 inventory] hwpx-h-01:\n{}", summary.human_report());
    // 영역별 카운트는 측정만. assert 는 의미있는 영역이 1개 이상 검출됐는지.
    let counts = summary.counts_by_area();
    let interesting = counts.iter().any(|(a, c)| {
        *c > 0
            && (*a == "table.raw_ctrl_data"
                || *a == "paragraph.line_seg.vertical_pos"
                || *a == "cell.list_attr.bit16")
    });
    assert!(
        interesting,
        "hwpx-h-01 에서 위반 영역이 검출돼야 함 (페이지 폭주가 발생하므로). counts={:?}",
        counts
    );
}

#[test]
fn adapter_deterministic_across_clones() {
    // 두 개의 동일 클론에 어댑터를 적용하면 결과가 같다 (결정론적 동작).
    let bytes = load_sample("hwpx-h-01.hwpx");
    let core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");

    let mut doc1 = core.document().clone();
    let mut doc2 = core.document().clone();

    let r1 = convert_hwpx_to_hwp_ir(&mut doc1);
    let r2 = convert_hwpx_to_hwp_ir(&mut doc2);
    assert_eq!(r1, r2);
}

#[test]
fn adapter_skips_hwp_source() {
    let mut doc = rhwp::model::document::Document::default();
    let report = convert_if_hwpx_source(&mut doc, rhwp::parser::FileFormat::Hwp);
    assert_eq!(
        report.skipped_reason.as_deref(),
        Some("source_format != Hwpx/Hwp3")
    );
}

// ============================================================
// Stage 2 — table.raw_ctrl_data 합성 검증
// ============================================================

#[test]
fn stage2_raw_ctrl_data_synthesized_for_hwpx_h_01() {
    let bytes = load_sample("hwpx-h-01.hwpx");
    let core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");

    // 어댑터 적용 전: raw_ctrl_data 가 모두 비어있어야 함 (HWPX 출처 특성)
    let mut empty_count_before = 0;
    for section in &core.document().sections {
        for para in &section.paragraphs {
            for ctrl in &para.controls {
                if let Control::Table(t) = ctrl {
                    if t.raw_ctrl_data.is_empty() {
                        empty_count_before += 1;
                    }
                }
            }
        }
    }
    assert!(
        empty_count_before > 0,
        "HWPX 출처에는 빈 raw_ctrl_data 가 있어야 함"
    );

    // 어댑터 적용
    let mut doc = core.document().clone();
    let report = convert_hwpx_to_hwp_ir(&mut doc);
    assert!(
        report.tables_ctrl_data_synthesized > 0,
        "어댑터가 ctrl_data 를 합성해야 함. report={:?}",
        report
    );

    // 어댑터 적용 후: 모든 표의 raw_ctrl_data 가 채워져 있어야 함
    let mut empty_count_after = 0;
    for section in &doc.sections {
        for para in &section.paragraphs {
            for ctrl in &para.controls {
                if let Control::Table(t) = ctrl {
                    if t.raw_ctrl_data.is_empty() {
                        empty_count_after += 1;
                    }
                }
            }
        }
    }
    assert_eq!(
        empty_count_after, 0,
        "어댑터 적용 후 모든 표는 raw_ctrl_data 가 채워져야 함"
    );
}

#[test]
fn stage2_diagnostics_no_longer_flag_table_ctrl_data() {
    let bytes = load_sample("hwpx-h-01.hwpx");
    let core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");
    let mut doc = core.document().clone();
    convert_hwpx_to_hwp_ir(&mut doc);

    let summary = diff_hwpx_vs_serializer_assumptions(&doc);
    let counts = summary.counts_by_area();
    let ctrl_data_count = counts
        .iter()
        .find(|(a, _)| *a == "table.raw_ctrl_data")
        .map(|(_, c)| *c)
        .unwrap_or(0);
    assert_eq!(
        ctrl_data_count, 0,
        "어댑터 적용 후 진단 도구가 table.raw_ctrl_data 위반을 보고하지 않아야 함. counts={:?}",
        counts
    );
}

#[test]
fn stage2_idempotent_does_not_double_synthesize() {
    let bytes = load_sample("hwpx-h-01.hwpx");
    let core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");
    let mut doc = core.document().clone();

    let r1 = convert_hwpx_to_hwp_ir(&mut doc);
    let r2 = convert_hwpx_to_hwp_ir(&mut doc);

    assert!(r1.tables_ctrl_data_synthesized > 0, "1차 호출 시 합성 발생");
    assert_eq!(
        r2.tables_ctrl_data_synthesized, 0,
        "2차 호출 시 합성 0 (idempotent)"
    );
}

#[test]
fn stage2_hwp_source_unchanged() {
    // HWP 원본 로드 → 어댑터 적용 → 표 raw_ctrl_data 가 변경되지 않아야 함
    // (HWP 출처는 raw_ctrl_data 가 이미 비어있지 않으므로 어댑터 가드에 막힘)
    let path = "samples/hwp_table_test.hwp";
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => {
            eprintln!("[skip] {} 없음", path);
            return;
        }
    };
    let core = DocumentCore::from_bytes(&bytes).expect("HWP 로드 실패");
    let mut doc = core.document().clone();

    // 어댑터 적용 전 raw_ctrl_data 스냅샷
    let snapshot_before: Vec<Vec<u8>> = doc
        .sections
        .iter()
        .flat_map(|s| s.paragraphs.iter())
        .flat_map(|p| p.controls.iter())
        .filter_map(|c| match c {
            Control::Table(t) => Some(t.raw_ctrl_data.clone()),
            _ => None,
        })
        .collect();

    convert_hwpx_to_hwp_ir(&mut doc);

    let snapshot_after: Vec<Vec<u8>> = doc
        .sections
        .iter()
        .flat_map(|s| s.paragraphs.iter())
        .flat_map(|p| p.controls.iter())
        .filter_map(|c| match c {
            Control::Table(t) => Some(t.raw_ctrl_data.clone()),
            _ => None,
        })
        .collect();

    assert_eq!(
        snapshot_before, snapshot_after,
        "HWP 출처 raw_ctrl_data 는 어댑터에 의해 변경되지 않아야 함"
    );
}

/// Stage 2 베이스라인 측정: 어댑터 적용 후 페이지 폭주 비율이 줄어야 함.
/// (완전 회복은 Stage 4 lineseg vpos 사전계산 후, 단계 회귀 측정 목적)
fn page_count_with_adapter(hwpx_bytes: &[u8]) -> (u32, u32) {
    let core = DocumentCore::from_bytes(hwpx_bytes).expect("HWPX 로드 실패");
    let original_pages = core.page_count();

    let mut doc = core.document().clone();
    convert_hwpx_to_hwp_ir(&mut doc);

    // 어댑터 적용된 doc 으로 직렬화 — DocumentCore 우회
    let hwp_bytes = rhwp::serializer::serialize_hwp(&doc).expect("직렬화 실패");

    let reloaded = DocumentCore::from_bytes(&hwp_bytes).expect("HWP 재로드 실패");
    let reloaded_pages = reloaded.page_count();

    (original_pages, reloaded_pages)
}

fn expected_hancom_hwp_page_count(name: &str, original_pages: u32) -> u32 {
    match name {
        // 한컴 2020 정답 HWP(`samples/hwpx/hancom-hwp/hwpx-h-02.hwp`)는 10페이지다.
        // rhwp-studio HWPX 렌더러의 원본 페이지 수 9와 다르므로, 저장 검증은
        // HWPX 렌더러 값이 아니라 한컴 HWP 저장 결과를 기준으로 둔다.
        "hwpx-h-02" | "hwpx-h-02.hwpx" => 10,
        _ => original_pages,
    }
}

#[test]
fn stage2_page_count_after_adapter_hwpx_h_01() {
    let bytes = load_sample("hwpx-h-01.hwpx");
    let (orig, after) = page_count_with_adapter(&bytes);
    let (_, before) = page_count_after_hwp_export(&bytes);
    eprintln!(
        "[#178 Stage 2] hwpx-h-01: orig={}, before_adapter={}, after_adapter={}",
        orig, before, after
    );
    // 회복 단계 — Stage 5 까지는 부분 개선만 기대.
    // 어댑터로 인해 폭주가 더 심해지면 Stage 2 가 잘못된 합성을 한 것이므로 실패.
    assert!(
        after <= before,
        "어댑터 적용 후 페이지 수가 더 늘면 회귀: before={} after={}",
        before,
        after
    );
}

#[test]
fn task888_basic_table_materializes_hancom_table_attrs() {
    let bytes = load_sample("basic-table-01.hwpx");
    let core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");
    let mut doc = core.document().clone();

    let report = convert_hwpx_to_hwp_ir(&mut doc);
    let table = doc
        .sections
        .iter()
        .flat_map(|s| s.paragraphs.iter())
        .flat_map(|p| p.controls.iter())
        .find_map(|ctrl| match ctrl {
            Control::Table(t) => Some(t),
            _ => None,
        })
        .expect("basic-table-01 표 없음");

    assert_eq!(report.table_ctrl_header_attr_materialized, 1);
    assert_eq!(report.table_record_attr_materialized, 1);
    assert_eq!(table.raw_table_record_attr, 0x0400_0006);
    assert_eq!(report.table_record_row_sizes_materialized, 1);
    assert_eq!(table.row_sizes, vec![4, 4, 4]);
    assert!(table.raw_ctrl_data.len() >= 4);
    assert_eq!(
        u32::from_le_bytes([
            table.raw_ctrl_data[0],
            table.raw_ctrl_data[1],
            table.raw_ctrl_data[2],
            table.raw_ctrl_data[3],
        ]),
        0x082a_2210
    );
}

#[test]
fn task888_expense_report_materializes_tac_table_ctrl_attrs() {
    let bytes = load_sample("expense_report.hwpx");
    let core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");
    let mut doc = core.document().clone();

    let report = convert_hwpx_to_hwp_ir(&mut doc);
    let mut tac_attrs = Vec::new();
    let mut tac_row_sizes = Vec::new();

    for section in &doc.sections {
        for para in &section.paragraphs {
            for ctrl in &para.controls {
                if let Control::Table(t) = ctrl {
                    if t.common.treat_as_char {
                        assert!(t.raw_ctrl_data.len() >= 4, "TAC table raw_ctrl_data");
                        let packed = u32::from_le_bytes([
                            t.raw_ctrl_data[0],
                            t.raw_ctrl_data[1],
                            t.raw_ctrl_data[2],
                            t.raw_ctrl_data[3],
                        ]);
                        assert_ne!(packed, 0, "TAC table CTRL_HEADER attr must be materialized");
                        assert_eq!(t.attr, packed);
                        tac_attrs.push(packed);
                        tac_row_sizes.push(t.row_sizes.clone());
                    }
                }
            }
        }
    }

    assert_eq!(tac_attrs.len(), 2, "expense_report TAC table count");
    assert_eq!(
        tac_row_sizes,
        vec![vec![5, 3, 3], vec![4, 1, 4, 3, 6, 1, 3, 1, 2]],
        "TAC table row_sizes must be row cell counts, not row heights"
    );
    assert_eq!(report.table_ctrl_header_attr_materialized, 2);
    assert_eq!(report.table_record_row_sizes_materialized, 2);
}

#[test]
fn task888_expense_report_normalizes_transparent_paragraph_border_fill() {
    let bytes = load_sample("expense_report.hwpx");
    let core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");
    let mut doc = core.document().clone();

    let report = convert_hwpx_to_hwp_ir(&mut doc);
    assert_eq!(report.border_fills_no_fill_normalized, 1);

    let mut refs = std::collections::HashSet::new();
    for para_shape in &doc.doc_info.para_shapes {
        if para_shape.border_fill_id > 0 {
            refs.insert(para_shape.border_fill_id);
        }
    }
    for char_shape in &doc.doc_info.char_shapes {
        if char_shape.border_fill_id > 0 {
            refs.insert(char_shape.border_fill_id);
        }
    }

    assert!(
        !refs.is_empty(),
        "paragraph/char BorderFill refs must exist"
    );
    for id in refs {
        let border_fill = doc
            .doc_info
            .border_fills
            .get(id.saturating_sub(1) as usize)
            .expect("valid BorderFill ref");
        assert!(
            matches!(border_fill.fill.fill_type, FillType::None),
            "paragraph/char BorderFill #{} must be normalized to no-fill",
            id
        );
    }
}

#[test]
fn task888_expense_report_parses_page_border_fills() {
    let bytes = load_sample("expense_report.hwpx");
    let core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");
    let section_def = &core.document().sections[0].section_def;

    assert_eq!(section_def.page_border_fill.attr, 0x0000_0041);
    assert_eq!(section_def.page_border_fill.border_fill_id, 3);
    assert_eq!(section_def.page_border_fill.spacing_left, 4252);
    assert_eq!(section_def.page_border_fill.spacing_right, 4252);
    assert_eq!(section_def.page_border_fill.spacing_top, 4252);
    assert_eq!(section_def.page_border_fill.spacing_bottom, 4252);

    assert_eq!(section_def.extra_page_border_fills.len(), 2);
    assert_eq!(section_def.extra_page_border_fills[0].attr, 0x0000_0041);
    assert_eq!(section_def.extra_page_border_fills[0].border_fill_id, 3);
    assert_eq!(section_def.extra_page_border_fills[1].attr, 0x0000_0001);
    assert_eq!(section_def.extra_page_border_fills[1].border_fill_id, 3);
}

#[test]
fn task888_expense_report_page_border_fills_survive_hwp_save_reload() {
    let bytes = load_sample("expense_report.hwpx");
    let mut core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");

    let hwp_bytes = core.export_hwp_with_adapter().expect("HWP 직렬화 실패");
    let reloaded = DocumentCore::from_bytes(&hwp_bytes).expect("HWP 재로드 실패");
    let section_def = &reloaded.document().sections[0].section_def;

    assert_eq!(section_def.page_border_fill.attr, 0x0000_0041);
    assert_eq!(section_def.page_border_fill.border_fill_id, 3);
    assert_eq!(section_def.page_border_fill.spacing_left, 4252);
    assert_eq!(section_def.page_border_fill.spacing_right, 4252);
    assert_eq!(section_def.page_border_fill.spacing_top, 4252);
    assert_eq!(section_def.page_border_fill.spacing_bottom, 4252);

    assert_eq!(section_def.extra_page_border_fills.len(), 2);
    assert_eq!(section_def.extra_page_border_fills[0].attr, 0x0000_0041);
    assert_eq!(section_def.extra_page_border_fills[0].border_fill_id, 3);
    assert_eq!(section_def.extra_page_border_fills[1].attr, 0x0000_0001);
    assert_eq!(section_def.extra_page_border_fills[1].border_fill_id, 3);
}

#[test]
fn task899_business_overview_cell_backgrounds_use_no_pattern() {
    let bytes = load_sample("business_overview.hwpx");
    let core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");
    let doc = core.document();

    for border_fill_id in [5_u16, 6, 7] {
        let border_fill = doc
            .doc_info
            .border_fills
            .get((border_fill_id - 1) as usize)
            .unwrap_or_else(|| panic!("BorderFill #{} 없음", border_fill_id));
        assert!(
            matches!(border_fill.fill.fill_type, FillType::Solid),
            "BorderFill #{} must be solid fill",
            border_fill_id
        );
        let solid = border_fill
            .fill
            .solid
            .as_ref()
            .unwrap_or_else(|| panic!("BorderFill #{} solid fill 없음", border_fill_id));
        assert!(
            solid.background_color != 0xffff_ffff,
            "BorderFill #{} must preserve faceColor",
            border_fill_id
        );
        assert_eq!(
            solid.pattern_type, -1,
            "BorderFill #{} has faceColor but no hatchStyle; HWP save must encode no-pattern as -1",
            border_fill_id
        );
    }
}

// ============================================================
// Stage 4 — lineseg lh/vpos 사전계산 + SectionDef 컨트롤 삽입 검증
// ============================================================

#[test]
fn stage4_section_def_control_inserted() {
    let bytes = load_sample("hwpx-h-01.hwpx");
    let core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");

    // 어댑터 적용 전: 첫 문단에 SectionDef 컨트롤이 없어야 함 (HWPX 출처 특성)
    let first_para_orig = &core.document().sections[0].paragraphs[0];
    assert!(
        !first_para_orig
            .controls
            .iter()
            .any(|c| matches!(c, Control::SectionDef(_))),
        "HWPX 출처 첫 문단에 SectionDef 가 이미 있다면 가정 위반"
    );

    let mut doc = core.document().clone();
    let report = convert_hwpx_to_hwp_ir(&mut doc);
    assert!(
        report.section_def_controls_inserted > 0,
        "SectionDef 삽입이 발생해야 함"
    );

    // 어댑터 적용 후: 모든 섹션의 첫 문단에 SectionDef 가 있어야 함
    for (s_idx, section) in doc.sections.iter().enumerate() {
        let first_para = &section.paragraphs[0];
        assert!(
            first_para
                .controls
                .iter()
                .any(|c| matches!(c, Control::SectionDef(_))),
            "섹션 {} 의 첫 문단에 SectionDef 컨트롤 없음",
            s_idx
        );
    }
}

#[test]
fn stage4_section_def_idempotent() {
    let bytes = load_sample("hwpx-h-01.hwpx");
    let core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");
    let mut doc = core.document().clone();

    let r1 = convert_hwpx_to_hwp_ir(&mut doc);
    let r2 = convert_hwpx_to_hwp_ir(&mut doc);
    assert!(r1.section_def_controls_inserted > 0);
    assert_eq!(
        r2.section_def_controls_inserted, 0,
        "2차 호출 시 삽입 0 (idempotent)"
    );
}

#[test]
fn stage4_page_def_preserved_after_roundtrip() {
    // 어댑터 적용 후 직렬화 → 재로드 시 PageDef (width, height, margins) 가 보존돼야 함.
    let bytes = load_sample("hwpx-h-01.hwpx");
    let core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");
    let orig_pd = core.document().sections[0].section_def.page_def.clone();

    let mut doc = core.document().clone();
    convert_hwpx_to_hwp_ir(&mut doc);
    let hwp_bytes = rhwp::serializer::serialize_hwp(&doc).expect("직렬화 실패");
    let reloaded = DocumentCore::from_bytes(&hwp_bytes).expect("재로드 실패");
    let reload_pd = &reloaded.document().sections[0].section_def.page_def;

    assert_eq!(orig_pd.width, reload_pd.width, "width 보존");
    assert_eq!(orig_pd.height, reload_pd.height, "height 보존");
    assert_eq!(
        orig_pd.margin_left, reload_pd.margin_left,
        "margin_left 보존"
    );
    assert_eq!(
        orig_pd.margin_right, reload_pd.margin_right,
        "margin_right 보존"
    );
    assert_eq!(orig_pd.margin_top, reload_pd.margin_top, "margin_top 보존");
    assert_eq!(
        orig_pd.margin_bottom, reload_pd.margin_bottom,
        "margin_bottom 보존"
    );
}

/// Stage 4 핵심 게이트: 어댑터 적용 → 직렬화 → 재로드 시 페이지 수가 원본과 일치.
fn assert_page_count_recovered(name: &str, bytes: &[u8]) {
    let (orig, after) = page_count_with_adapter(bytes);
    let expected = expected_hancom_hwp_page_count(name, orig);
    eprintln!(
        "[#178 Stage 4] {}: orig={}, expected_hwp={}, after_adapter={}",
        name, orig, expected, after
    );
    assert_eq!(
        after, expected,
        "{}: 어댑터 적용 후 페이지 수 {} != 한컴 HWP 저장 기준 {} (orig={})",
        name, after, expected, orig
    );
}

#[test]
fn stage4_page_count_recovered_hwpx_h_01() {
    assert_page_count_recovered("hwpx-h-01", &load_sample("hwpx-h-01.hwpx"));
}

#[test]
fn stage4_page_count_recovered_hwpx_h_02() {
    assert_page_count_recovered("hwpx-h-02", &load_sample("hwpx-h-02.hwpx"));
}

#[test]
fn stage4_page_count_recovered_hwpx_h_03() {
    assert_page_count_recovered("hwpx-h-03", &load_sample("hwpx-h-03.hwpx"));
}

// ============================================================
// Stage 5 — 통합 진입점 export_hwp_with_adapter() 검증
// ============================================================

#[test]
fn stage5_export_hwp_with_adapter_hwpx_source_recovers_pages() {
    let bytes = load_sample("hwpx-h-01.hwpx");
    let mut core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드");
    let orig = core.page_count();

    let hwp_bytes = core.export_hwp_with_adapter().expect("HWP 직렬화");
    let reloaded = DocumentCore::from_bytes(&hwp_bytes).expect("HWP 재로드");

    assert_eq!(
        reloaded.page_count(),
        orig,
        "어댑터 통합 진입점: 페이지 수 보존 (orig={}, reloaded={})",
        orig,
        reloaded.page_count()
    );
}

#[test]
fn stage5_export_hwp_with_adapter_hwp_source_unchanged() {
    // HWP 원본 — 어댑터는 no-op (source_format != Hwpx)
    let path = "samples/hwp_table_test.hwp";
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => {
            eprintln!("[skip] {} 없음", path);
            return;
        }
    };
    let mut core = DocumentCore::from_bytes(&bytes).expect("HWP 로드");

    let bytes_native = core.export_hwp_native().expect("native 직렬화");
    let bytes_adapter = core.export_hwp_with_adapter().expect("adapter 직렬화");

    assert_eq!(
        bytes_native, bytes_adapter,
        "HWP 출처는 어댑터 호출이 native 와 동일 결과여야 함"
    );
}

#[test]
fn stage5_export_hwp_with_adapter_idempotent_on_repeated_calls() {
    // 같은 DocumentCore 에 export_hwp_with_adapter() 를 두 번 호출해도 저장 결과가 같다.
    // Stage #854부터 어댑터는 저장용 clone에만 적용되어 live IR을 변경하지 않는다.
    let bytes = load_sample("hwpx-h-01.hwpx");
    let mut core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드");

    let first = core.export_hwp_with_adapter().expect("1차");
    let second = core.export_hwp_with_adapter().expect("2차");

    assert_eq!(
        first, second,
        "동일 DocumentCore 에 어댑터 통합 진입점 2회 호출 시 같은 bytes"
    );
}

#[test]
fn stage5_all_three_samples_recover_via_unified_entry_point() {
    for name in ["hwpx-h-01.hwpx", "hwpx-h-02.hwpx", "hwpx-h-03.hwpx"] {
        let bytes = load_sample(name);
        let mut core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드");
        let orig = core.page_count();
        let expected = expected_hancom_hwp_page_count(name, orig);

        let hwp_bytes = core.export_hwp_with_adapter().expect("HWP 직렬화");
        let reloaded = DocumentCore::from_bytes(&hwp_bytes).expect("HWP 재로드");

        assert_eq!(
            reloaded.page_count(),
            expected,
            "{}: 한컴 HWP 저장 기준 페이지 수 일치 (orig={}, expected={}, reloaded={})",
            name,
            orig,
            expected,
            reloaded.page_count()
        );
    }
}

// ============================================================
// Stage 6 — serialize_hwp_with_verify 명시 검증 함수
// ============================================================

#[test]
fn stage6_verify_recovered_for_hwpx_h_01() {
    let bytes = load_sample("hwpx-h-01.hwpx");
    let mut core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드");
    let v = core.serialize_hwp_with_verify().expect("verify");
    eprintln!(
        "[#178 Stage 6] verify hwpx-h-01: before={}, after={}, recovered={}, bytes={}",
        v.page_count_before, v.page_count_after, v.recovered, v.bytes_len
    );
    assert!(
        v.recovered,
        "페이지 회복 실패: before={} after={}",
        v.page_count_before, v.page_count_after
    );
    assert_eq!(v.page_count_before, v.page_count_after);
    assert!(v.bytes_len > 0);
}

#[test]
fn stage6_verify_recovered_for_all_three_samples() {
    for name in ["hwpx-h-01.hwpx", "hwpx-h-02.hwpx", "hwpx-h-03.hwpx"] {
        let bytes = load_sample(name);
        let mut core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드");
        let v = core.serialize_hwp_with_verify().expect("verify");
        let expected = expected_hancom_hwp_page_count(name, v.page_count_before);
        assert_eq!(
            v.page_count_after, expected,
            "{}: 한컴 HWP 저장 기준 페이지 수 일치 (before={}, expected={}, after={})",
            name, v.page_count_before, expected, v.page_count_after
        );
        assert_eq!(v.recovered, v.page_count_before == v.page_count_after);
    }
}

#[test]
fn stage6_verify_for_hwp_source_also_recovered() {
    // HWP 출처 — 어댑터는 no-op, 그래도 verify 는 동작해야 함 (recovered=true)
    let path = "samples/hwp_table_test.hwp";
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => {
            eprintln!("[skip] {} 없음", path);
            return;
        }
    };
    let mut core = DocumentCore::from_bytes(&bytes).expect("HWP 로드");
    let v = core.serialize_hwp_with_verify().expect("verify");
    assert!(v.recovered, "HWP 출처 자기 재로드 페이지 수 일치");
}

#[test]
fn stage5_wasm_api_export_hwp_uses_adapter() {
    // wasm_api 의 export_hwp (네이티브 래퍼: export_hwp_native_wrapper 가 아니라
    // HwpDocument 자체가 DerefMut<DocumentCore>) 가 어댑터를 자동 적용하는지 확인.
    // 본 테스트는 네이티브 환경에서 wasm_api 진입점 동작을 검증.
    let bytes = load_sample("hwpx-h-01.hwpx");
    let mut doc = rhwp::wasm_api::HwpDocument::from_bytes(&bytes).expect("HWPX 로드");
    let orig = doc.page_count();

    // export_hwp 는 wasm_bindgen 메서드라 직접 호출 불가 → 동등한 export_hwp_with_adapter 호출
    let hwp_bytes = doc.export_hwp_with_adapter().expect("어댑터 직렬화");
    let reloaded = DocumentCore::from_bytes(&hwp_bytes).expect("HWP 재로드");

    assert_eq!(
        reloaded.page_count(),
        orig as u32,
        "wasm_api 경로: 페이지 수 보존 (orig={}, reloaded={})",
        orig,
        reloaded.page_count()
    );
}

// ============================================================
// Task #903 — hwpx-h-01 embedded BinData 저장 보존
// ============================================================

fn assert_hwp_embedded_bindata_loaded(label: &str, core: &DocumentCore) {
    let doc = core.document();
    assert_eq!(
        doc.doc_info.bin_data_list.len(),
        5,
        "{}: BinData record count",
        label
    );
    assert_eq!(
        doc.bin_data_content.len(),
        5,
        "{}: loaded BinDataContent count",
        label
    );

    for expected_id in 1..=5 {
        let bd = doc
            .doc_info
            .bin_data_list
            .iter()
            .find(|bd| bd.storage_id == expected_id)
            .unwrap_or_else(|| panic!("{}: BinData storage_id={} 없음", label, expected_id));
        assert_eq!(
            bd.data_type,
            BinDataType::Embedding,
            "{}: storage_id={} data_type attr=0x{:04x}",
            label,
            expected_id,
            bd.attr
        );
        assert_eq!(
            bd.attr & 0x000f,
            1,
            "{}: storage_id={} attr low nibble must be Embedding",
            label,
            expected_id
        );

        let content = doc
            .bin_data_content
            .iter()
            .find(|content| content.id == expected_id)
            .unwrap_or_else(|| panic!("{}: BinDataContent id={} 없음", label, expected_id));
        assert!(
            !content.data.is_empty(),
            "{}: BinDataContent id={} bytes must be non-empty",
            label,
            expected_id
        );
    }
}

fn task903_hwpx_h_01_group_child_shape_attrs(core: &DocumentCore) -> Vec<&ShapeComponentAttr> {
    let para = &core.document().sections[0].paragraphs[29];
    let group = match &para.controls[0] {
        Control::Shape(shape) => match shape.as_ref() {
            ShapeObject::Group(group) => group,
            other => panic!("문단 0:29는 묶음이어야 함: {:?}", other),
        },
        other => panic!("문단 0:29 첫 컨트롤은 shape이어야 함: {:?}", other),
    };

    group
        .children
        .iter()
        .enumerate()
        .map(|(idx, child)| match child {
            ShapeObject::Picture(pic) => &pic.shape_attr,
            other => panic!("child[{}]는 그림이어야 함: {:?}", idx, other),
        })
        .collect()
}

fn task903_hwpx_h_01_logo_group(core: &DocumentCore) -> &GroupShape {
    let para = &core.document().sections[0].paragraphs[29];
    match &para.controls[0] {
        Control::Shape(shape) => match shape.as_ref() {
            ShapeObject::Group(group) => group,
            other => panic!("문단 0:29는 묶음이어야 함: {:?}", other),
        },
        other => panic!("문단 0:29 첫 컨트롤은 shape이어야 함: {:?}", other),
    }
}

fn task903_hwpx_h_01_logo_group_mut(core: &mut DocumentCore) -> &mut GroupShape {
    let para = &mut core.document_mut().sections[0].paragraphs[29];
    match para.controls.get_mut(0) {
        Some(Control::Shape(shape)) => match shape.as_mut() {
            ShapeObject::Group(group) => group,
            other => panic!("문단 0:29는 묶음이어야 함: {:?}", other),
        },
        other => panic!("문단 0:29 첫 컨트롤은 shape이어야 함: {:?}", other),
    }
}

fn task903_hwpx_h_01_first_table_picture_common_attrs(core: &DocumentCore) -> Vec<&CommonObjAttr> {
    let table = task903_hwpx_h_01_first_table(core);
    table
        .cells
        .iter()
        .flat_map(|cell| cell.paragraphs.iter())
        .flat_map(|para| para.controls.iter())
        .filter_map(|ctrl| match ctrl {
            Control::Picture(pic) => Some(&pic.common),
            _ => None,
        })
        .collect()
}

fn task903_hwpx_h_01_first_table_picture_common_mut(
    core: &mut DocumentCore,
    picture_idx: usize,
) -> Option<&mut CommonObjAttr> {
    let mut seen = 0usize;
    for section in &mut core.document_mut().sections {
        for para in &mut section.paragraphs {
            for ctrl in &mut para.controls {
                let Control::Table(table) = ctrl else {
                    continue;
                };
                for cell in &mut table.cells {
                    for cell_para in &mut cell.paragraphs {
                        for cell_ctrl in &mut cell_para.controls {
                            if let Control::Picture(pic) = cell_ctrl {
                                if seen == picture_idx {
                                    return Some(&mut pic.common);
                                }
                                seen += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

fn task903_hwpx_h_01_first_table(core: &DocumentCore) -> &Table {
    task903_hwpx_h_01_table_at(core, 0, 0)
}

fn task903_hwpx_h_01_first_table_mut(core: &mut DocumentCore) -> &mut Table {
    for section in &mut core.document_mut().sections {
        for para in &mut section.paragraphs {
            for ctrl in &mut para.controls {
                if let Control::Table(table) = ctrl {
                    return table.as_mut();
                }
            }
        }
    }
    panic!("hwpx-h-01 첫 표를 찾지 못함")
}

fn task903_hwpx_h_01_table_at(core: &DocumentCore, para_idx: usize, table_idx: usize) -> &Table {
    task903_hwpx_h_01_table_at_in_section(core, 0, para_idx, table_idx)
}

fn task903_hwpx_h_01_table_at_in_section(
    core: &DocumentCore,
    section_idx: usize,
    para_idx: usize,
    table_idx: usize,
) -> &Table {
    let para = &core.document().sections[section_idx].paragraphs[para_idx];
    para.controls
        .iter()
        .filter_map(|ctrl| match ctrl {
            Control::Table(table) => Some(table.as_ref()),
            _ => None,
        })
        .nth(table_idx)
        .unwrap_or_else(|| {
            panic!(
                "문단 {}:{}에는 {}번째 표가 있어야 함",
                section_idx, para_idx, table_idx
            )
        })
}

fn task903_hwpx_h_01_table_at_mut(
    core: &mut DocumentCore,
    para_idx: usize,
    table_idx: usize,
) -> &mut Table {
    task903_hwpx_h_01_table_at_in_section_mut(core, 0, para_idx, table_idx)
}

fn task903_hwpx_h_01_table_at_in_section_mut(
    core: &mut DocumentCore,
    section_idx: usize,
    para_idx: usize,
    table_idx: usize,
) -> &mut Table {
    let para = &mut core.document_mut().sections[section_idx].paragraphs[para_idx];
    para.controls
        .iter_mut()
        .filter_map(|ctrl| match ctrl {
            Control::Table(table) => Some(table.as_mut()),
            _ => None,
        })
        .nth(table_idx)
        .unwrap_or_else(|| {
            panic!(
                "문단 {}:{}에는 {}번째 표가 있어야 함",
                section_idx, para_idx, table_idx
            )
        })
}

fn task903_visit_tables_mut(paragraphs: &mut [Paragraph], f: &mut impl FnMut(&mut Table)) {
    for para in paragraphs {
        for ctrl in &mut para.controls {
            match ctrl {
                Control::Table(table) => {
                    f(table);
                    if let Some(caption) = &mut table.caption {
                        task903_visit_tables_mut(&mut caption.paragraphs, f);
                    }
                    for cell in &mut table.cells {
                        task903_visit_tables_mut(&mut cell.paragraphs, f);
                    }
                }
                Control::Header(header) => {
                    task903_visit_tables_mut(&mut header.paragraphs, f);
                }
                Control::Footer(footer) => {
                    task903_visit_tables_mut(&mut footer.paragraphs, f);
                }
                Control::Footnote(footnote) => {
                    task903_visit_tables_mut(&mut footnote.paragraphs, f);
                }
                Control::Endnote(endnote) => {
                    task903_visit_tables_mut(&mut endnote.paragraphs, f);
                }
                Control::HiddenComment(comment) => {
                    task903_visit_tables_mut(&mut comment.paragraphs, f);
                }
                _ => {}
            }
        }
    }
}

fn task903_collect_tables_from_paragraphs(paragraphs: &[Paragraph], out: &mut Vec<Table>) {
    for para in paragraphs {
        for ctrl in &para.controls {
            match ctrl {
                Control::Table(table) => {
                    out.push((**table).clone());
                    if let Some(caption) = &table.caption {
                        task903_collect_tables_from_paragraphs(&caption.paragraphs, out);
                    }
                    for cell in &table.cells {
                        task903_collect_tables_from_paragraphs(&cell.paragraphs, out);
                    }
                }
                Control::Header(header) => {
                    task903_collect_tables_from_paragraphs(&header.paragraphs, out);
                }
                Control::Footer(footer) => {
                    task903_collect_tables_from_paragraphs(&footer.paragraphs, out);
                }
                Control::Footnote(footnote) => {
                    task903_collect_tables_from_paragraphs(&footnote.paragraphs, out);
                }
                Control::Endnote(endnote) => {
                    task903_collect_tables_from_paragraphs(&endnote.paragraphs, out);
                }
                Control::HiddenComment(comment) => {
                    task903_collect_tables_from_paragraphs(&comment.paragraphs, out);
                }
                _ => {}
            }
        }
    }
}

fn task903_collect_all_tables(core: &DocumentCore) -> Vec<Table> {
    let mut tables = Vec::new();
    for section in &core.document().sections {
        task903_collect_tables_from_paragraphs(&section.paragraphs, &mut tables);
    }
    tables
}

fn task903_visit_shape_paragraphs_mut(
    shape: &mut ShapeObject,
    f: &mut impl FnMut(&mut [Paragraph]),
) {
    if let Some(drawing) = shape.drawing_mut() {
        if let Some(text_box) = &mut drawing.text_box {
            f(&mut text_box.paragraphs);
        }
        if let Some(caption) = &mut drawing.caption {
            f(&mut caption.paragraphs);
        }
    }

    match shape {
        ShapeObject::Group(group) => {
            if let Some(caption) = &mut group.caption {
                f(&mut caption.paragraphs);
            }
            for child in &mut group.children {
                task903_visit_shape_paragraphs_mut(child, f);
            }
        }
        ShapeObject::Picture(picture) => {
            if let Some(caption) = &mut picture.caption {
                f(&mut caption.paragraphs);
            }
        }
        _ => {}
    }
}

fn task903_materialize_para_header_tail_2(paragraphs: &mut [Paragraph]) -> usize {
    let mut changed = 0usize;
    for para in paragraphs {
        if para.raw_header_extra.len() < 12 {
            // raw_header_extra[0..6]는 serializer가 재계산하는
            // numCharShapes/numRangeTags/numLineSegs 자리라 값 자체는 사용하지 않는다.
            // raw_header_extra[6..]가 instanceId(4) + 예약 tail로 기록된다.
            let mut extra = vec![0u8; 12];
            if para.raw_header_extra.len() > 6 {
                let copy_len = (para.raw_header_extra.len() - 6).min(6);
                extra[6..6 + copy_len].copy_from_slice(&para.raw_header_extra[6..6 + copy_len]);
            }
            para.raw_header_extra = extra;
            changed += 1;
        }

        for ctrl in &mut para.controls {
            match ctrl {
                Control::Table(table) => {
                    if let Some(caption) = &mut table.caption {
                        changed += task903_materialize_para_header_tail_2(&mut caption.paragraphs);
                    }
                    for cell in &mut table.cells {
                        changed += task903_materialize_para_header_tail_2(&mut cell.paragraphs);
                    }
                }
                Control::Shape(shape) => {
                    task903_visit_shape_paragraphs_mut(shape, &mut |paragraphs| {
                        changed += task903_materialize_para_header_tail_2(paragraphs);
                    });
                }
                Control::Picture(picture) => {
                    if let Some(caption) = &mut picture.caption {
                        changed += task903_materialize_para_header_tail_2(&mut caption.paragraphs);
                    }
                }
                Control::Header(header) => {
                    changed += task903_materialize_para_header_tail_2(&mut header.paragraphs);
                }
                Control::Footer(footer) => {
                    changed += task903_materialize_para_header_tail_2(&mut footer.paragraphs);
                }
                Control::Footnote(footnote) => {
                    changed += task903_materialize_para_header_tail_2(&mut footnote.paragraphs);
                }
                Control::Endnote(endnote) => {
                    changed += task903_materialize_para_header_tail_2(&mut endnote.paragraphs);
                }
                Control::HiddenComment(comment) => {
                    changed += task903_materialize_para_header_tail_2(&mut comment.paragraphs);
                }
                _ => {}
            }
        }
    }
    changed
}

fn task903_materialize_cell_list_header_tail_13(table: &mut Table) -> usize {
    let mut changed = 0;
    for cell in &mut table.cells {
        if cell.raw_list_extra.is_empty() {
            let mut extra = vec![0u8; 13];
            extra[0..4].copy_from_slice(&cell.width.to_le_bytes());
            cell.raw_list_extra = extra;
            changed += 1;
        }
    }
    changed
}

fn task903_materialize_table_record_tail_2(table: &mut Table) -> usize {
    if table.raw_table_record_extra.is_empty() {
        table.raw_table_record_extra = vec![0u8; 2];
        1
    } else {
        0
    }
}

fn task903_materialize_stage8_section_def_core(
    section: &mut rhwp::model::document::Section,
) -> usize {
    fn apply(sd: &mut rhwp::model::document::SectionDef) -> bool {
        let before = (
            sd.column_spacing,
            sd.outline_numbering_id,
            sd.raw_ctrl_extra.len(),
        );
        sd.column_spacing = 0x046e;
        sd.outline_numbering_id = 1;
        if sd.raw_ctrl_extra.len() < 19 {
            sd.raw_ctrl_extra.resize(19, 0);
        }
        before
            != (
                sd.column_spacing,
                sd.outline_numbering_id,
                sd.raw_ctrl_extra.len(),
            )
    }

    let mut changed = 0usize;
    if apply(&mut section.section_def) {
        changed += 1;
    }
    for para in &mut section.paragraphs {
        for ctrl in &mut para.controls {
            if let Control::SectionDef(sd) = ctrl {
                if apply(sd) {
                    changed += 1;
                }
            }
        }
    }
    changed
}

fn task903_materialize_first_cell_list_header_tail_65(core: &mut DocumentCore) -> usize {
    const FIRST_CELL_TAIL_AFTER_WIDTH: [u8; 27] = [
        0xff, 0x1b, 0x02, 0x01, 0x00, 0x00, 0x00, 0x00, 0x40, 0x01, 0x00, 0x03, 0x00, 0x30, 0xae,
        0x00, 0xad, 0x85, 0xba, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    for section in &mut core.document_mut().sections {
        for para in &mut section.paragraphs {
            for ctrl in &mut para.controls {
                if let Control::Table(table) = ctrl {
                    let Some(first_cell) = table.cells.first_mut() else {
                        return 0;
                    };
                    let mut extra = vec![0u8; 31];
                    extra[0..4].copy_from_slice(&first_cell.width.to_le_bytes());
                    extra[4..].copy_from_slice(&FIRST_CELL_TAIL_AFTER_WIDTH);
                    if first_cell.raw_list_extra != extra {
                        first_cell.raw_list_extra = extra;
                        return 1;
                    }
                    return 0;
                }
            }
        }
    }
    0
}

fn task903_reference_first_table_picture_common(picture_idx: usize) -> CommonObjAttr {
    let reference_bytes =
        std::fs::read("samples/hwpx/hancom-hwp/hwpx-h-01.hwp").expect("한컴 정답 HWP 없음");
    let reference = DocumentCore::from_bytes(&reference_bytes).expect("한컴 정답 HWP 파싱 실패");
    let attrs = task903_hwpx_h_01_first_table_picture_common_attrs(&reference);
    let common = attrs
        .get(picture_idx)
        .unwrap_or_else(|| panic!("정답 HWP 첫 표 그림 common[{}] 없음", picture_idx));
    (**common).clone()
}

fn task903_materialize_first_picture_common_from_reference(core: &mut DocumentCore) -> usize {
    let reference_common = task903_reference_first_table_picture_common(0);
    let Some(target_common) = task903_hwpx_h_01_first_table_picture_common_mut(core, 0) else {
        return 0;
    };
    *target_common = reference_common;
    1
}

fn task903_reference_hwpx_h_01_core() -> DocumentCore {
    let reference_bytes =
        std::fs::read("samples/hwpx/hancom-hwp/hwpx-h-01.hwp").expect("한컴 정답 HWP 없음");
    DocumentCore::from_bytes(&reference_bytes).expect("한컴 정답 HWP 파싱 실패")
}

fn task903_first_table_picture_clones(table: &Table) -> Vec<rhwp::model::image::Picture> {
    table
        .cells
        .iter()
        .flat_map(|cell| cell.paragraphs.iter())
        .flat_map(|para| para.controls.iter())
        .filter_map(|ctrl| match ctrl {
            Control::Picture(pic) => Some((**pic).clone()),
            _ => None,
        })
        .collect()
}

fn task903_materialize_first_table_ctrl_header_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_first_table_mut(core);
    table.raw_ctrl_data = reference_table.raw_ctrl_data.clone();
    1
}

fn task903_materialize_first_table_cell_list_headers_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_first_table_mut(core);
    assert_eq!(
        table.cells.len(),
        reference_table.cells.len(),
        "첫 표 셀 수는 정답 HWP와 같아야 함"
    );

    for (target, reference) in table.cells.iter_mut().zip(reference_table.cells.iter()) {
        target.list_header_width_ref = reference.list_header_width_ref;
        target.text_direction = reference.text_direction;
        target.vertical_align = reference.vertical_align;
        target.col = reference.col;
        target.row = reference.row;
        target.col_span = reference.col_span;
        target.row_span = reference.row_span;
        target.width = reference.width;
        target.height = reference.height;
        target.padding = reference.padding;
        target.border_fill_id = reference.border_fill_id;
        target.apply_inner_margin = reference.apply_inner_margin;
        target.is_header = reference.is_header;
        target.raw_list_extra = reference.raw_list_extra.clone();
        target.field_name = reference.field_name.clone();
    }

    table.cells.len()
}

fn task903_materialize_first_table_record_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_first_table_mut(core);
    table.attr = reference_table.attr;
    table.row_count = reference_table.row_count;
    table.col_count = reference_table.col_count;
    table.cell_spacing = reference_table.cell_spacing;
    table.padding = reference_table.padding;
    table.row_sizes = reference_table.row_sizes.clone();
    table.border_fill_id = reference_table.border_fill_id;
    table.zones = reference_table.zones.clone();
    table.page_break = reference_table.page_break;
    table.repeat_header = reference_table.repeat_header;
    table.raw_table_record_attr = reference_table.raw_table_record_attr;
    table.raw_table_record_extra = reference_table.raw_table_record_extra.clone();
    1
}

fn task903_materialize_first_table_cell_para_headers_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_first_table_mut(core);
    assert_eq!(
        table.cells.len(),
        reference_table.cells.len(),
        "첫 표 셀 수는 정답 HWP와 같아야 함"
    );

    let mut changed = 0usize;
    for (target_cell, reference_cell) in table.cells.iter_mut().zip(reference_table.cells.iter()) {
        let n = target_cell
            .paragraphs
            .len()
            .min(reference_cell.paragraphs.len());
        for idx in 0..n {
            target_cell.paragraphs[idx].raw_header_extra =
                reference_cell.paragraphs[idx].raw_header_extra.clone();
            changed += 1;
        }
    }
    changed
}

fn task903_materialize_first_table_picture_records_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let reference_pictures = task903_first_table_picture_clones(reference_table);
    assert!(
        !reference_pictures.is_empty(),
        "정답 HWP 첫 표에는 그림이 있어야 함"
    );

    let table = task903_hwpx_h_01_first_table_mut(core);
    let mut seen = 0usize;
    for cell in &mut table.cells {
        for para in &mut cell.paragraphs {
            for ctrl in &mut para.controls {
                if let Control::Picture(pic) = ctrl {
                    let reference = reference_pictures
                        .get(seen)
                        .unwrap_or_else(|| panic!("정답 HWP 첫 표 그림[{}] 없음", seen));
                    pic.common = reference.common.clone();
                    pic.shape_attr = reference.shape_attr.clone();
                    pic.border_color = reference.border_color;
                    pic.border_width = reference.border_width;
                    pic.border_attr = reference.border_attr;
                    pic.border_x = reference.border_x;
                    pic.border_y = reference.border_y;
                    pic.crop = reference.crop;
                    pic.padding = reference.padding;
                    pic.image_attr = reference.image_attr.clone();
                    pic.border_opacity = reference.border_opacity;
                    pic.instance_id = reference.instance_id;
                    pic.raw_picture_extra = reference.raw_picture_extra.clone();
                    seen += 1;
                }
            }
        }
    }

    assert_eq!(
        seen,
        reference_pictures.len(),
        "첫 표 그림 수는 정답 HWP와 같아야 함"
    );
    seen
}

fn task903_generate_stage7_probe_variant(
    output_name: &str,
    cell_tail_13: bool,
    table_tail_2: bool,
    para_tail_2: bool,
    section_ctrl_tail_19: bool,
) -> (usize, usize, usize, usize, usize) {
    let bytes = load_sample("hwpx-h-01.hwpx");
    let mut core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");

    // Adapter를 먼저 적용한 뒤 tail만 바꾼다. 이렇게 해야 Stage 6의 배치 회복 상태와
    // Stage 7의 레코드 tail 보강 효과를 분리해서 볼 수 있다.
    convert_if_hwpx_source(core.document_mut(), rhwp::parser::FileFormat::Hwpx);

    let mut changed_cells = 0usize;
    let mut changed_tables = 0usize;
    let mut changed_paragraphs = 0usize;
    let mut changed_sections = 0usize;
    for section in &mut core.document_mut().sections {
        if section_ctrl_tail_19 && section.section_def.raw_ctrl_extra.len() < 19 {
            section.section_def.raw_ctrl_extra.resize(19, 0);
            changed_sections += 1;
        }
        if section_ctrl_tail_19 {
            for para in &mut section.paragraphs {
                for ctrl in &mut para.controls {
                    if let Control::SectionDef(sd) = ctrl {
                        if sd.raw_ctrl_extra.len() < 19 {
                            sd.raw_ctrl_extra.resize(19, 0);
                            changed_sections += 1;
                        }
                    }
                }
            }
        }
        task903_visit_tables_mut(&mut section.paragraphs, &mut |table| {
            if cell_tail_13 {
                changed_cells += task903_materialize_cell_list_header_tail_13(table);
            }
            if table_tail_2 {
                changed_tables += task903_materialize_table_record_tail_2(table);
            }
        });
        if para_tail_2 {
            changed_paragraphs += task903_materialize_para_header_tail_2(&mut section.paragraphs);
        }
    }

    let hwp_bytes = core.export_hwp_native().expect("Stage 7 HWP 직렬화 실패");
    let out_dir = std::path::Path::new("output/poc/hwpx2hwp/task903/stage7_record_tail_probe");
    std::fs::create_dir_all(out_dir).expect("Stage 7 output dir 생성 실패");
    let out_path = out_dir.join(output_name);
    std::fs::write(&out_path, &hwp_bytes).expect("Stage 7 probe 파일 저장 실패");

    let reloaded = DocumentCore::from_bytes(&hwp_bytes).expect("Stage 7 probe HWP 재로드 실패");
    assert_eq!(reloaded.page_count(), 9, "{} page count", output_name);
    let flow_table = task903_hwpx_h_01_table_at(&reloaded, 10, 0);
    assert_eq!(flow_table.common.text_wrap, TextWrap::TopAndBottom);
    assert_eq!(flow_table.common.vert_rel_to, VertRelTo::Para);
    assert_eq!(flow_table.common.horz_rel_to, HorzRelTo::Para);
    assert_ne!(
        (
            flow_table.outer_margin_left,
            flow_table.outer_margin_right,
            flow_table.outer_margin_top,
            flow_table.outer_margin_bottom,
        ),
        (0, 0, 0, 0),
        "{} flow table outer_margin",
        output_name
    );

    if cell_tail_13 {
        let cells_with_tail = flow_table
            .cells
            .iter()
            .filter(|cell| cell.raw_list_extra.len() == 13)
            .count();
        assert_eq!(
            cells_with_tail,
            flow_table.cells.len(),
            "{} flow table cells must reload with 13B raw_list_extra",
            output_name
        );
    }
    if table_tail_2 {
        // TABLE tail 2B는 파서에서 nZones=0으로 소비될 수 있다. 이 probe의 검증은
        // 직렬화 바이트와 dump-records의 TABLE 크기로 확인한다.
        assert!(
            flow_table.row_count > 0 && flow_table.col_count > 0,
            "{} flow table must still reload as a table",
            output_name
        );
    }
    if para_tail_2 {
        assert!(
            reloaded.document().sections[0]
                .paragraphs
                .iter()
                .all(|para| para.raw_header_extra.len() >= 12),
            "{} top-level paragraphs must reload with at least 12B raw_header_extra",
            output_name
        );
    }
    if section_ctrl_tail_19 {
        assert!(
            changed_sections > 0,
            "{} SectionDef CTRL_HEADER tail must be materialized before serialization",
            output_name
        );
    }

    eprintln!(
        "[#903 Stage 7] {}: bytes={}, changed_cells={}, changed_tables={}, changed_paragraphs={}, changed_sections={}, pages={}",
        out_path.display(),
        hwp_bytes.len(),
        changed_cells,
        changed_tables,
        changed_paragraphs,
        changed_sections,
        reloaded.page_count()
    );

    (
        hwp_bytes.len(),
        changed_cells,
        changed_tables,
        changed_paragraphs,
        changed_sections,
    )
}

fn task903_generate_stage8_probe_variant(
    output_name: &str,
    section_core: bool,
    para_tail_2: bool,
    first_cell_65: bool,
    first_picture_common: bool,
) -> (usize, usize, usize, usize, usize) {
    let bytes = load_sample("hwpx-h-01.hwpx");
    let mut core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");

    convert_if_hwpx_source(core.document_mut(), rhwp::parser::FileFormat::Hwpx);

    let mut changed_sections = 0usize;
    let mut changed_paragraphs = 0usize;
    let mut changed_first_cells = 0usize;
    let mut changed_first_pictures = 0usize;

    if section_core {
        for section in &mut core.document_mut().sections {
            changed_sections += task903_materialize_stage8_section_def_core(section);
        }
    }
    if para_tail_2 {
        for section in &mut core.document_mut().sections {
            changed_paragraphs += task903_materialize_para_header_tail_2(&mut section.paragraphs);
        }
    }
    if first_cell_65 {
        changed_first_cells += task903_materialize_first_cell_list_header_tail_65(&mut core);
    }
    if first_picture_common {
        changed_first_pictures +=
            task903_materialize_first_picture_common_from_reference(&mut core);
    }

    let hwp_bytes = core.export_hwp_native().expect("Stage 8 HWP 직렬화 실패");
    let out_dir = std::path::Path::new("output/poc/hwpx2hwp/task903/stage8_core_field_probe");
    std::fs::create_dir_all(out_dir).expect("Stage 8 output dir 생성 실패");
    let out_path = out_dir.join(output_name);
    std::fs::write(&out_path, &hwp_bytes).expect("Stage 8 probe 파일 저장 실패");

    let reloaded = DocumentCore::from_bytes(&hwp_bytes).expect("Stage 8 probe HWP 재로드 실패");
    assert_eq!(reloaded.page_count(), 9, "{} page count", output_name);
    let flow_table = task903_hwpx_h_01_table_at(&reloaded, 10, 0);
    assert_eq!(flow_table.common.text_wrap, TextWrap::TopAndBottom);
    assert_eq!(flow_table.common.vert_rel_to, VertRelTo::Para);
    assert_eq!(flow_table.common.horz_rel_to, HorzRelTo::Para);
    assert_ne!(
        (
            flow_table.outer_margin_left,
            flow_table.outer_margin_right,
            flow_table.outer_margin_top,
            flow_table.outer_margin_bottom,
        ),
        (0, 0, 0, 0),
        "{} flow table outer_margin",
        output_name
    );

    if first_cell_65 {
        let first_table = task903_hwpx_h_01_first_table(&reloaded);
        let first_cell = first_table
            .cells
            .first()
            .expect("첫 표 첫 셀은 존재해야 함");
        assert_eq!(
            first_cell.raw_list_extra.len(),
            31,
            "{} first cell must reload with 31B raw_list_extra",
            output_name
        );
    }
    if first_picture_common {
        let attrs = task903_hwpx_h_01_first_table_picture_common_attrs(&reloaded);
        let first = attrs
            .first()
            .expect("첫 표 첫 그림 CommonObjAttr는 존재해야 함");
        assert!(
            !first.description.is_empty(),
            "{} first picture description must reload from reference",
            output_name
        );
    }

    eprintln!(
        "[#903 Stage 8] {}: bytes={}, changed_sections={}, changed_paragraphs={}, changed_first_cells={}, changed_first_pictures={}, pages={}",
        out_path.display(),
        hwp_bytes.len(),
        changed_sections,
        changed_paragraphs,
        changed_first_cells,
        changed_first_pictures,
        reloaded.page_count()
    );

    (
        hwp_bytes.len(),
        changed_sections,
        changed_paragraphs,
        changed_first_cells,
        changed_first_pictures,
    )
}

#[derive(Debug, Default)]
struct Task903Stage9Changes {
    bytes: usize,
    base_sections: usize,
    base_first_cells: usize,
    base_first_pictures: usize,
    table_ctrl_headers: usize,
    cell_list_headers: usize,
    table_records: usize,
    cell_para_headers: usize,
    picture_records: usize,
}

fn task903_generate_stage9_probe_variant(
    output_name: &str,
    table_ctrl_header: bool,
    cell_list_headers: bool,
    table_record: bool,
    cell_para_headers: bool,
    picture_records: bool,
) -> Task903Stage9Changes {
    let bytes = load_sample("hwpx-h-01.hwpx");
    let mut core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");

    convert_if_hwpx_source(core.document_mut(), rhwp::parser::FileFormat::Hwpx);

    let reference = task903_reference_hwpx_h_01_core();
    let reference_table = task903_hwpx_h_01_first_table(&reference).clone();

    let mut changes = Task903Stage9Changes::default();

    // Stage 9는 Stage 8의 가장 넓은 보강 상태를 기준선으로 삼고,
    // 첫 표 record payload만 조합별로 바꿔 한컴 판정을 본다.
    for section in &mut core.document_mut().sections {
        changes.base_sections += task903_materialize_stage8_section_def_core(section);
    }
    changes.base_first_cells += task903_materialize_first_cell_list_header_tail_65(&mut core);
    changes.base_first_pictures +=
        task903_materialize_first_picture_common_from_reference(&mut core);

    if table_ctrl_header {
        changes.table_ctrl_headers +=
            task903_materialize_first_table_ctrl_header_from_reference(&mut core, &reference_table);
    }
    if cell_list_headers {
        changes.cell_list_headers +=
            task903_materialize_first_table_cell_list_headers_from_reference(
                &mut core,
                &reference_table,
            );
    }
    if table_record {
        changes.table_records +=
            task903_materialize_first_table_record_from_reference(&mut core, &reference_table);
    }
    if cell_para_headers {
        changes.cell_para_headers +=
            task903_materialize_first_table_cell_para_headers_from_reference(
                &mut core,
                &reference_table,
            );
    }
    if picture_records {
        changes.picture_records += task903_materialize_first_table_picture_records_from_reference(
            &mut core,
            &reference_table,
        );
    }

    let hwp_bytes = core.export_hwp_native().expect("Stage 9 HWP 직렬화 실패");
    let out_dir =
        std::path::Path::new("output/poc/hwpx2hwp/task903/stage9_first_table_payload_probe");
    std::fs::create_dir_all(out_dir).expect("Stage 9 output dir 생성 실패");
    let out_path = out_dir.join(output_name);
    std::fs::write(&out_path, &hwp_bytes).expect("Stage 9 probe 파일 저장 실패");

    changes.bytes = hwp_bytes.len();

    let reloaded = DocumentCore::from_bytes(&hwp_bytes).expect("Stage 9 probe HWP 재로드 실패");
    assert_eq!(reloaded.page_count(), 9, "{} page count", output_name);
    let first_table = task903_hwpx_h_01_first_table(&reloaded);
    assert_eq!(
        first_table.cells.len(),
        reference_table.cells.len(),
        "{} first table cell count",
        output_name
    );
    assert_eq!(
        task903_hwpx_h_01_first_table_picture_common_attrs(&reloaded).len(),
        task903_first_table_picture_clones(&reference_table).len(),
        "{} first table picture count",
        output_name
    );

    if cell_list_headers {
        for (idx, (actual, reference)) in first_table
            .cells
            .iter()
            .zip(reference_table.cells.iter())
            .enumerate()
        {
            assert_eq!(
                actual.raw_list_extra.len(),
                reference.raw_list_extra.len(),
                "{} cell[{}] raw_list_extra length",
                output_name,
                idx
            );
        }
    }

    eprintln!(
        "[#903 Stage 9] {}: bytes={}, base_sections={}, base_first_cells={}, base_first_pictures={}, table_ctrl_headers={}, cell_list_headers={}, table_records={}, cell_para_headers={}, picture_records={}, pages={}",
        out_path.display(),
        changes.bytes,
        changes.base_sections,
        changes.base_first_cells,
        changes.base_first_pictures,
        changes.table_ctrl_headers,
        changes.cell_list_headers,
        changes.table_records,
        changes.cell_para_headers,
        changes.picture_records,
        reloaded.page_count()
    );

    changes
}

#[derive(Debug, Clone, Copy)]
enum Task903FirstTablePicturePatch {
    Common,
    ShapeComponent,
    ScPicture,
    Full,
}

#[derive(Debug, Default)]
struct Task903Stage10Changes {
    bytes: usize,
    base_sections: usize,
    base_first_cells: usize,
    common_records: usize,
    shape_components: usize,
    sc_pictures: usize,
    full_pictures: usize,
}

fn task903_first_table_picture_mut(
    core: &mut DocumentCore,
    picture_idx: usize,
) -> Option<&mut rhwp::model::image::Picture> {
    let table = task903_hwpx_h_01_first_table_mut(core);
    let mut seen = 0usize;
    for cell in &mut table.cells {
        for para in &mut cell.paragraphs {
            for ctrl in &mut para.controls {
                if let Control::Picture(pic) = ctrl {
                    if seen == picture_idx {
                        return Some(pic.as_mut());
                    }
                    seen += 1;
                }
            }
        }
    }
    None
}

fn task903_copy_picture_sc_payload(
    target: &mut rhwp::model::image::Picture,
    reference: &rhwp::model::image::Picture,
) {
    target.border_color = reference.border_color;
    target.border_width = reference.border_width;
    target.border_attr = reference.border_attr;
    target.border_x = reference.border_x;
    target.border_y = reference.border_y;
    target.crop = reference.crop;
    target.padding = reference.padding;
    target.image_attr = reference.image_attr.clone();
    target.border_opacity = reference.border_opacity;
    target.instance_id = reference.instance_id;
    target.raw_picture_extra = reference.raw_picture_extra.clone();
}

fn task903_materialize_first_table_picture_patch_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
    picture_idx: usize,
    patch: Task903FirstTablePicturePatch,
) -> usize {
    let reference_pictures = task903_first_table_picture_clones(reference_table);
    let reference = reference_pictures
        .get(picture_idx)
        .unwrap_or_else(|| panic!("정답 HWP 첫 표 그림[{}] 없음", picture_idx));
    let target = task903_first_table_picture_mut(core, picture_idx)
        .unwrap_or_else(|| panic!("대상 첫 표 그림[{}] 없음", picture_idx));

    match patch {
        Task903FirstTablePicturePatch::Common => {
            target.common = reference.common.clone();
        }
        Task903FirstTablePicturePatch::ShapeComponent => {
            target.shape_attr = reference.shape_attr.clone();
        }
        Task903FirstTablePicturePatch::ScPicture => {
            task903_copy_picture_sc_payload(target, reference);
        }
        Task903FirstTablePicturePatch::Full => {
            target.common = reference.common.clone();
            target.shape_attr = reference.shape_attr.clone();
            task903_copy_picture_sc_payload(target, reference);
        }
    }

    1
}

fn task903_generate_stage10_probe_variant(
    output_name: &str,
    patches: &[(usize, Task903FirstTablePicturePatch)],
) -> Task903Stage10Changes {
    let bytes = load_sample("hwpx-h-01.hwpx");
    let mut core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");

    convert_if_hwpx_source(core.document_mut(), rhwp::parser::FileFormat::Hwpx);

    let reference = task903_reference_hwpx_h_01_core();
    let reference_table = task903_hwpx_h_01_first_table(&reference).clone();

    let mut changes = Task903Stage10Changes::default();

    // Stage 10은 Stage 8의 section core + first cell 65B를 기준선으로 삼는다.
    // 그림 payload는 variant가 명시한 부분만 이식해서 Stage 9의 06 변화를 최소화한다.
    for section in &mut core.document_mut().sections {
        changes.base_sections += task903_materialize_stage8_section_def_core(section);
    }
    changes.base_first_cells += task903_materialize_first_cell_list_header_tail_65(&mut core);

    for (picture_idx, patch) in patches {
        let changed = task903_materialize_first_table_picture_patch_from_reference(
            &mut core,
            &reference_table,
            *picture_idx,
            *patch,
        );
        match patch {
            Task903FirstTablePicturePatch::Common => changes.common_records += changed,
            Task903FirstTablePicturePatch::ShapeComponent => changes.shape_components += changed,
            Task903FirstTablePicturePatch::ScPicture => changes.sc_pictures += changed,
            Task903FirstTablePicturePatch::Full => changes.full_pictures += changed,
        }
    }

    let hwp_bytes = core.export_hwp_native().expect("Stage 10 HWP 직렬화 실패");
    let out_dir =
        std::path::Path::new("output/poc/hwpx2hwp/task903/stage10_minimal_read_error_probe");
    std::fs::create_dir_all(out_dir).expect("Stage 10 output dir 생성 실패");
    let out_path = out_dir.join(output_name);
    std::fs::write(&out_path, &hwp_bytes).expect("Stage 10 probe 파일 저장 실패");

    changes.bytes = hwp_bytes.len();

    let reloaded = DocumentCore::from_bytes(&hwp_bytes).expect("Stage 10 probe HWP 재로드 실패");
    assert_eq!(reloaded.page_count(), 9, "{} page count", output_name);
    let first_table = task903_hwpx_h_01_first_table(&reloaded);
    assert_eq!(
        first_table.cells.len(),
        reference_table.cells.len(),
        "{} first table cell count",
        output_name
    );
    assert_eq!(
        task903_hwpx_h_01_first_table_picture_common_attrs(&reloaded).len(),
        task903_first_table_picture_clones(&reference_table).len(),
        "{} first table picture count",
        output_name
    );

    eprintln!(
        "[#903 Stage 10] {}: bytes={}, base_sections={}, base_first_cells={}, common_records={}, shape_components={}, sc_pictures={}, full_pictures={}, pages={}",
        out_path.display(),
        changes.bytes,
        changes.base_sections,
        changes.base_first_cells,
        changes.common_records,
        changes.shape_components,
        changes.sc_pictures,
        changes.full_pictures,
        reloaded.page_count()
    );

    changes
}

#[derive(Debug, Default)]
struct Task903Stage11Changes {
    bytes: usize,
    base_sections: usize,
    base_first_cells: usize,
    base_full_pictures: usize,
    table_ctrl_headers: usize,
    cell_list_headers: usize,
    table_records: usize,
    cell_para_headers: usize,
    ctrl_data_paragraphs: usize,
}

fn task903_materialize_first_table_cell_ctrl_data_records_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_first_table_mut(core);
    assert_eq!(
        table.cells.len(),
        reference_table.cells.len(),
        "첫 표 셀 수는 정답 HWP와 같아야 함"
    );

    let mut changed = 0usize;
    for (target_cell, reference_cell) in table.cells.iter_mut().zip(reference_table.cells.iter()) {
        let n = target_cell
            .paragraphs
            .len()
            .min(reference_cell.paragraphs.len());
        for idx in 0..n {
            target_cell.paragraphs[idx].ctrl_data_records =
                reference_cell.paragraphs[idx].ctrl_data_records.clone();
            changed += 1;
        }
    }
    changed
}

fn task903_generate_stage11_probe_variant(
    output_name: &str,
    table_ctrl_header: bool,
    cell_list_headers: bool,
    table_record: bool,
    cell_para_headers: bool,
    ctrl_data_records: bool,
) -> Task903Stage11Changes {
    let bytes = load_sample("hwpx-h-01.hwpx");
    let mut core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");

    convert_if_hwpx_source(core.document_mut(), rhwp::parser::FileFormat::Hwpx);

    let reference = task903_reference_hwpx_h_01_core();
    let reference_table = task903_hwpx_h_01_first_table(&reference).clone();

    let mut changes = Task903Stage11Changes::default();

    // Stage 11 기준선: Stage 10의 06을 재현한 뒤 structural payload를 additive하게 붙인다.
    for section in &mut core.document_mut().sections {
        changes.base_sections += task903_materialize_stage8_section_def_core(section);
    }
    changes.base_first_cells += task903_materialize_first_cell_list_header_tail_65(&mut core);
    changes.base_full_pictures += task903_materialize_first_table_picture_patch_from_reference(
        &mut core,
        &reference_table,
        0,
        Task903FirstTablePicturePatch::Full,
    );
    changes.base_full_pictures += task903_materialize_first_table_picture_patch_from_reference(
        &mut core,
        &reference_table,
        1,
        Task903FirstTablePicturePatch::Full,
    );

    if table_ctrl_header {
        changes.table_ctrl_headers +=
            task903_materialize_first_table_ctrl_header_from_reference(&mut core, &reference_table);
    }
    if cell_list_headers {
        changes.cell_list_headers +=
            task903_materialize_first_table_cell_list_headers_from_reference(
                &mut core,
                &reference_table,
            );
    }
    if table_record {
        changes.table_records +=
            task903_materialize_first_table_record_from_reference(&mut core, &reference_table);
    }
    if cell_para_headers {
        changes.cell_para_headers +=
            task903_materialize_first_table_cell_para_headers_from_reference(
                &mut core,
                &reference_table,
            );
    }
    if ctrl_data_records {
        changes.ctrl_data_paragraphs +=
            task903_materialize_first_table_cell_ctrl_data_records_from_reference(
                &mut core,
                &reference_table,
            );
    }

    let hwp_bytes = core.export_hwp_native().expect("Stage 11 HWP 직렬화 실패");
    let out_dir =
        std::path::Path::new("output/poc/hwpx2hwp/task903/stage11_picture_structural_combo_probe");
    std::fs::create_dir_all(out_dir).expect("Stage 11 output dir 생성 실패");
    let out_path = out_dir.join(output_name);
    std::fs::write(&out_path, &hwp_bytes).expect("Stage 11 probe 파일 저장 실패");

    changes.bytes = hwp_bytes.len();

    let reloaded = DocumentCore::from_bytes(&hwp_bytes).expect("Stage 11 probe HWP 재로드 실패");
    assert_eq!(reloaded.page_count(), 9, "{} page count", output_name);
    let first_table = task903_hwpx_h_01_first_table(&reloaded);
    assert_eq!(
        first_table.cells.len(),
        reference_table.cells.len(),
        "{} first table cell count",
        output_name
    );
    assert_eq!(
        task903_hwpx_h_01_first_table_picture_common_attrs(&reloaded).len(),
        task903_first_table_picture_clones(&reference_table).len(),
        "{} first table picture count",
        output_name
    );

    eprintln!(
        "[#903 Stage 11] {}: bytes={}, base_sections={}, base_first_cells={}, base_full_pictures={}, table_ctrl_headers={}, cell_list_headers={}, table_records={}, cell_para_headers={}, ctrl_data_paragraphs={}, pages={}",
        out_path.display(),
        changes.bytes,
        changes.base_sections,
        changes.base_first_cells,
        changes.base_full_pictures,
        changes.table_ctrl_headers,
        changes.cell_list_headers,
        changes.table_records,
        changes.cell_para_headers,
        changes.ctrl_data_paragraphs,
        reloaded.page_count()
    );

    changes
}

fn task903_hwpx_h_01_second_table(core: &DocumentCore) -> &Table {
    task903_hwpx_h_01_table_at(core, 3, 0)
}

fn task903_hwpx_h_01_second_table_mut(core: &mut DocumentCore) -> &mut Table {
    let para = &mut core.document_mut().sections[0].paragraphs[3];
    match para.controls.get_mut(0) {
        Some(Control::Table(table)) => table.as_mut(),
        other => panic!("문단 0:3 첫 컨트롤은 두 번째 표여야 함: {:?}", other),
    }
}

fn task903_copy_para_header_payload(target: &mut Paragraph, reference: &Paragraph) {
    target.char_count = reference.char_count;
    target.control_mask = reference.control_mask;
    target.para_shape_id = reference.para_shape_id;
    target.style_id = reference.style_id;
    target.column_type = reference.column_type;
    target.raw_break_type = reference.raw_break_type;
    target.char_count_msb = reference.char_count_msb;
    target.raw_header_extra = reference.raw_header_extra.clone();
}

fn task903_copy_para_records_without_controls(target: &mut Paragraph, reference: &Paragraph) {
    task903_copy_para_header_payload(target, reference);
    target.text = reference.text.clone();
    target.char_offsets = reference.char_offsets.clone();
    target.char_shapes = reference.char_shapes.clone();
    target.line_segs = reference.line_segs.clone();
    target.range_tags = reference.range_tags.clone();
    target.field_ranges = reference.field_ranges.clone();
    target.has_para_text = reference.has_para_text;
    target.tab_extended = reference.tab_extended.clone();
    target.numbering_restart = reference.numbering_restart;
}

fn task903_materialize_post_first_table_top_level_para_headers_from_reference(
    core: &mut DocumentCore,
    reference: &DocumentCore,
) -> usize {
    let mut changed = 0usize;
    for para_idx in [1usize, 2, 3] {
        let target = &mut core.document_mut().sections[0].paragraphs[para_idx];
        let reference = &reference.document().sections[0].paragraphs[para_idx];
        task903_copy_para_header_payload(target, reference);
        changed += 1;
    }
    changed
}

fn task903_materialize_post_first_table_top_level_para_records_from_reference(
    core: &mut DocumentCore,
    reference: &DocumentCore,
) -> usize {
    let mut changed = 0usize;
    for para_idx in [1usize, 2, 3] {
        let target = &mut core.document_mut().sections[0].paragraphs[para_idx];
        let reference = &reference.document().sections[0].paragraphs[para_idx];
        task903_copy_para_records_without_controls(target, reference);
        changed += 1;
    }
    changed
}

fn task903_copy_table_record_payload(target: &mut Table, reference: &Table) {
    target.attr = reference.attr;
    target.row_count = reference.row_count;
    target.col_count = reference.col_count;
    target.cell_spacing = reference.cell_spacing;
    target.padding = reference.padding;
    target.row_sizes = reference.row_sizes.clone();
    target.border_fill_id = reference.border_fill_id;
    target.zones = reference.zones.clone();
    target.page_break = reference.page_break;
    target.repeat_header = reference.repeat_header;
    target.raw_table_record_attr = reference.raw_table_record_attr;
    target.raw_table_record_extra = reference.raw_table_record_extra.clone();
}

fn task903_table_record_tail_with_zone_count(reference: &Table) -> Vec<u8> {
    let mut tail = Vec::new();
    tail.extend_from_slice(&(reference.zones.len() as u16).to_le_bytes());
    for zone in &reference.zones {
        tail.extend_from_slice(&zone.start_row.to_le_bytes());
        tail.extend_from_slice(&zone.start_col.to_le_bytes());
        tail.extend_from_slice(&zone.end_row.to_le_bytes());
        tail.extend_from_slice(&zone.end_col.to_le_bytes());
        tail.extend_from_slice(&zone.border_fill_id.to_le_bytes());
    }
    tail.extend_from_slice(&reference.raw_table_record_extra);
    tail
}

fn task903_copy_table_record_payload_with_encoded_tail(target: &mut Table, reference: &Table) {
    task903_copy_table_record_payload(target, reference);
    target.raw_table_record_extra = task903_table_record_tail_with_zone_count(reference);
}

fn task903_copy_cell_list_header_payload(
    target: &mut rhwp::model::table::Cell,
    reference: &rhwp::model::table::Cell,
) {
    target.list_header_width_ref = reference.list_header_width_ref;
    target.text_direction = reference.text_direction;
    target.vertical_align = reference.vertical_align;
    target.col = reference.col;
    target.row = reference.row;
    target.col_span = reference.col_span;
    target.row_span = reference.row_span;
    target.width = reference.width;
    target.height = reference.height;
    target.padding = reference.padding;
    target.border_fill_id = reference.border_fill_id;
    target.apply_inner_margin = reference.apply_inner_margin;
    target.is_header = reference.is_header;
    target.raw_list_extra = reference.raw_list_extra.clone();
    target.field_name = reference.field_name.clone();
}

fn task903_materialize_next_table_ctrl_header_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_second_table_mut(core);
    table.raw_ctrl_data = reference_table.raw_ctrl_data.clone();
    1
}

fn task903_materialize_next_table_child_headers_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_second_table_mut(core);
    assert_eq!(
        table.cells.len(),
        reference_table.cells.len(),
        "두 번째 표 셀 수는 정답 HWP와 같아야 함"
    );

    task903_copy_table_record_payload(table, reference_table);

    let mut changed = 1usize;
    for (target_cell, reference_cell) in table.cells.iter_mut().zip(reference_table.cells.iter()) {
        task903_copy_cell_list_header_payload(target_cell, reference_cell);
        changed += 1;

        let n = target_cell
            .paragraphs
            .len()
            .min(reference_cell.paragraphs.len());
        for idx in 0..n {
            task903_copy_para_header_payload(
                &mut target_cell.paragraphs[idx],
                &reference_cell.paragraphs[idx],
            );
            changed += 1;
        }
    }

    changed
}

fn task903_materialize_next_table_record_span_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let target = task903_hwpx_h_01_second_table_mut(core);
    *target = reference_table.clone();
    1
}

#[derive(Debug, Default)]
struct Task903Stage13Changes {
    bytes: usize,
    base_sections: usize,
    base_first_cells: usize,
    base_full_pictures: usize,
    base_first_table_ctrl_headers: usize,
    base_first_table_cell_list_headers: usize,
    base_first_table_records: usize,
    base_first_table_cell_para_headers: usize,
    post_top_level_para_headers: usize,
    post_top_level_para_records: usize,
    next_table_ctrl_headers: usize,
    next_table_child_headers: usize,
    next_table_record_spans: usize,
}

fn task903_generate_stage13_probe_variant(
    output_name: &str,
    post_top_level_para_headers: bool,
    post_top_level_para_records: bool,
    next_table_ctrl_header: bool,
    next_table_child_headers: bool,
    next_table_record_span: bool,
) -> Task903Stage13Changes {
    let bytes = load_sample("hwpx-h-01.hwpx");
    let mut core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");

    convert_if_hwpx_source(core.document_mut(), rhwp::parser::FileFormat::Hwpx);

    let reference = task903_reference_hwpx_h_01_core();
    let reference_first_table = task903_hwpx_h_01_first_table(&reference).clone();
    let reference_second_table = task903_hwpx_h_01_second_table(&reference).clone();

    let mut changes = Task903Stage13Changes::default();

    // Stage 13 기준선: Stage 11의 05_picture_full_plus_structural_bundle을 재현한다.
    for section in &mut core.document_mut().sections {
        changes.base_sections += task903_materialize_stage8_section_def_core(section);
    }
    changes.base_first_cells += task903_materialize_first_cell_list_header_tail_65(&mut core);
    changes.base_full_pictures += task903_materialize_first_table_picture_patch_from_reference(
        &mut core,
        &reference_first_table,
        0,
        Task903FirstTablePicturePatch::Full,
    );
    changes.base_full_pictures += task903_materialize_first_table_picture_patch_from_reference(
        &mut core,
        &reference_first_table,
        1,
        Task903FirstTablePicturePatch::Full,
    );
    changes.base_first_table_ctrl_headers +=
        task903_materialize_first_table_ctrl_header_from_reference(
            &mut core,
            &reference_first_table,
        );
    changes.base_first_table_cell_list_headers +=
        task903_materialize_first_table_cell_list_headers_from_reference(
            &mut core,
            &reference_first_table,
        );
    changes.base_first_table_records +=
        task903_materialize_first_table_record_from_reference(&mut core, &reference_first_table);
    changes.base_first_table_cell_para_headers +=
        task903_materialize_first_table_cell_para_headers_from_reference(
            &mut core,
            &reference_first_table,
        );

    if post_top_level_para_records {
        changes.post_top_level_para_records +=
            task903_materialize_post_first_table_top_level_para_records_from_reference(
                &mut core, &reference,
            );
    } else if post_top_level_para_headers {
        changes.post_top_level_para_headers +=
            task903_materialize_post_first_table_top_level_para_headers_from_reference(
                &mut core, &reference,
            );
    }

    if next_table_record_span {
        changes.next_table_record_spans +=
            task903_materialize_next_table_record_span_from_reference(
                &mut core,
                &reference_second_table,
            );
    } else {
        if next_table_ctrl_header {
            changes.next_table_ctrl_headers +=
                task903_materialize_next_table_ctrl_header_from_reference(
                    &mut core,
                    &reference_second_table,
                );
        }
        if next_table_child_headers {
            changes.next_table_child_headers +=
                task903_materialize_next_table_child_headers_from_reference(
                    &mut core,
                    &reference_second_table,
                );
        }
    }

    let hwp_bytes = core.export_hwp_native().expect("Stage 13 HWP 직렬화 실패");
    let out_dir = std::path::Path::new(
        "output/poc/hwpx2hwp/task903/stage13_after_first_table_boundary_probe",
    );
    std::fs::create_dir_all(out_dir).expect("Stage 13 output dir 생성 실패");
    let out_path = out_dir.join(output_name);
    std::fs::write(&out_path, &hwp_bytes).expect("Stage 13 probe 파일 저장 실패");

    changes.bytes = hwp_bytes.len();

    let reloaded = DocumentCore::from_bytes(&hwp_bytes).expect("Stage 13 probe HWP 재로드 실패");
    assert_eq!(reloaded.page_count(), 9, "{} page count", output_name);
    let first_table = task903_hwpx_h_01_first_table(&reloaded);
    assert_eq!(
        first_table.cells.len(),
        reference_first_table.cells.len(),
        "{} first table cell count",
        output_name
    );
    assert_eq!(
        task903_hwpx_h_01_first_table_picture_common_attrs(&reloaded).len(),
        task903_first_table_picture_clones(&reference_first_table).len(),
        "{} first table picture count",
        output_name
    );

    if post_top_level_para_headers || post_top_level_para_records {
        for para_idx in [1usize, 2, 3] {
            assert_eq!(
                reloaded.document().sections[0].paragraphs[para_idx]
                    .raw_header_extra
                    .len(),
                reference.document().sections[0].paragraphs[para_idx]
                    .raw_header_extra
                    .len(),
                "{} paragraph {} raw_header_extra length",
                output_name,
                para_idx
            );
        }
    }
    if next_table_ctrl_header || next_table_record_span {
        assert_eq!(
            task903_hwpx_h_01_second_table(&reloaded).raw_ctrl_data,
            reference_second_table.raw_ctrl_data,
            "{} next table CTRL_HEADER payload",
            output_name
        );
    }
    if next_table_child_headers || next_table_record_span {
        let second_table = task903_hwpx_h_01_second_table(&reloaded);
        assert_eq!(
            second_table.raw_table_record_extra.len(),
            reference_second_table.raw_table_record_extra.len(),
            "{} next table TABLE extra length",
            output_name
        );
        assert_eq!(
            second_table.cells[0].raw_list_extra.len(),
            reference_second_table.cells[0].raw_list_extra.len(),
            "{} next table first cell LIST_HEADER extra length",
            output_name
        );
        assert_eq!(
            second_table.cells[0].paragraphs[0].raw_header_extra.len(),
            reference_second_table.cells[0].paragraphs[0]
                .raw_header_extra
                .len(),
            "{} next table first cell PARA_HEADER extra length",
            output_name
        );
    }

    eprintln!(
        "[#903 Stage 13] {}: bytes={}, base_sections={}, base_first_cells={}, base_full_pictures={}, base_first_table_ctrl_headers={}, base_first_table_cell_list_headers={}, base_first_table_records={}, base_first_table_cell_para_headers={}, post_top_level_para_headers={}, post_top_level_para_records={}, next_table_ctrl_headers={}, next_table_child_headers={}, next_table_record_spans={}, pages={}",
        out_path.display(),
        changes.bytes,
        changes.base_sections,
        changes.base_first_cells,
        changes.base_full_pictures,
        changes.base_first_table_ctrl_headers,
        changes.base_first_table_cell_list_headers,
        changes.base_first_table_records,
        changes.base_first_table_cell_para_headers,
        changes.post_top_level_para_headers,
        changes.post_top_level_para_records,
        changes.next_table_ctrl_headers,
        changes.next_table_child_headers,
        changes.next_table_record_spans,
        reloaded.page_count()
    );

    changes
}

fn task903_hwpx_h_01_chart_table(core: &DocumentCore) -> &Table {
    task903_hwpx_h_01_table_at(core, 10, 0)
}

fn task903_hwpx_h_01_chart_table_mut(core: &mut DocumentCore) -> &mut Table {
    let para = &mut core.document_mut().sections[0].paragraphs[10];
    match para.controls.get_mut(0) {
        Some(Control::Table(table)) => table.as_mut(),
        other => panic!("문단 0:10 첫 컨트롤은 chart 표여야 함: {:?}", other),
    }
}

fn task903_materialize_chart_table_ctrl_header_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_chart_table_mut(core);
    table.raw_ctrl_data = reference_table.raw_ctrl_data.clone();
    1
}

fn task903_materialize_chart_table_record_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_chart_table_mut(core);
    task903_copy_table_record_payload(table, reference_table);
    1
}

fn task903_materialize_chart_table_record_with_encoded_tail_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_chart_table_mut(core);
    task903_copy_table_record_payload_with_encoded_tail(table, reference_table);
    1
}

fn task903_materialize_chart_table_first_cell_headers_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_chart_table_mut(core);
    assert!(
        !table.cells.is_empty() && !reference_table.cells.is_empty(),
        "chart 표 첫 셀은 존재해야 함"
    );

    let target_cell = &mut table.cells[0];
    let reference_cell = &reference_table.cells[0];
    task903_copy_cell_list_header_payload(target_cell, reference_cell);

    let mut changed = 1usize;
    let n = target_cell
        .paragraphs
        .len()
        .min(reference_cell.paragraphs.len());
    for idx in 0..n {
        task903_copy_para_header_payload(
            &mut target_cell.paragraphs[idx],
            &reference_cell.paragraphs[idx],
        );
        changed += 1;
    }
    changed
}

fn task903_materialize_chart_table_all_cell_headers_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_chart_table_mut(core);
    assert_eq!(
        table.cells.len(),
        reference_table.cells.len(),
        "chart 표 셀 수는 정답 HWP와 같아야 함"
    );

    let mut changed = 0usize;
    for (target_cell, reference_cell) in table.cells.iter_mut().zip(reference_table.cells.iter()) {
        task903_copy_cell_list_header_payload(target_cell, reference_cell);
        changed += 1;

        let n = target_cell
            .paragraphs
            .len()
            .min(reference_cell.paragraphs.len());
        for idx in 0..n {
            task903_copy_para_header_payload(
                &mut target_cell.paragraphs[idx],
                &reference_cell.paragraphs[idx],
            );
            changed += 1;
        }
    }
    changed
}

fn task903_materialize_chart_table_full_object_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let target = task903_hwpx_h_01_chart_table_mut(core);
    *target = reference_table.clone();
    1
}

fn task903_materialize_chart_table_full_object_with_encoded_tail_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let target = task903_hwpx_h_01_chart_table_mut(core);
    *target = reference_table.clone();
    target.raw_table_record_extra = task903_table_record_tail_with_zone_count(reference_table);
    1
}

fn task903_materialize_top_level_para_record_from_reference(
    core: &mut DocumentCore,
    reference: &DocumentCore,
    para_idx: usize,
) -> usize {
    task903_materialize_para_record_from_reference(core, reference, 0, para_idx)
}

fn task903_materialize_para_record_from_reference(
    core: &mut DocumentCore,
    reference: &DocumentCore,
    section_idx: usize,
    para_idx: usize,
) -> usize {
    let target = &mut core.document_mut().sections[section_idx].paragraphs[para_idx];
    let reference = &reference.document().sections[section_idx].paragraphs[para_idx];
    task903_copy_para_records_without_controls(target, reference);
    1
}

fn task903_materialize_full_para_from_reference(
    core: &mut DocumentCore,
    reference: &DocumentCore,
    section_idx: usize,
    para_idx: usize,
) -> usize {
    core.document_mut().sections[section_idx].paragraphs[para_idx] =
        reference.document().sections[section_idx].paragraphs[para_idx].clone();
    1
}

fn task903_hwpx_h_01_industry_table(core: &DocumentCore) -> &Table {
    task903_hwpx_h_01_table_at(core, 14, 0)
}

fn task903_hwpx_h_01_industry_table_mut(core: &mut DocumentCore) -> &mut Table {
    let para = &mut core.document_mut().sections[0].paragraphs[14];
    match para.controls.get_mut(0) {
        Some(Control::Table(table)) => table.as_mut(),
        other => panic!("문단 0:14 첫 컨트롤은 industry 표여야 함: {:?}", other),
    }
}

fn task903_materialize_industry_table_ctrl_header_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_industry_table_mut(core);
    table.raw_ctrl_data = reference_table.raw_ctrl_data.clone();
    1
}

fn task903_materialize_industry_table_record_with_encoded_tail_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_industry_table_mut(core);
    task903_copy_table_record_payload_with_encoded_tail(table, reference_table);
    1
}

fn task903_materialize_industry_table_all_cell_headers_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_industry_table_mut(core);
    assert_eq!(
        table.cells.len(),
        reference_table.cells.len(),
        "industry 표 셀 수는 정답 HWP와 같아야 함"
    );

    let mut changed = 0usize;
    for (target_cell, reference_cell) in table.cells.iter_mut().zip(reference_table.cells.iter()) {
        task903_copy_cell_list_header_payload(target_cell, reference_cell);
        changed += 1;

        let n = target_cell
            .paragraphs
            .len()
            .min(reference_cell.paragraphs.len());
        for idx in 0..n {
            task903_copy_para_header_payload(
                &mut target_cell.paragraphs[idx],
                &reference_cell.paragraphs[idx],
            );
            changed += 1;
        }
    }
    changed
}

fn task903_materialize_industry_table_full_object_with_encoded_tail_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let target = task903_hwpx_h_01_industry_table_mut(core);
    *target = reference_table.clone();
    target.raw_table_record_extra = task903_table_record_tail_with_zone_count(reference_table);
    1
}

fn task903_hwpx_h_01_country_table(core: &DocumentCore) -> &Table {
    task903_hwpx_h_01_table_at(core, 21, 0)
}

fn task903_hwpx_h_01_country_table_mut(core: &mut DocumentCore) -> &mut Table {
    let para = &mut core.document_mut().sections[0].paragraphs[21];
    match para.controls.get_mut(0) {
        Some(Control::Table(table)) => table.as_mut(),
        other => panic!("문단 0:21 첫 컨트롤은 country 표여야 함: {:?}", other),
    }
}

fn task903_materialize_country_table_ctrl_header_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_country_table_mut(core);
    table.raw_ctrl_data = reference_table.raw_ctrl_data.clone();
    1
}

fn task903_materialize_country_table_record_with_encoded_tail_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_country_table_mut(core);
    task903_copy_table_record_payload_with_encoded_tail(table, reference_table);
    1
}

fn task903_materialize_country_table_all_cell_headers_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_country_table_mut(core);
    assert_eq!(
        table.cells.len(),
        reference_table.cells.len(),
        "country 표 셀 수는 정답 HWP와 같아야 함"
    );

    let mut changed = 0usize;
    for (target_cell, reference_cell) in table.cells.iter_mut().zip(reference_table.cells.iter()) {
        task903_copy_cell_list_header_payload(target_cell, reference_cell);
        changed += 1;

        let n = target_cell
            .paragraphs
            .len()
            .min(reference_cell.paragraphs.len());
        for idx in 0..n {
            task903_copy_para_header_payload(
                &mut target_cell.paragraphs[idx],
                &reference_cell.paragraphs[idx],
            );
            changed += 1;
        }
    }
    changed
}

fn task903_materialize_country_table_full_object_with_encoded_tail_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let target = task903_hwpx_h_01_country_table_mut(core);
    *target = reference_table.clone();
    target.raw_table_record_extra = task903_table_record_tail_with_zone_count(reference_table);
    1
}

fn task903_hwpx_h_01_region_table(core: &DocumentCore) -> &Table {
    task903_hwpx_h_01_table_at(core, 23, 0)
}

fn task903_hwpx_h_01_region_table_mut(core: &mut DocumentCore) -> &mut Table {
    let para = &mut core.document_mut().sections[0].paragraphs[23];
    match para.controls.get_mut(0) {
        Some(Control::Table(table)) => table.as_mut(),
        other => panic!("문단 0:23 첫 컨트롤은 region 표여야 함: {:?}", other),
    }
}

fn task903_materialize_region_table_ctrl_header_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_region_table_mut(core);
    table.raw_ctrl_data = reference_table.raw_ctrl_data.clone();
    1
}

fn task903_materialize_region_table_record_with_encoded_tail_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_region_table_mut(core);
    task903_copy_table_record_payload_with_encoded_tail(table, reference_table);
    1
}

fn task903_materialize_region_table_all_cell_headers_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_region_table_mut(core);
    assert_eq!(
        table.cells.len(),
        reference_table.cells.len(),
        "region 표 셀 수는 정답 HWP와 같아야 함"
    );

    let mut changed = 0usize;
    for (target_cell, reference_cell) in table.cells.iter_mut().zip(reference_table.cells.iter()) {
        task903_copy_cell_list_header_payload(target_cell, reference_cell);
        changed += 1;

        let n = target_cell
            .paragraphs
            .len()
            .min(reference_cell.paragraphs.len());
        for idx in 0..n {
            task903_copy_para_header_payload(
                &mut target_cell.paragraphs[idx],
                &reference_cell.paragraphs[idx],
            );
            changed += 1;
        }
    }
    changed
}

fn task903_materialize_region_table_full_object_with_encoded_tail_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let target = task903_hwpx_h_01_region_table_mut(core);
    *target = reference_table.clone();
    target.raw_table_record_extra = task903_table_record_tail_with_zone_count(reference_table);
    1
}

fn task903_hwpx_h_01_notice_table(core: &DocumentCore) -> &Table {
    task903_hwpx_h_01_table_at(core, 28, 0)
}

fn task903_hwpx_h_01_notice_table_mut(core: &mut DocumentCore) -> &mut Table {
    task903_hwpx_h_01_table_at_mut(core, 28, 0)
}

fn task903_materialize_notice_table_ctrl_header_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_notice_table_mut(core);
    table.raw_ctrl_data = reference_table.raw_ctrl_data.clone();
    1
}

fn task903_materialize_notice_table_record_with_encoded_tail_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_notice_table_mut(core);
    task903_copy_table_record_payload_with_encoded_tail(table, reference_table);
    1
}

fn task903_materialize_notice_table_all_cell_headers_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_notice_table_mut(core);
    assert_eq!(
        table.cells.len(),
        reference_table.cells.len(),
        "notice 표 셀 수는 정답 HWP와 같아야 함"
    );

    let mut changed = 0usize;
    for (target_cell, reference_cell) in table.cells.iter_mut().zip(reference_table.cells.iter()) {
        task903_copy_cell_list_header_payload(target_cell, reference_cell);
        changed += 1;

        let n = target_cell
            .paragraphs
            .len()
            .min(reference_cell.paragraphs.len());
        for idx in 0..n {
            task903_copy_para_header_payload(
                &mut target_cell.paragraphs[idx],
                &reference_cell.paragraphs[idx],
            );
            changed += 1;
        }
    }
    changed
}

fn task903_materialize_notice_table_full_object_with_encoded_tail_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let target = task903_hwpx_h_01_notice_table_mut(core);
    *target = reference_table.clone();
    target.raw_table_record_extra = task903_table_record_tail_with_zone_count(reference_table);
    1
}

fn task903_materialize_logo_group_common_from_reference(
    core: &mut DocumentCore,
    reference_group: &GroupShape,
) -> usize {
    let group = task903_hwpx_h_01_logo_group_mut(core);
    group.common = reference_group.common.clone();
    1
}

fn task903_materialize_logo_group_shape_attr_from_reference(
    core: &mut DocumentCore,
    reference_group: &GroupShape,
) -> usize {
    let group = task903_hwpx_h_01_logo_group_mut(core);
    group.shape_attr = reference_group.shape_attr.clone();
    1
}

fn task903_materialize_logo_group_child_shape_attrs_from_reference(
    core: &mut DocumentCore,
    reference_group: &GroupShape,
) -> usize {
    let group = task903_hwpx_h_01_logo_group_mut(core);
    assert_eq!(
        group.children.len(),
        reference_group.children.len(),
        "logo group child 수는 정답 HWP와 같아야 함"
    );

    let mut changed = 0usize;
    for (target_child, reference_child) in group
        .children
        .iter_mut()
        .zip(reference_group.children.iter())
    {
        match (target_child, reference_child) {
            (ShapeObject::Picture(target), ShapeObject::Picture(reference)) => {
                target.shape_attr = reference.shape_attr.clone();
                changed += 1;
            }
            (ShapeObject::Group(target), ShapeObject::Group(reference)) => {
                target.shape_attr = reference.shape_attr.clone();
                changed += 1;
            }
            (target, reference) => panic!(
                "logo group child type mismatch: target={:?}, reference={:?}",
                target, reference
            ),
        }
    }
    changed
}

fn task903_materialize_logo_group_child_full_pictures_from_reference(
    core: &mut DocumentCore,
    reference_group: &GroupShape,
) -> usize {
    let group = task903_hwpx_h_01_logo_group_mut(core);
    assert_eq!(
        group.children.len(),
        reference_group.children.len(),
        "logo group child 수는 정답 HWP와 같아야 함"
    );
    group.children = reference_group.children.clone();
    group.children.len()
}

fn task903_materialize_logo_group_full_object_from_reference(
    core: &mut DocumentCore,
    reference_group: &GroupShape,
) -> usize {
    let group = task903_hwpx_h_01_logo_group_mut(core);
    *group = reference_group.clone();
    1
}

fn task903_hwpx_h_01_attachment_title_table(core: &DocumentCore) -> &Table {
    task903_hwpx_h_01_table_at(core, 30, 0)
}

fn task903_hwpx_h_01_attachment_title_table_mut(core: &mut DocumentCore) -> &mut Table {
    task903_hwpx_h_01_table_at_mut(core, 30, 0)
}

fn task903_materialize_attachment_title_table_full_object_with_encoded_tail_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let target = task903_hwpx_h_01_attachment_title_table_mut(core);
    *target = reference_table.clone();
    target.raw_table_record_extra = task903_table_record_tail_with_zone_count(reference_table);
    1
}

fn task903_hwpx_h_01_top_country_table(core: &DocumentCore) -> &Table {
    task903_hwpx_h_01_table_at(core, 44, 0)
}

fn task903_hwpx_h_01_top_country_table_mut(core: &mut DocumentCore) -> &mut Table {
    task903_hwpx_h_01_table_at_mut(core, 44, 0)
}

fn task903_materialize_top_country_table_ctrl_header_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_top_country_table_mut(core);
    table.raw_ctrl_data = reference_table.raw_ctrl_data.clone();
    1
}

fn task903_materialize_top_country_table_record_with_encoded_tail_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_top_country_table_mut(core);
    task903_copy_table_record_payload_with_encoded_tail(table, reference_table);
    1
}

fn task903_materialize_top_country_table_all_cell_headers_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_top_country_table_mut(core);
    assert_eq!(
        table.cells.len(),
        reference_table.cells.len(),
        "top country 표 셀 수는 정답 HWP와 같아야 함"
    );

    let mut changed = 0usize;
    for (target_cell, reference_cell) in table.cells.iter_mut().zip(reference_table.cells.iter()) {
        task903_copy_cell_list_header_payload(target_cell, reference_cell);
        changed += 1;

        let n = target_cell
            .paragraphs
            .len()
            .min(reference_cell.paragraphs.len());
        for idx in 0..n {
            task903_copy_para_header_payload(
                &mut target_cell.paragraphs[idx],
                &reference_cell.paragraphs[idx],
            );
            changed += 1;
        }
    }
    changed
}

fn task903_materialize_top_country_table_full_object_with_encoded_tail_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let target = task903_hwpx_h_01_top_country_table_mut(core);
    *target = reference_table.clone();
    target.raw_table_record_extra = task903_table_record_tail_with_zone_count(reference_table);
    1
}

fn task903_hwpx_h_01_year_trend_table(core: &DocumentCore) -> &Table {
    task903_hwpx_h_01_table_at(core, 52, 0)
}

fn task903_hwpx_h_01_year_trend_table_mut(core: &mut DocumentCore) -> &mut Table {
    task903_hwpx_h_01_table_at_mut(core, 52, 0)
}

fn task903_materialize_year_trend_table_ctrl_header_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_year_trend_table_mut(core);
    table.raw_ctrl_data = reference_table.raw_ctrl_data.clone();
    1
}

fn task903_materialize_year_trend_table_record_with_encoded_tail_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_year_trend_table_mut(core);
    task903_copy_table_record_payload_with_encoded_tail(table, reference_table);
    1
}

fn task903_materialize_year_trend_table_all_cell_headers_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_year_trend_table_mut(core);
    assert_eq!(
        table.cells.len(),
        reference_table.cells.len(),
        "year trend 표 셀 수는 정답 HWP와 같아야 함"
    );

    let mut changed = 0usize;
    for (target_cell, reference_cell) in table.cells.iter_mut().zip(reference_table.cells.iter()) {
        task903_copy_cell_list_header_payload(target_cell, reference_cell);
        changed += 1;

        let n = target_cell
            .paragraphs
            .len()
            .min(reference_cell.paragraphs.len());
        for idx in 0..n {
            task903_copy_para_header_payload(
                &mut target_cell.paragraphs[idx],
                &reference_cell.paragraphs[idx],
            );
            changed += 1;
        }
    }
    changed
}

fn task903_materialize_year_trend_table_full_object_with_encoded_tail_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let target = task903_hwpx_h_01_year_trend_table_mut(core);
    *target = reference_table.clone();
    target.raw_table_record_extra = task903_table_record_tail_with_zone_count(reference_table);
    1
}

fn task903_hwpx_h_01_second_year_trend_table(core: &DocumentCore) -> &Table {
    task903_hwpx_h_01_table_at(core, 89, 0)
}

fn task903_hwpx_h_01_second_year_trend_table_mut(core: &mut DocumentCore) -> &mut Table {
    task903_hwpx_h_01_table_at_mut(core, 89, 0)
}

fn task903_materialize_second_year_trend_table_ctrl_header_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_second_year_trend_table_mut(core);
    table.raw_ctrl_data = reference_table.raw_ctrl_data.clone();
    1
}

fn task903_materialize_second_year_trend_table_record_with_encoded_tail_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_second_year_trend_table_mut(core);
    task903_copy_table_record_payload_with_encoded_tail(table, reference_table);
    1
}

fn task903_materialize_second_year_trend_table_all_cell_headers_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_second_year_trend_table_mut(core);
    assert_eq!(
        table.cells.len(),
        reference_table.cells.len(),
        "second year trend 표 셀 수는 정답 HWP와 같아야 함"
    );

    let mut changed = 0usize;
    for (target_cell, reference_cell) in table.cells.iter_mut().zip(reference_table.cells.iter()) {
        task903_copy_cell_list_header_payload(target_cell, reference_cell);
        changed += 1;

        let n = target_cell
            .paragraphs
            .len()
            .min(reference_cell.paragraphs.len());
        for idx in 0..n {
            task903_copy_para_header_payload(
                &mut target_cell.paragraphs[idx],
                &reference_cell.paragraphs[idx],
            );
            changed += 1;
        }
    }
    changed
}

fn task903_materialize_second_year_trend_table_full_object_with_encoded_tail_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let target = task903_hwpx_h_01_second_year_trend_table_mut(core);
    *target = reference_table.clone();
    target.raw_table_record_extra = task903_table_record_tail_with_zone_count(reference_table);
    1
}

fn task903_hwpx_h_01_final_industry_table(core: &DocumentCore) -> &Table {
    task903_hwpx_h_01_table_at(core, 94, 0)
}

fn task903_hwpx_h_01_final_industry_table_mut(core: &mut DocumentCore) -> &mut Table {
    task903_hwpx_h_01_table_at_mut(core, 94, 0)
}

fn task903_materialize_final_industry_table_ctrl_header_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_final_industry_table_mut(core);
    table.raw_ctrl_data = reference_table.raw_ctrl_data.clone();
    1
}

fn task903_materialize_final_industry_table_record_with_encoded_tail_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_final_industry_table_mut(core);
    task903_copy_table_record_payload_with_encoded_tail(table, reference_table);
    1
}

fn task903_materialize_final_industry_table_all_cell_headers_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_final_industry_table_mut(core);
    assert_eq!(
        table.cells.len(),
        reference_table.cells.len(),
        "final industry 표 셀 수는 정답 HWP와 같아야 함"
    );

    let mut changed = 0usize;
    for (target_cell, reference_cell) in table.cells.iter_mut().zip(reference_table.cells.iter()) {
        task903_copy_cell_list_header_payload(target_cell, reference_cell);
        changed += 1;

        let n = target_cell
            .paragraphs
            .len()
            .min(reference_cell.paragraphs.len());
        for idx in 0..n {
            task903_copy_para_header_payload(
                &mut target_cell.paragraphs[idx],
                &reference_cell.paragraphs[idx],
            );
            changed += 1;
        }
    }
    changed
}

fn task903_materialize_final_industry_table_full_object_with_encoded_tail_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let target = task903_hwpx_h_01_final_industry_table_mut(core);
    *target = reference_table.clone();
    target.raw_table_record_extra = task903_table_record_tail_with_zone_count(reference_table);
    1
}

fn task903_hwpx_h_01_final_country_table(core: &DocumentCore) -> &Table {
    task903_hwpx_h_01_table_at(core, 97, 0)
}

fn task903_hwpx_h_01_final_country_table_mut(core: &mut DocumentCore) -> &mut Table {
    task903_hwpx_h_01_table_at_mut(core, 97, 0)
}

fn task903_materialize_final_country_table_ctrl_header_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_final_country_table_mut(core);
    table.raw_ctrl_data = reference_table.raw_ctrl_data.clone();
    1
}

fn task903_materialize_final_country_table_record_with_encoded_tail_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_final_country_table_mut(core);
    task903_copy_table_record_payload_with_encoded_tail(table, reference_table);
    1
}

fn task903_materialize_final_country_table_all_cell_headers_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_final_country_table_mut(core);
    assert_eq!(
        table.cells.len(),
        reference_table.cells.len(),
        "final country 표 셀 수는 정답 HWP와 같아야 함"
    );

    let mut changed = 0usize;
    for (target_cell, reference_cell) in table.cells.iter_mut().zip(reference_table.cells.iter()) {
        task903_copy_cell_list_header_payload(target_cell, reference_cell);
        changed += 1;

        let n = target_cell
            .paragraphs
            .len()
            .min(reference_cell.paragraphs.len());
        for idx in 0..n {
            task903_copy_para_header_payload(
                &mut target_cell.paragraphs[idx],
                &reference_cell.paragraphs[idx],
            );
            changed += 1;
        }
    }
    changed
}

fn task903_materialize_final_country_table_full_object_with_encoded_tail_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let target = task903_hwpx_h_01_final_country_table_mut(core);
    *target = reference_table.clone();
    target.raw_table_record_extra = task903_table_record_tail_with_zone_count(reference_table);
    1
}

fn task903_hwpx_h_01_final_region_table(core: &DocumentCore) -> &Table {
    task903_hwpx_h_01_table_at(core, 102, 0)
}

fn task903_hwpx_h_01_final_region_table_mut(core: &mut DocumentCore) -> &mut Table {
    task903_hwpx_h_01_table_at_mut(core, 102, 0)
}

fn task903_materialize_final_region_table_record_with_encoded_tail_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_final_region_table_mut(core);
    task903_copy_table_record_payload_with_encoded_tail(table, reference_table);
    1
}

fn task903_materialize_final_region_table_full_object_with_encoded_tail_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let target = task903_hwpx_h_01_final_region_table_mut(core);
    *target = reference_table.clone();
    target.raw_table_record_extra = task903_table_record_tail_with_zone_count(reference_table);
    1
}

fn task903_hwpx_h_01_section1_reference_table(core: &DocumentCore) -> &Table {
    task903_hwpx_h_01_table_at_in_section(core, 1, 0, 0)
}

fn task903_hwpx_h_01_section1_reference_table_mut(core: &mut DocumentCore) -> &mut Table {
    task903_hwpx_h_01_table_at_in_section_mut(core, 1, 0, 0)
}

fn task903_materialize_section1_reference_table_record_with_encoded_tail_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_section1_reference_table_mut(core);
    task903_copy_table_record_payload_with_encoded_tail(table, reference_table);
    1
}

fn task903_materialize_section1_reference_table_full_object_with_encoded_tail_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let target = task903_hwpx_h_01_section1_reference_table_mut(core);
    *target = reference_table.clone();
    target.raw_table_record_extra = task903_table_record_tail_with_zone_count(reference_table);
    1
}

fn task903_copy_table_cell_paragraph_linesegs(target: &mut Table, reference: &Table) -> usize {
    assert_eq!(
        target.cells.len(),
        reference.cells.len(),
        "cell paragraph lineSeg 복사는 셀 수가 같아야 함"
    );

    let mut changed = 0usize;
    for (target_cell, reference_cell) in target.cells.iter_mut().zip(reference.cells.iter()) {
        let n = target_cell
            .paragraphs
            .len()
            .min(reference_cell.paragraphs.len());
        for idx in 0..n {
            target_cell.paragraphs[idx].line_segs =
                reference_cell.paragraphs[idx].line_segs.clone();
            changed += 1;
        }
    }
    changed
}

fn task903_copy_table_cell_paragraph_records(target: &mut Table, reference: &Table) -> usize {
    assert_eq!(
        target.cells.len(),
        reference.cells.len(),
        "cell paragraph record 복사는 셀 수가 같아야 함"
    );

    let mut changed = 0usize;
    for (target_cell, reference_cell) in target.cells.iter_mut().zip(reference.cells.iter()) {
        let n = target_cell
            .paragraphs
            .len()
            .min(reference_cell.paragraphs.len());
        for idx in 0..n {
            task903_copy_para_records_without_controls(
                &mut target_cell.paragraphs[idx],
                &reference_cell.paragraphs[idx],
            );
            changed += 1;
        }
    }
    changed
}

fn task903_materialize_industry_table_cell_paragraph_linesegs_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_industry_table_mut(core);
    task903_copy_table_cell_paragraph_linesegs(table, reference_table)
}

fn task903_materialize_industry_table_cell_paragraph_records_from_reference(
    core: &mut DocumentCore,
    reference_table: &Table,
) -> usize {
    let table = task903_hwpx_h_01_industry_table_mut(core);
    task903_copy_table_cell_paragraph_records(table, reference_table)
}

#[derive(Debug, Default)]
struct Task903Stage14Changes {
    bytes: usize,
    base_sections: usize,
    base_first_cells: usize,
    base_full_pictures: usize,
    base_first_table_ctrl_headers: usize,
    base_first_table_cell_list_headers: usize,
    base_first_table_records: usize,
    base_first_table_cell_para_headers: usize,
    base_next_table_child_headers: usize,
    chart_table_ctrl_headers: usize,
    chart_table_records: usize,
    chart_table_first_cell_headers: usize,
    chart_table_all_cell_headers: usize,
    chart_table_full_objects: usize,
}

fn task903_generate_stage14_probe_variant(
    output_name: &str,
    chart_table_ctrl_header: bool,
    chart_table_record: bool,
    chart_table_first_cell_headers: bool,
    chart_table_all_cell_headers: bool,
    chart_table_full_object: bool,
) -> Task903Stage14Changes {
    let bytes = load_sample("hwpx-h-01.hwpx");
    let mut core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");

    convert_if_hwpx_source(core.document_mut(), rhwp::parser::FileFormat::Hwpx);

    let reference = task903_reference_hwpx_h_01_core();
    let reference_first_table = task903_hwpx_h_01_first_table(&reference).clone();
    let reference_second_table = task903_hwpx_h_01_second_table(&reference).clone();
    let reference_chart_table = task903_hwpx_h_01_chart_table(&reference).clone();

    let mut changes = Task903Stage14Changes::default();

    // Stage 14 기준선: Stage 13의 03_next_table_child_headers를 재현한다.
    for section in &mut core.document_mut().sections {
        changes.base_sections += task903_materialize_stage8_section_def_core(section);
    }
    changes.base_first_cells += task903_materialize_first_cell_list_header_tail_65(&mut core);
    changes.base_full_pictures += task903_materialize_first_table_picture_patch_from_reference(
        &mut core,
        &reference_first_table,
        0,
        Task903FirstTablePicturePatch::Full,
    );
    changes.base_full_pictures += task903_materialize_first_table_picture_patch_from_reference(
        &mut core,
        &reference_first_table,
        1,
        Task903FirstTablePicturePatch::Full,
    );
    changes.base_first_table_ctrl_headers +=
        task903_materialize_first_table_ctrl_header_from_reference(
            &mut core,
            &reference_first_table,
        );
    changes.base_first_table_cell_list_headers +=
        task903_materialize_first_table_cell_list_headers_from_reference(
            &mut core,
            &reference_first_table,
        );
    changes.base_first_table_records +=
        task903_materialize_first_table_record_from_reference(&mut core, &reference_first_table);
    changes.base_first_table_cell_para_headers +=
        task903_materialize_first_table_cell_para_headers_from_reference(
            &mut core,
            &reference_first_table,
        );
    changes.base_next_table_child_headers +=
        task903_materialize_next_table_child_headers_from_reference(
            &mut core,
            &reference_second_table,
        );

    if chart_table_full_object {
        changes.chart_table_full_objects +=
            task903_materialize_chart_table_full_object_from_reference(
                &mut core,
                &reference_chart_table,
            );
    } else {
        if chart_table_ctrl_header {
            changes.chart_table_ctrl_headers +=
                task903_materialize_chart_table_ctrl_header_from_reference(
                    &mut core,
                    &reference_chart_table,
                );
        }
        if chart_table_record {
            changes.chart_table_records += task903_materialize_chart_table_record_from_reference(
                &mut core,
                &reference_chart_table,
            );
        }
        if chart_table_first_cell_headers {
            changes.chart_table_first_cell_headers +=
                task903_materialize_chart_table_first_cell_headers_from_reference(
                    &mut core,
                    &reference_chart_table,
                );
        }
        if chart_table_all_cell_headers {
            changes.chart_table_all_cell_headers +=
                task903_materialize_chart_table_all_cell_headers_from_reference(
                    &mut core,
                    &reference_chart_table,
                );
        }
    }

    let hwp_bytes = core.export_hwp_native().expect("Stage 14 HWP 직렬화 실패");
    let out_dir =
        std::path::Path::new("output/poc/hwpx2hwp/task903/stage14_chart_table_boundary_probe");
    std::fs::create_dir_all(out_dir).expect("Stage 14 output dir 생성 실패");
    let out_path = out_dir.join(output_name);
    std::fs::write(&out_path, &hwp_bytes).expect("Stage 14 probe 파일 저장 실패");

    changes.bytes = hwp_bytes.len();

    let reloaded = DocumentCore::from_bytes(&hwp_bytes).expect("Stage 14 probe HWP 재로드 실패");
    assert_eq!(reloaded.page_count(), 9, "{} page count", output_name);

    let first_table = task903_hwpx_h_01_first_table(&reloaded);
    assert_eq!(
        first_table.cells.len(),
        reference_first_table.cells.len(),
        "{} first table cell count",
        output_name
    );
    let second_table = task903_hwpx_h_01_second_table(&reloaded);
    assert_eq!(
        second_table.cells[0].raw_list_extra.len(),
        reference_second_table.cells[0].raw_list_extra.len(),
        "{} second table base LIST_HEADER extra length",
        output_name
    );

    let chart_table = task903_hwpx_h_01_chart_table(&reloaded);
    assert_eq!(
        chart_table.cells.len(),
        reference_chart_table.cells.len(),
        "{} chart table cell count",
        output_name
    );
    if chart_table_ctrl_header || chart_table_full_object {
        assert_eq!(
            chart_table.raw_ctrl_data, reference_chart_table.raw_ctrl_data,
            "{} chart table CTRL_HEADER payload",
            output_name
        );
    }
    if chart_table_record || chart_table_full_object {
        assert_eq!(
            chart_table.raw_table_record_extra.len(),
            reference_chart_table.raw_table_record_extra.len(),
            "{} chart table TABLE extra length",
            output_name
        );
    }
    if chart_table_first_cell_headers || chart_table_all_cell_headers || chart_table_full_object {
        assert_eq!(
            chart_table.cells[0].raw_list_extra.len(),
            reference_chart_table.cells[0].raw_list_extra.len(),
            "{} chart table first cell LIST_HEADER extra length",
            output_name
        );
        assert_eq!(
            chart_table.cells[0].paragraphs[0].raw_header_extra.len(),
            reference_chart_table.cells[0].paragraphs[0]
                .raw_header_extra
                .len(),
            "{} chart table first cell PARA_HEADER extra length",
            output_name
        );
    }

    eprintln!(
        "[#903 Stage 14] {}: bytes={}, base_sections={}, base_first_cells={}, base_full_pictures={}, base_first_table_ctrl_headers={}, base_first_table_cell_list_headers={}, base_first_table_records={}, base_first_table_cell_para_headers={}, base_next_table_child_headers={}, chart_table_ctrl_headers={}, chart_table_records={}, chart_table_first_cell_headers={}, chart_table_all_cell_headers={}, chart_table_full_objects={}, pages={}",
        out_path.display(),
        changes.bytes,
        changes.base_sections,
        changes.base_first_cells,
        changes.base_full_pictures,
        changes.base_first_table_ctrl_headers,
        changes.base_first_table_cell_list_headers,
        changes.base_first_table_records,
        changes.base_first_table_cell_para_headers,
        changes.base_next_table_child_headers,
        changes.chart_table_ctrl_headers,
        changes.chart_table_records,
        changes.chart_table_first_cell_headers,
        changes.chart_table_all_cell_headers,
        changes.chart_table_full_objects,
        reloaded.page_count()
    );

    changes
}

#[derive(Debug, Default)]
struct Task903Stage16Changes {
    bytes: usize,
    base_sections: usize,
    base_first_cells: usize,
    base_full_pictures: usize,
    base_first_table_ctrl_headers: usize,
    base_first_table_cell_list_headers: usize,
    base_first_table_records: usize,
    base_first_table_cell_para_headers: usize,
    base_next_table_child_headers: usize,
    chart_host_para_records: usize,
    chart_table_ctrl_headers: usize,
    chart_table_records_with_tail: usize,
    chart_table_all_cell_headers: usize,
    chart_table_full_objects_with_tail: usize,
    following_title_records: usize,
}

fn task903_generate_stage16_probe_variant(
    output_name: &str,
    chart_host_para_records: bool,
    chart_table_ctrl_header: bool,
    chart_table_record_with_tail: bool,
    chart_table_all_cell_headers: bool,
    chart_table_full_object_with_tail: bool,
    following_title_records: bool,
) -> Task903Stage16Changes {
    let bytes = load_sample("hwpx-h-01.hwpx");
    let mut core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");

    convert_if_hwpx_source(core.document_mut(), rhwp::parser::FileFormat::Hwpx);

    let reference = task903_reference_hwpx_h_01_core();
    let reference_first_table = task903_hwpx_h_01_first_table(&reference).clone();
    let reference_second_table = task903_hwpx_h_01_second_table(&reference).clone();
    let reference_chart_table = task903_hwpx_h_01_chart_table(&reference).clone();

    let mut changes = Task903Stage16Changes::default();

    // Stage 16 기준선: Stage 13의 03_next_table_child_headers를 재현한다.
    for section in &mut core.document_mut().sections {
        changes.base_sections += task903_materialize_stage8_section_def_core(section);
    }
    changes.base_first_cells += task903_materialize_first_cell_list_header_tail_65(&mut core);
    changes.base_full_pictures += task903_materialize_first_table_picture_patch_from_reference(
        &mut core,
        &reference_first_table,
        0,
        Task903FirstTablePicturePatch::Full,
    );
    changes.base_full_pictures += task903_materialize_first_table_picture_patch_from_reference(
        &mut core,
        &reference_first_table,
        1,
        Task903FirstTablePicturePatch::Full,
    );
    changes.base_first_table_ctrl_headers +=
        task903_materialize_first_table_ctrl_header_from_reference(
            &mut core,
            &reference_first_table,
        );
    changes.base_first_table_cell_list_headers +=
        task903_materialize_first_table_cell_list_headers_from_reference(
            &mut core,
            &reference_first_table,
        );
    changes.base_first_table_records +=
        task903_materialize_first_table_record_from_reference(&mut core, &reference_first_table);
    changes.base_first_table_cell_para_headers +=
        task903_materialize_first_table_cell_para_headers_from_reference(
            &mut core,
            &reference_first_table,
        );
    changes.base_next_table_child_headers +=
        task903_materialize_next_table_child_headers_from_reference(
            &mut core,
            &reference_second_table,
        );

    if chart_host_para_records {
        changes.chart_host_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 10);
    }

    if chart_table_full_object_with_tail {
        changes.chart_table_full_objects_with_tail +=
            task903_materialize_chart_table_full_object_with_encoded_tail_from_reference(
                &mut core,
                &reference_chart_table,
            );
    } else {
        if chart_table_ctrl_header {
            changes.chart_table_ctrl_headers +=
                task903_materialize_chart_table_ctrl_header_from_reference(
                    &mut core,
                    &reference_chart_table,
                );
        }
        if chart_table_record_with_tail {
            changes.chart_table_records_with_tail +=
                task903_materialize_chart_table_record_with_encoded_tail_from_reference(
                    &mut core,
                    &reference_chart_table,
                );
        }
        if chart_table_all_cell_headers {
            changes.chart_table_all_cell_headers +=
                task903_materialize_chart_table_all_cell_headers_from_reference(
                    &mut core,
                    &reference_chart_table,
                );
        }
    }

    if following_title_records {
        changes.following_title_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 13);
    }

    let hwp_bytes = core.export_hwp_native().expect("Stage 16 HWP 직렬화 실패");
    let out_dir = std::path::Path::new("output/poc/hwpx2hwp/task903/stage16_chart_tuple_probe");
    std::fs::create_dir_all(out_dir).expect("Stage 16 output dir 생성 실패");
    let out_path = out_dir.join(output_name);
    std::fs::write(&out_path, &hwp_bytes).expect("Stage 16 probe 파일 저장 실패");

    changes.bytes = hwp_bytes.len();

    let reloaded = DocumentCore::from_bytes(&hwp_bytes).expect("Stage 16 probe HWP 재로드 실패");
    assert_eq!(reloaded.page_count(), 9, "{} page count", output_name);

    let first_table = task903_hwpx_h_01_first_table(&reloaded);
    assert_eq!(
        first_table.cells.len(),
        reference_first_table.cells.len(),
        "{} first table cell count",
        output_name
    );
    let second_table = task903_hwpx_h_01_second_table(&reloaded);
    assert_eq!(
        second_table.cells[0].raw_list_extra.len(),
        reference_second_table.cells[0].raw_list_extra.len(),
        "{} second table base LIST_HEADER extra length",
        output_name
    );

    let chart_table = task903_hwpx_h_01_chart_table(&reloaded);
    assert_eq!(
        chart_table.cells.len(),
        reference_chart_table.cells.len(),
        "{} chart table cell count",
        output_name
    );
    if chart_table_ctrl_header || chart_table_full_object_with_tail {
        assert_eq!(
            chart_table.raw_ctrl_data, reference_chart_table.raw_ctrl_data,
            "{} chart table CTRL_HEADER payload",
            output_name
        );
    }
    if chart_table_record_with_tail || chart_table_full_object_with_tail {
        assert_eq!(
            chart_table.raw_table_record_attr, reference_chart_table.raw_table_record_attr,
            "{} chart table TABLE attr",
            output_name
        );
        assert_eq!(
            chart_table.zones.len(),
            reference_chart_table.zones.len(),
            "{} chart table zone count",
            output_name
        );
    }
    if chart_table_all_cell_headers || chart_table_full_object_with_tail {
        assert_eq!(
            chart_table.cells[0].raw_list_extra.len(),
            reference_chart_table.cells[0].raw_list_extra.len(),
            "{} chart table first cell LIST_HEADER extra length",
            output_name
        );
        assert_eq!(
            chart_table.cells[0].paragraphs[0].raw_header_extra.len(),
            reference_chart_table.cells[0].paragraphs[0]
                .raw_header_extra
                .len(),
            "{} chart table first cell PARA_HEADER extra length",
            output_name
        );
    }
    if following_title_records {
        assert_eq!(
            reloaded.document().sections[0].paragraphs[13].text,
            reference.document().sections[0].paragraphs[13].text,
            "{} following title text",
            output_name
        );
    }

    eprintln!(
        "[#903 Stage 16] {}: bytes={}, base_sections={}, base_first_cells={}, base_full_pictures={}, base_first_table_ctrl_headers={}, base_first_table_cell_list_headers={}, base_first_table_records={}, base_first_table_cell_para_headers={}, base_next_table_child_headers={}, chart_host_para_records={}, chart_table_ctrl_headers={}, chart_table_records_with_tail={}, chart_table_all_cell_headers={}, chart_table_full_objects_with_tail={}, following_title_records={}, pages={}",
        out_path.display(),
        changes.bytes,
        changes.base_sections,
        changes.base_first_cells,
        changes.base_full_pictures,
        changes.base_first_table_ctrl_headers,
        changes.base_first_table_cell_list_headers,
        changes.base_first_table_records,
        changes.base_first_table_cell_para_headers,
        changes.base_next_table_child_headers,
        changes.chart_host_para_records,
        changes.chart_table_ctrl_headers,
        changes.chart_table_records_with_tail,
        changes.chart_table_all_cell_headers,
        changes.chart_table_full_objects_with_tail,
        changes.following_title_records,
        reloaded.page_count()
    );

    changes
}

#[derive(Debug, Default)]
struct Task903Stage17Changes {
    bytes: usize,
    base_sections: usize,
    base_first_cells: usize,
    base_full_pictures: usize,
    base_first_table_ctrl_headers: usize,
    base_first_table_cell_list_headers: usize,
    base_first_table_records: usize,
    base_first_table_cell_para_headers: usize,
    base_next_table_child_headers: usize,
    base_chart_host_para_records: usize,
    base_chart_table_full_objects_with_tail: usize,
    base_following_title_records: usize,
    industry_host_para_records: usize,
    industry_table_ctrl_headers: usize,
    industry_table_records_with_tail: usize,
    industry_table_all_cell_headers: usize,
    industry_table_full_objects_with_tail: usize,
    next_boundary_para_records: usize,
}

fn task903_generate_stage17_probe_variant(
    output_name: &str,
    industry_host_para_records: bool,
    industry_table_ctrl_header: bool,
    industry_table_record_with_tail: bool,
    industry_table_all_cell_headers: bool,
    industry_table_full_object_with_tail: bool,
    next_boundary_para_records: bool,
) -> Task903Stage17Changes {
    let bytes = load_sample("hwpx-h-01.hwpx");
    let mut core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");

    convert_if_hwpx_source(core.document_mut(), rhwp::parser::FileFormat::Hwpx);

    let reference = task903_reference_hwpx_h_01_core();
    let reference_first_table = task903_hwpx_h_01_first_table(&reference).clone();
    let reference_second_table = task903_hwpx_h_01_second_table(&reference).clone();
    let reference_chart_table = task903_hwpx_h_01_chart_table(&reference).clone();
    let reference_industry_table = task903_hwpx_h_01_industry_table(&reference).clone();

    let mut changes = Task903Stage17Changes::default();

    // Stage 17 기준선: Stage 16의 06_chart_tuple_plus_following_title_text를 재현한다.
    for section in &mut core.document_mut().sections {
        changes.base_sections += task903_materialize_stage8_section_def_core(section);
    }
    changes.base_first_cells += task903_materialize_first_cell_list_header_tail_65(&mut core);
    changes.base_full_pictures += task903_materialize_first_table_picture_patch_from_reference(
        &mut core,
        &reference_first_table,
        0,
        Task903FirstTablePicturePatch::Full,
    );
    changes.base_full_pictures += task903_materialize_first_table_picture_patch_from_reference(
        &mut core,
        &reference_first_table,
        1,
        Task903FirstTablePicturePatch::Full,
    );
    changes.base_first_table_ctrl_headers +=
        task903_materialize_first_table_ctrl_header_from_reference(
            &mut core,
            &reference_first_table,
        );
    changes.base_first_table_cell_list_headers +=
        task903_materialize_first_table_cell_list_headers_from_reference(
            &mut core,
            &reference_first_table,
        );
    changes.base_first_table_records +=
        task903_materialize_first_table_record_from_reference(&mut core, &reference_first_table);
    changes.base_first_table_cell_para_headers +=
        task903_materialize_first_table_cell_para_headers_from_reference(
            &mut core,
            &reference_first_table,
        );
    changes.base_next_table_child_headers +=
        task903_materialize_next_table_child_headers_from_reference(
            &mut core,
            &reference_second_table,
        );
    changes.base_chart_host_para_records +=
        task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 10);
    changes.base_chart_table_full_objects_with_tail +=
        task903_materialize_chart_table_full_object_with_encoded_tail_from_reference(
            &mut core,
            &reference_chart_table,
        );
    changes.base_following_title_records +=
        task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 13);

    if industry_host_para_records {
        changes.industry_host_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 14);
    }

    if industry_table_full_object_with_tail {
        changes.industry_table_full_objects_with_tail +=
            task903_materialize_industry_table_full_object_with_encoded_tail_from_reference(
                &mut core,
                &reference_industry_table,
            );
    } else {
        if industry_table_ctrl_header {
            changes.industry_table_ctrl_headers +=
                task903_materialize_industry_table_ctrl_header_from_reference(
                    &mut core,
                    &reference_industry_table,
                );
        }
        if industry_table_record_with_tail {
            changes.industry_table_records_with_tail +=
                task903_materialize_industry_table_record_with_encoded_tail_from_reference(
                    &mut core,
                    &reference_industry_table,
                );
        }
        if industry_table_all_cell_headers {
            changes.industry_table_all_cell_headers +=
                task903_materialize_industry_table_all_cell_headers_from_reference(
                    &mut core,
                    &reference_industry_table,
                );
        }
    }

    if next_boundary_para_records {
        changes.next_boundary_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 15);
    }

    let hwp_bytes = core.export_hwp_native().expect("Stage 17 HWP 직렬화 실패");
    let out_dir = std::path::Path::new("output/poc/hwpx2hwp/task903/stage17_industry_table_probe");
    std::fs::create_dir_all(out_dir).expect("Stage 17 output dir 생성 실패");
    let out_path = out_dir.join(output_name);
    std::fs::write(&out_path, &hwp_bytes).expect("Stage 17 probe 파일 저장 실패");

    changes.bytes = hwp_bytes.len();

    let reloaded = DocumentCore::from_bytes(&hwp_bytes).expect("Stage 17 probe HWP 재로드 실패");
    assert_eq!(reloaded.page_count(), 9, "{} page count", output_name);
    assert_eq!(
        reloaded.document().sections[0].paragraphs[13].text,
        reference.document().sections[0].paragraphs[13].text,
        "{} following title text",
        output_name
    );

    let industry_table = task903_hwpx_h_01_industry_table(&reloaded);
    assert_eq!(
        industry_table.cells.len(),
        reference_industry_table.cells.len(),
        "{} industry table cell count",
        output_name
    );
    if industry_table_ctrl_header || industry_table_full_object_with_tail {
        assert_eq!(
            industry_table.raw_ctrl_data, reference_industry_table.raw_ctrl_data,
            "{} industry table CTRL_HEADER payload",
            output_name
        );
    }
    if industry_table_record_with_tail || industry_table_full_object_with_tail {
        assert_eq!(
            industry_table.raw_table_record_attr, reference_industry_table.raw_table_record_attr,
            "{} industry table TABLE attr",
            output_name
        );
        assert_eq!(
            industry_table.zones.len(),
            reference_industry_table.zones.len(),
            "{} industry table zone count",
            output_name
        );
    }
    if industry_table_all_cell_headers || industry_table_full_object_with_tail {
        assert_eq!(
            industry_table.cells[0].raw_list_extra.len(),
            reference_industry_table.cells[0].raw_list_extra.len(),
            "{} industry table first cell LIST_HEADER extra length",
            output_name
        );
        assert_eq!(
            industry_table.cells[0].paragraphs[0].raw_header_extra.len(),
            reference_industry_table.cells[0].paragraphs[0]
                .raw_header_extra
                .len(),
            "{} industry table first cell PARA_HEADER extra length",
            output_name
        );
    }

    eprintln!(
        "[#903 Stage 17] {}: bytes={}, base_sections={}, base_first_cells={}, base_full_pictures={}, base_first_table_ctrl_headers={}, base_first_table_cell_list_headers={}, base_first_table_records={}, base_first_table_cell_para_headers={}, base_next_table_child_headers={}, base_chart_host_para_records={}, base_chart_table_full_objects_with_tail={}, base_following_title_records={}, industry_host_para_records={}, industry_table_ctrl_headers={}, industry_table_records_with_tail={}, industry_table_all_cell_headers={}, industry_table_full_objects_with_tail={}, next_boundary_para_records={}, pages={}",
        out_path.display(),
        changes.bytes,
        changes.base_sections,
        changes.base_first_cells,
        changes.base_full_pictures,
        changes.base_first_table_ctrl_headers,
        changes.base_first_table_cell_list_headers,
        changes.base_first_table_records,
        changes.base_first_table_cell_para_headers,
        changes.base_next_table_child_headers,
        changes.base_chart_host_para_records,
        changes.base_chart_table_full_objects_with_tail,
        changes.base_following_title_records,
        changes.industry_host_para_records,
        changes.industry_table_ctrl_headers,
        changes.industry_table_records_with_tail,
        changes.industry_table_all_cell_headers,
        changes.industry_table_full_objects_with_tail,
        changes.next_boundary_para_records,
        reloaded.page_count()
    );

    changes
}

#[derive(Debug, Default)]
struct Task903Stage18Changes {
    bytes: usize,
    base_sections: usize,
    base_first_cells: usize,
    base_full_pictures: usize,
    base_first_table_ctrl_headers: usize,
    base_first_table_cell_list_headers: usize,
    base_first_table_records: usize,
    base_first_table_cell_para_headers: usize,
    base_next_table_child_headers: usize,
    base_chart_host_para_records: usize,
    base_chart_table_full_objects_with_tail: usize,
    base_following_title_records: usize,
    industry_host_para_records: usize,
    industry_table_records_with_tail: usize,
    industry_table_all_cell_headers: usize,
    industry_table_full_objects_with_tail: usize,
    industry_next_boundary_para_records: usize,
    industry_cell_linesegs: usize,
    industry_cell_para_records: usize,
    country_title_para_records: usize,
    country_host_para_records: usize,
    country_table_ctrl_headers: usize,
    country_table_records_with_tail: usize,
    country_table_all_cell_headers: usize,
    country_table_full_objects_with_tail: usize,
}

#[allow(clippy::too_many_arguments)]
fn task903_generate_stage18_probe_variant(
    output_name: &str,
    industry_host_para_records: bool,
    industry_table_record_with_tail: bool,
    industry_table_all_cell_headers: bool,
    industry_table_full_object_with_tail: bool,
    industry_next_boundary_para_records: bool,
    industry_cell_linesegs: bool,
    industry_cell_para_records: bool,
    country_title_para_records: bool,
    country_host_para_records: bool,
    country_table_ctrl_header: bool,
    country_table_record_with_tail: bool,
    country_table_all_cell_headers: bool,
    country_table_full_object_with_tail: bool,
) -> Task903Stage18Changes {
    let bytes = load_sample("hwpx-h-01.hwpx");
    let mut core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");

    convert_if_hwpx_source(core.document_mut(), rhwp::parser::FileFormat::Hwpx);

    let reference = task903_reference_hwpx_h_01_core();
    let reference_first_table = task903_hwpx_h_01_first_table(&reference).clone();
    let reference_second_table = task903_hwpx_h_01_second_table(&reference).clone();
    let reference_chart_table = task903_hwpx_h_01_chart_table(&reference).clone();
    let reference_industry_table = task903_hwpx_h_01_industry_table(&reference).clone();
    let reference_country_table = task903_hwpx_h_01_country_table(&reference).clone();

    let mut changes = Task903Stage18Changes::default();

    // Stage 18 공통 기준선: Stage 16의 06_chart_tuple_plus_following_title_text를 재현한다.
    for section in &mut core.document_mut().sections {
        changes.base_sections += task903_materialize_stage8_section_def_core(section);
    }
    changes.base_first_cells += task903_materialize_first_cell_list_header_tail_65(&mut core);
    changes.base_full_pictures += task903_materialize_first_table_picture_patch_from_reference(
        &mut core,
        &reference_first_table,
        0,
        Task903FirstTablePicturePatch::Full,
    );
    changes.base_full_pictures += task903_materialize_first_table_picture_patch_from_reference(
        &mut core,
        &reference_first_table,
        1,
        Task903FirstTablePicturePatch::Full,
    );
    changes.base_first_table_ctrl_headers +=
        task903_materialize_first_table_ctrl_header_from_reference(
            &mut core,
            &reference_first_table,
        );
    changes.base_first_table_cell_list_headers +=
        task903_materialize_first_table_cell_list_headers_from_reference(
            &mut core,
            &reference_first_table,
        );
    changes.base_first_table_records +=
        task903_materialize_first_table_record_from_reference(&mut core, &reference_first_table);
    changes.base_first_table_cell_para_headers +=
        task903_materialize_first_table_cell_para_headers_from_reference(
            &mut core,
            &reference_first_table,
        );
    changes.base_next_table_child_headers +=
        task903_materialize_next_table_child_headers_from_reference(
            &mut core,
            &reference_second_table,
        );
    changes.base_chart_host_para_records +=
        task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 10);
    changes.base_chart_table_full_objects_with_tail +=
        task903_materialize_chart_table_full_object_with_encoded_tail_from_reference(
            &mut core,
            &reference_chart_table,
        );
    changes.base_following_title_records +=
        task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 13);

    if industry_host_para_records {
        changes.industry_host_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 14);
    }

    if industry_table_full_object_with_tail {
        changes.industry_table_full_objects_with_tail +=
            task903_materialize_industry_table_full_object_with_encoded_tail_from_reference(
                &mut core,
                &reference_industry_table,
            );
    } else {
        if industry_table_record_with_tail {
            changes.industry_table_records_with_tail +=
                task903_materialize_industry_table_record_with_encoded_tail_from_reference(
                    &mut core,
                    &reference_industry_table,
                );
        }
        if industry_table_all_cell_headers {
            changes.industry_table_all_cell_headers +=
                task903_materialize_industry_table_all_cell_headers_from_reference(
                    &mut core,
                    &reference_industry_table,
                );
        }
    }

    if industry_next_boundary_para_records {
        changes.industry_next_boundary_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 15);
    }
    if industry_cell_linesegs {
        changes.industry_cell_linesegs +=
            task903_materialize_industry_table_cell_paragraph_linesegs_from_reference(
                &mut core,
                &reference_industry_table,
            );
    }
    if industry_cell_para_records {
        changes.industry_cell_para_records +=
            task903_materialize_industry_table_cell_paragraph_records_from_reference(
                &mut core,
                &reference_industry_table,
            );
    }

    if country_title_para_records {
        changes.country_title_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 20);
    }
    if country_host_para_records {
        changes.country_host_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 21);
    }

    if country_table_full_object_with_tail {
        changes.country_table_full_objects_with_tail +=
            task903_materialize_country_table_full_object_with_encoded_tail_from_reference(
                &mut core,
                &reference_country_table,
            );
    } else {
        if country_table_ctrl_header {
            changes.country_table_ctrl_headers +=
                task903_materialize_country_table_ctrl_header_from_reference(
                    &mut core,
                    &reference_country_table,
                );
        }
        if country_table_record_with_tail {
            changes.country_table_records_with_tail +=
                task903_materialize_country_table_record_with_encoded_tail_from_reference(
                    &mut core,
                    &reference_country_table,
                );
        }
        if country_table_all_cell_headers {
            changes.country_table_all_cell_headers +=
                task903_materialize_country_table_all_cell_headers_from_reference(
                    &mut core,
                    &reference_country_table,
                );
        }
    }

    let hwp_bytes = core.export_hwp_native().expect("Stage 18 HWP 직렬화 실패");
    let out_dir = std::path::Path::new("output/poc/hwpx2hwp/task903/stage18_country_reflow_probe");
    std::fs::create_dir_all(out_dir).expect("Stage 18 output dir 생성 실패");
    let out_path = out_dir.join(output_name);
    std::fs::write(&out_path, &hwp_bytes).expect("Stage 18 probe 파일 저장 실패");

    changes.bytes = hwp_bytes.len();

    let reloaded = DocumentCore::from_bytes(&hwp_bytes).expect("Stage 18 probe HWP 재로드 실패");
    assert_eq!(reloaded.page_count(), 9, "{} page count", output_name);
    assert_eq!(
        reloaded.document().sections[0].paragraphs[13].text,
        reference.document().sections[0].paragraphs[13].text,
        "{} following title text",
        output_name
    );

    let industry_table = task903_hwpx_h_01_industry_table(&reloaded);
    assert_eq!(
        industry_table.cells.len(),
        reference_industry_table.cells.len(),
        "{} industry table cell count",
        output_name
    );
    let country_table = task903_hwpx_h_01_country_table(&reloaded);
    assert_eq!(
        country_table.cells.len(),
        reference_country_table.cells.len(),
        "{} country table cell count",
        output_name
    );

    if country_title_para_records {
        assert_eq!(
            reloaded.document().sections[0].paragraphs[20].text,
            reference.document().sections[0].paragraphs[20].text,
            "{} country title text",
            output_name
        );
    }
    if country_table_ctrl_header || country_table_full_object_with_tail {
        assert_eq!(
            country_table.raw_ctrl_data, reference_country_table.raw_ctrl_data,
            "{} country table CTRL_HEADER payload",
            output_name
        );
    }
    if country_table_record_with_tail || country_table_full_object_with_tail {
        assert_eq!(
            country_table.raw_table_record_attr, reference_country_table.raw_table_record_attr,
            "{} country table TABLE attr",
            output_name
        );
    }
    if country_table_all_cell_headers || country_table_full_object_with_tail {
        assert_eq!(
            country_table.cells[0].raw_list_extra.len(),
            reference_country_table.cells[0].raw_list_extra.len(),
            "{} country table first cell LIST_HEADER extra length",
            output_name
        );
        assert_eq!(
            country_table.cells[0].paragraphs[0].raw_header_extra.len(),
            reference_country_table.cells[0].paragraphs[0]
                .raw_header_extra
                .len(),
            "{} country table first cell PARA_HEADER extra length",
            output_name
        );
    }
    if industry_cell_linesegs || industry_cell_para_records {
        let actual_seg = &industry_table.cells[0].paragraphs[0].line_segs[0];
        let reference_seg = &reference_industry_table.cells[0].paragraphs[0].line_segs[0];
        assert_eq!(
            industry_table.cells[0].paragraphs[0].line_segs.len(),
            reference_industry_table.cells[0].paragraphs[0]
                .line_segs
                .len(),
            "{} industry first cell lineSeg count",
            output_name
        );
        assert_eq!(
            actual_seg.line_height, reference_seg.line_height,
            "{} industry first cell lineSeg line_height",
            output_name
        );
        assert_eq!(
            actual_seg.baseline_distance, reference_seg.baseline_distance,
            "{} industry first cell lineSeg baseline",
            output_name
        );
    }

    eprintln!(
        "[#903 Stage 18] {}: bytes={}, base_sections={}, base_first_cells={}, base_full_pictures={}, base_first_table_ctrl_headers={}, base_first_table_cell_list_headers={}, base_first_table_records={}, base_first_table_cell_para_headers={}, base_next_table_child_headers={}, base_chart_host_para_records={}, base_chart_table_full_objects_with_tail={}, base_following_title_records={}, industry_host_para_records={}, industry_table_records_with_tail={}, industry_table_all_cell_headers={}, industry_table_full_objects_with_tail={}, industry_next_boundary_para_records={}, industry_cell_linesegs={}, industry_cell_para_records={}, country_title_para_records={}, country_host_para_records={}, country_table_ctrl_headers={}, country_table_records_with_tail={}, country_table_all_cell_headers={}, country_table_full_objects_with_tail={}, pages={}",
        out_path.display(),
        changes.bytes,
        changes.base_sections,
        changes.base_first_cells,
        changes.base_full_pictures,
        changes.base_first_table_ctrl_headers,
        changes.base_first_table_cell_list_headers,
        changes.base_first_table_records,
        changes.base_first_table_cell_para_headers,
        changes.base_next_table_child_headers,
        changes.base_chart_host_para_records,
        changes.base_chart_table_full_objects_with_tail,
        changes.base_following_title_records,
        changes.industry_host_para_records,
        changes.industry_table_records_with_tail,
        changes.industry_table_all_cell_headers,
        changes.industry_table_full_objects_with_tail,
        changes.industry_next_boundary_para_records,
        changes.industry_cell_linesegs,
        changes.industry_cell_para_records,
        changes.country_title_para_records,
        changes.country_host_para_records,
        changes.country_table_ctrl_headers,
        changes.country_table_records_with_tail,
        changes.country_table_all_cell_headers,
        changes.country_table_full_objects_with_tail,
        reloaded.page_count()
    );

    changes
}

#[derive(Debug, Default)]
struct Task903Stage19Changes {
    bytes: usize,
    region_title_para_records: usize,
    region_host_para_records: usize,
    region_table_ctrl_headers: usize,
    region_table_records_with_tail: usize,
    region_table_all_cell_headers: usize,
    region_table_full_objects_with_tail: usize,
    following_empty_para_records: usize,
    following_text_para_records: usize,
}

#[allow(clippy::too_many_arguments)]
fn task903_generate_stage19_probe_variant(
    output_name: &str,
    region_title_para_records: bool,
    region_host_para_records: bool,
    region_table_ctrl_header: bool,
    region_table_record_with_tail: bool,
    region_table_all_cell_headers: bool,
    region_table_full_object_with_tail: bool,
    following_empty_para_records: bool,
    following_text_para_records: bool,
) -> Task903Stage19Changes {
    let bytes = load_sample("hwpx-h-01.hwpx");
    let mut core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");

    convert_if_hwpx_source(core.document_mut(), rhwp::parser::FileFormat::Hwpx);

    let reference = task903_reference_hwpx_h_01_core();
    let reference_first_table = task903_hwpx_h_01_first_table(&reference).clone();
    let reference_second_table = task903_hwpx_h_01_second_table(&reference).clone();
    let reference_chart_table = task903_hwpx_h_01_chart_table(&reference).clone();
    let reference_industry_table = task903_hwpx_h_01_industry_table(&reference).clone();
    let reference_country_table = task903_hwpx_h_01_country_table(&reference).clone();
    let reference_region_table = task903_hwpx_h_01_region_table(&reference).clone();

    let mut changes = Task903Stage19Changes::default();

    // Stage 19 공통 기준선: Stage 18의 06_country_host_para_plus_table_full_tuple을 재현한다.
    for section in &mut core.document_mut().sections {
        task903_materialize_stage8_section_def_core(section);
    }
    task903_materialize_first_cell_list_header_tail_65(&mut core);
    task903_materialize_first_table_picture_patch_from_reference(
        &mut core,
        &reference_first_table,
        0,
        Task903FirstTablePicturePatch::Full,
    );
    task903_materialize_first_table_picture_patch_from_reference(
        &mut core,
        &reference_first_table,
        1,
        Task903FirstTablePicturePatch::Full,
    );
    task903_materialize_first_table_ctrl_header_from_reference(&mut core, &reference_first_table);
    task903_materialize_first_table_cell_list_headers_from_reference(
        &mut core,
        &reference_first_table,
    );
    task903_materialize_first_table_record_from_reference(&mut core, &reference_first_table);
    task903_materialize_first_table_cell_para_headers_from_reference(
        &mut core,
        &reference_first_table,
    );
    task903_materialize_next_table_child_headers_from_reference(&mut core, &reference_second_table);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 10);
    task903_materialize_chart_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_chart_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 13);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 14);
    task903_materialize_industry_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_industry_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 15);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 20);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 21);
    task903_materialize_country_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_country_table,
    );

    if region_title_para_records {
        changes.region_title_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 22);
    }
    if region_host_para_records {
        changes.region_host_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 23);
    }

    if region_table_full_object_with_tail {
        changes.region_table_full_objects_with_tail +=
            task903_materialize_region_table_full_object_with_encoded_tail_from_reference(
                &mut core,
                &reference_region_table,
            );
    } else {
        if region_table_ctrl_header {
            changes.region_table_ctrl_headers +=
                task903_materialize_region_table_ctrl_header_from_reference(
                    &mut core,
                    &reference_region_table,
                );
        }
        if region_table_record_with_tail {
            changes.region_table_records_with_tail +=
                task903_materialize_region_table_record_with_encoded_tail_from_reference(
                    &mut core,
                    &reference_region_table,
                );
        }
        if region_table_all_cell_headers {
            changes.region_table_all_cell_headers +=
                task903_materialize_region_table_all_cell_headers_from_reference(
                    &mut core,
                    &reference_region_table,
                );
        }
    }

    if following_empty_para_records {
        changes.following_empty_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 24);
    }
    if following_text_para_records {
        changes.following_text_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 25);
    }

    let hwp_bytes = core.export_hwp_native().expect("Stage 19 HWP 직렬화 실패");
    let out_dir = std::path::Path::new("output/poc/hwpx2hwp/task903/stage19_region_boundary_probe");
    std::fs::create_dir_all(out_dir).expect("Stage 19 output dir 생성 실패");
    let out_path = out_dir.join(output_name);
    std::fs::write(&out_path, &hwp_bytes).expect("Stage 19 probe 파일 저장 실패");

    changes.bytes = hwp_bytes.len();

    let reloaded = DocumentCore::from_bytes(&hwp_bytes).expect("Stage 19 probe HWP 재로드 실패");
    assert_eq!(reloaded.page_count(), 9, "{} page count", output_name);
    assert_eq!(
        reloaded.document().sections[0].paragraphs[20].text,
        reference.document().sections[0].paragraphs[20].text,
        "{} country title text",
        output_name
    );

    let region_table = task903_hwpx_h_01_region_table(&reloaded);
    assert_eq!(
        region_table.cells.len(),
        reference_region_table.cells.len(),
        "{} region table cell count",
        output_name
    );

    if region_title_para_records {
        assert_eq!(
            reloaded.document().sections[0].paragraphs[22].text,
            reference.document().sections[0].paragraphs[22].text,
            "{} region title text",
            output_name
        );
    }
    if region_table_ctrl_header || region_table_full_object_with_tail {
        assert_eq!(
            region_table.raw_ctrl_data, reference_region_table.raw_ctrl_data,
            "{} region table CTRL_HEADER payload",
            output_name
        );
    }
    if region_table_record_with_tail || region_table_full_object_with_tail {
        assert_eq!(
            region_table.raw_table_record_attr, reference_region_table.raw_table_record_attr,
            "{} region table TABLE attr",
            output_name
        );
    }
    if region_table_all_cell_headers || region_table_full_object_with_tail {
        assert_eq!(
            region_table.cells[0].raw_list_extra.len(),
            reference_region_table.cells[0].raw_list_extra.len(),
            "{} region table first cell LIST_HEADER extra length",
            output_name
        );
        assert_eq!(
            region_table.cells[0].paragraphs[0].raw_header_extra.len(),
            reference_region_table.cells[0].paragraphs[0]
                .raw_header_extra
                .len(),
            "{} region table first cell PARA_HEADER extra length",
            output_name
        );
    }
    if following_empty_para_records {
        assert_eq!(
            reloaded.document().sections[0].paragraphs[24].text,
            reference.document().sections[0].paragraphs[24].text,
            "{} following empty paragraph text",
            output_name
        );
    }
    if following_text_para_records {
        assert_eq!(
            reloaded.document().sections[0].paragraphs[25].text,
            reference.document().sections[0].paragraphs[25].text,
            "{} following body paragraph text",
            output_name
        );
    }

    eprintln!(
        "[#903 Stage 19] {}: bytes={}, region_title_para_records={}, region_host_para_records={}, region_table_ctrl_headers={}, region_table_records_with_tail={}, region_table_all_cell_headers={}, region_table_full_objects_with_tail={}, following_empty_para_records={}, following_text_para_records={}, pages={}",
        out_path.display(),
        changes.bytes,
        changes.region_title_para_records,
        changes.region_host_para_records,
        changes.region_table_ctrl_headers,
        changes.region_table_records_with_tail,
        changes.region_table_all_cell_headers,
        changes.region_table_full_objects_with_tail,
        changes.following_empty_para_records,
        changes.following_text_para_records,
        reloaded.page_count()
    );

    changes
}

#[derive(Debug, Default)]
struct Task903Stage20Changes {
    bytes: usize,
    pre_notice_empty_para_records: usize,
    notice_para_records: usize,
    notice_table_ctrl_headers: usize,
    notice_table_records_with_tail: usize,
    notice_table_all_cell_headers: usize,
    notice_table_full_objects_with_tail: usize,
    logo_group_para_records: usize,
}

#[allow(clippy::too_many_arguments)]
fn task903_generate_stage20_probe_variant(
    output_name: &str,
    pre_notice_empty_para_records: bool,
    notice_para_records: bool,
    notice_table_ctrl_header: bool,
    notice_table_record_with_tail: bool,
    notice_table_all_cell_headers: bool,
    notice_table_full_object_with_tail: bool,
    logo_group_para_records: bool,
) -> Task903Stage20Changes {
    let bytes = load_sample("hwpx-h-01.hwpx");
    let mut core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");

    convert_if_hwpx_source(core.document_mut(), rhwp::parser::FileFormat::Hwpx);

    let reference = task903_reference_hwpx_h_01_core();
    let reference_first_table = task903_hwpx_h_01_first_table(&reference).clone();
    let reference_second_table = task903_hwpx_h_01_second_table(&reference).clone();
    let reference_chart_table = task903_hwpx_h_01_chart_table(&reference).clone();
    let reference_industry_table = task903_hwpx_h_01_industry_table(&reference).clone();
    let reference_country_table = task903_hwpx_h_01_country_table(&reference).clone();
    let reference_region_table = task903_hwpx_h_01_region_table(&reference).clone();
    let reference_notice_table = task903_hwpx_h_01_notice_table(&reference).clone();

    let mut changes = Task903Stage20Changes::default();

    // Stage 20 공통 기준선: Stage 19의 08_region_full_tuple_plus_following_text_para를 재현한다.
    for section in &mut core.document_mut().sections {
        task903_materialize_stage8_section_def_core(section);
    }
    task903_materialize_first_cell_list_header_tail_65(&mut core);
    task903_materialize_first_table_picture_patch_from_reference(
        &mut core,
        &reference_first_table,
        0,
        Task903FirstTablePicturePatch::Full,
    );
    task903_materialize_first_table_picture_patch_from_reference(
        &mut core,
        &reference_first_table,
        1,
        Task903FirstTablePicturePatch::Full,
    );
    task903_materialize_first_table_ctrl_header_from_reference(&mut core, &reference_first_table);
    task903_materialize_first_table_cell_list_headers_from_reference(
        &mut core,
        &reference_first_table,
    );
    task903_materialize_first_table_record_from_reference(&mut core, &reference_first_table);
    task903_materialize_first_table_cell_para_headers_from_reference(
        &mut core,
        &reference_first_table,
    );
    task903_materialize_next_table_child_headers_from_reference(&mut core, &reference_second_table);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 10);
    task903_materialize_chart_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_chart_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 13);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 14);
    task903_materialize_industry_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_industry_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 15);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 20);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 21);
    task903_materialize_country_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_country_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 22);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 23);
    task903_materialize_region_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_region_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 24);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 25);

    if pre_notice_empty_para_records {
        changes.pre_notice_empty_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 26);
        changes.pre_notice_empty_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 27);
    }
    if notice_para_records {
        changes.notice_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 28);
    }

    if notice_table_full_object_with_tail {
        changes.notice_table_full_objects_with_tail +=
            task903_materialize_notice_table_full_object_with_encoded_tail_from_reference(
                &mut core,
                &reference_notice_table,
            );
    } else {
        if notice_table_ctrl_header {
            changes.notice_table_ctrl_headers +=
                task903_materialize_notice_table_ctrl_header_from_reference(
                    &mut core,
                    &reference_notice_table,
                );
        }
        if notice_table_record_with_tail {
            changes.notice_table_records_with_tail +=
                task903_materialize_notice_table_record_with_encoded_tail_from_reference(
                    &mut core,
                    &reference_notice_table,
                );
        }
        if notice_table_all_cell_headers {
            changes.notice_table_all_cell_headers +=
                task903_materialize_notice_table_all_cell_headers_from_reference(
                    &mut core,
                    &reference_notice_table,
                );
        }
    }

    if logo_group_para_records {
        changes.logo_group_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 29);
    }

    let hwp_bytes = core.export_hwp_native().expect("Stage 20 HWP 직렬화 실패");
    let out_dir = std::path::Path::new("output/poc/hwpx2hwp/task903/stage20_notice_boundary_probe");
    std::fs::create_dir_all(out_dir).expect("Stage 20 output dir 생성 실패");
    let out_path = out_dir.join(output_name);
    std::fs::write(&out_path, &hwp_bytes).expect("Stage 20 probe 파일 저장 실패");

    changes.bytes = hwp_bytes.len();

    let reloaded = DocumentCore::from_bytes(&hwp_bytes).expect("Stage 20 probe HWP 재로드 실패");
    assert_eq!(reloaded.page_count(), 9, "{} page count", output_name);
    assert_eq!(
        reloaded.document().sections[0].paragraphs[28].text,
        reference.document().sections[0].paragraphs[28].text,
        "{} notice paragraph text",
        output_name
    );

    let notice_table = task903_hwpx_h_01_notice_table(&reloaded);
    assert_eq!(
        notice_table.cells.len(),
        reference_notice_table.cells.len(),
        "{} notice table cell count",
        output_name
    );

    if notice_table_ctrl_header || notice_table_full_object_with_tail {
        assert_eq!(
            notice_table.raw_ctrl_data, reference_notice_table.raw_ctrl_data,
            "{} notice table CTRL_HEADER payload",
            output_name
        );
    }
    if notice_table_record_with_tail || notice_table_full_object_with_tail {
        assert_eq!(
            notice_table.raw_table_record_attr, reference_notice_table.raw_table_record_attr,
            "{} notice table TABLE attr",
            output_name
        );
    }
    if notice_table_all_cell_headers || notice_table_full_object_with_tail {
        assert_eq!(
            notice_table.cells[0].raw_list_extra.len(),
            reference_notice_table.cells[0].raw_list_extra.len(),
            "{} notice table first cell LIST_HEADER extra length",
            output_name
        );
        assert_eq!(
            notice_table.cells[0].paragraphs[0].raw_header_extra.len(),
            reference_notice_table.cells[0].paragraphs[0]
                .raw_header_extra
                .len(),
            "{} notice table first cell PARA_HEADER extra length",
            output_name
        );
    }

    eprintln!(
        "[#903 Stage 20] {}: bytes={}, pre_notice_empty_para_records={}, notice_para_records={}, notice_table_ctrl_headers={}, notice_table_records_with_tail={}, notice_table_all_cell_headers={}, notice_table_full_objects_with_tail={}, logo_group_para_records={}, pages={}",
        out_path.display(),
        changes.bytes,
        changes.pre_notice_empty_para_records,
        changes.notice_para_records,
        changes.notice_table_ctrl_headers,
        changes.notice_table_records_with_tail,
        changes.notice_table_all_cell_headers,
        changes.notice_table_full_objects_with_tail,
        changes.logo_group_para_records,
        reloaded.page_count()
    );

    changes
}

#[derive(Debug, Default)]
struct Task903Stage21Changes {
    bytes: usize,
    logo_group_para_records: usize,
    logo_group_common_attrs: usize,
    logo_group_shape_attrs: usize,
    logo_group_child_shape_attrs: usize,
    logo_group_child_full_pictures: usize,
    logo_group_full_objects: usize,
    attachment_title_para_records: usize,
    attachment_title_table_full_objects: usize,
}

#[allow(clippy::too_many_arguments)]
fn task903_generate_stage21_probe_variant(
    output_name: &str,
    logo_group_para_record: bool,
    logo_group_common_attr: bool,
    logo_group_shape_attr: bool,
    logo_group_child_shape_attrs: bool,
    logo_group_child_full_pictures: bool,
    logo_group_full_object: bool,
    attachment_title_para_record: bool,
    attachment_title_table_full_object: bool,
) -> Task903Stage21Changes {
    let bytes = load_sample("hwpx-h-01.hwpx");
    let mut core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");

    convert_if_hwpx_source(core.document_mut(), rhwp::parser::FileFormat::Hwpx);

    let reference = task903_reference_hwpx_h_01_core();
    let reference_first_table = task903_hwpx_h_01_first_table(&reference).clone();
    let reference_second_table = task903_hwpx_h_01_second_table(&reference).clone();
    let reference_chart_table = task903_hwpx_h_01_chart_table(&reference).clone();
    let reference_industry_table = task903_hwpx_h_01_industry_table(&reference).clone();
    let reference_country_table = task903_hwpx_h_01_country_table(&reference).clone();
    let reference_region_table = task903_hwpx_h_01_region_table(&reference).clone();
    let reference_notice_table = task903_hwpx_h_01_notice_table(&reference).clone();
    let reference_logo_group = task903_hwpx_h_01_logo_group(&reference).clone();
    let reference_attachment_table = task903_hwpx_h_01_attachment_title_table(&reference).clone();

    let mut changes = Task903Stage21Changes::default();

    // Stage 21 공통 기준선: Stage 20의 06_notice_table_full_object_with_tail을 재현한다.
    for section in &mut core.document_mut().sections {
        task903_materialize_stage8_section_def_core(section);
    }
    task903_materialize_first_cell_list_header_tail_65(&mut core);
    task903_materialize_first_table_picture_patch_from_reference(
        &mut core,
        &reference_first_table,
        0,
        Task903FirstTablePicturePatch::Full,
    );
    task903_materialize_first_table_picture_patch_from_reference(
        &mut core,
        &reference_first_table,
        1,
        Task903FirstTablePicturePatch::Full,
    );
    task903_materialize_first_table_ctrl_header_from_reference(&mut core, &reference_first_table);
    task903_materialize_first_table_cell_list_headers_from_reference(
        &mut core,
        &reference_first_table,
    );
    task903_materialize_first_table_record_from_reference(&mut core, &reference_first_table);
    task903_materialize_first_table_cell_para_headers_from_reference(
        &mut core,
        &reference_first_table,
    );
    task903_materialize_next_table_child_headers_from_reference(&mut core, &reference_second_table);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 10);
    task903_materialize_chart_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_chart_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 13);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 14);
    task903_materialize_industry_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_industry_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 15);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 20);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 21);
    task903_materialize_country_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_country_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 22);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 23);
    task903_materialize_region_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_region_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 24);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 25);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 26);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 27);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 28);
    task903_materialize_notice_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_notice_table,
    );

    if logo_group_para_record {
        changes.logo_group_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 29);
    }

    if logo_group_full_object {
        changes.logo_group_full_objects +=
            task903_materialize_logo_group_full_object_from_reference(
                &mut core,
                &reference_logo_group,
            );
    } else {
        if logo_group_common_attr {
            changes.logo_group_common_attrs += task903_materialize_logo_group_common_from_reference(
                &mut core,
                &reference_logo_group,
            );
        }
        if logo_group_shape_attr {
            changes.logo_group_shape_attrs +=
                task903_materialize_logo_group_shape_attr_from_reference(
                    &mut core,
                    &reference_logo_group,
                );
        }
        if logo_group_child_shape_attrs {
            changes.logo_group_child_shape_attrs +=
                task903_materialize_logo_group_child_shape_attrs_from_reference(
                    &mut core,
                    &reference_logo_group,
                );
        }
        if logo_group_child_full_pictures {
            changes.logo_group_child_full_pictures +=
                task903_materialize_logo_group_child_full_pictures_from_reference(
                    &mut core,
                    &reference_logo_group,
                );
        }
    }

    if attachment_title_para_record {
        changes.attachment_title_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 30);
    }
    if attachment_title_table_full_object {
        changes.attachment_title_table_full_objects +=
            task903_materialize_attachment_title_table_full_object_with_encoded_tail_from_reference(
                &mut core,
                &reference_attachment_table,
            );
    }

    let hwp_bytes = core.export_hwp_native().expect("Stage 21 HWP 직렬화 실패");
    let out_dir = std::path::Path::new("output/poc/hwpx2hwp/task903/stage21_logo_group_probe");
    std::fs::create_dir_all(out_dir).expect("Stage 21 output dir 생성 실패");
    let out_path = out_dir.join(output_name);
    std::fs::write(&out_path, &hwp_bytes).expect("Stage 21 probe 파일 저장 실패");

    changes.bytes = hwp_bytes.len();

    let reloaded = DocumentCore::from_bytes(&hwp_bytes).expect("Stage 21 probe HWP 재로드 실패");
    assert_eq!(reloaded.page_count(), 9, "{} page count", output_name);

    let logo_group = task903_hwpx_h_01_logo_group(&reloaded);
    assert_eq!(
        logo_group.children.len(),
        reference_logo_group.children.len(),
        "{} logo group child count",
        output_name
    );

    if logo_group_common_attr || logo_group_full_object {
        assert_eq!(
            logo_group.common.instance_id, reference_logo_group.common.instance_id,
            "{} logo group common instance_id",
            output_name
        );
    }
    if logo_group_shape_attr || logo_group_full_object {
        assert_eq!(
            logo_group.shape_attr.current_width, reference_logo_group.shape_attr.current_width,
            "{} logo group current_width",
            output_name
        );
        assert_eq!(
            logo_group.shape_attr.current_height, reference_logo_group.shape_attr.current_height,
            "{} logo group current_height",
            output_name
        );
    }
    if logo_group_child_shape_attrs || logo_group_child_full_pictures || logo_group_full_object {
        for (idx, (actual_child, reference_child)) in logo_group
            .children
            .iter()
            .zip(reference_logo_group.children.iter())
            .enumerate()
        {
            let (ShapeObject::Picture(actual), ShapeObject::Picture(reference)) =
                (actual_child, reference_child)
            else {
                panic!("{} child[{}] must be picture", output_name, idx);
            };
            assert_eq!(
                actual.shape_attr.current_width, reference.shape_attr.current_width,
                "{} child[{}] current_width",
                output_name, idx
            );
            assert_eq!(
                actual.shape_attr.current_height, reference.shape_attr.current_height,
                "{} child[{}] current_height",
                output_name, idx
            );
        }
    }

    eprintln!(
        "[#903 Stage 21] {}: bytes={}, logo_group_para_records={}, logo_group_common_attrs={}, logo_group_shape_attrs={}, logo_group_child_shape_attrs={}, logo_group_child_full_pictures={}, logo_group_full_objects={}, attachment_title_para_records={}, attachment_title_table_full_objects={}, pages={}",
        out_path.display(),
        changes.bytes,
        changes.logo_group_para_records,
        changes.logo_group_common_attrs,
        changes.logo_group_shape_attrs,
        changes.logo_group_child_shape_attrs,
        changes.logo_group_child_full_pictures,
        changes.logo_group_full_objects,
        changes.attachment_title_para_records,
        changes.attachment_title_table_full_objects,
        reloaded.page_count()
    );

    changes
}

#[derive(Debug, Default)]
struct Task903Stage22Changes {
    bytes: usize,
    top_country_title_para_records: usize,
    top_country_host_para_records: usize,
    top_country_table_ctrl_headers: usize,
    top_country_table_records_with_tail: usize,
    top_country_table_all_cell_headers: usize,
    top_country_table_full_objects_with_tail: usize,
    next_boundary_para_records: usize,
}

#[allow(clippy::too_many_arguments)]
fn task903_generate_stage22_probe_variant(
    output_name: &str,
    top_country_title_para_record: bool,
    top_country_host_para_record: bool,
    top_country_table_ctrl_header: bool,
    top_country_table_record_with_tail: bool,
    top_country_table_all_cell_headers: bool,
    top_country_table_full_object_with_tail: bool,
    next_boundary_para_record: bool,
) -> Task903Stage22Changes {
    let bytes = load_sample("hwpx-h-01.hwpx");
    let mut core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");

    convert_if_hwpx_source(core.document_mut(), rhwp::parser::FileFormat::Hwpx);

    let reference = task903_reference_hwpx_h_01_core();
    let reference_first_table = task903_hwpx_h_01_first_table(&reference).clone();
    let reference_second_table = task903_hwpx_h_01_second_table(&reference).clone();
    let reference_chart_table = task903_hwpx_h_01_chart_table(&reference).clone();
    let reference_industry_table = task903_hwpx_h_01_industry_table(&reference).clone();
    let reference_country_table = task903_hwpx_h_01_country_table(&reference).clone();
    let reference_region_table = task903_hwpx_h_01_region_table(&reference).clone();
    let reference_notice_table = task903_hwpx_h_01_notice_table(&reference).clone();
    let reference_logo_group = task903_hwpx_h_01_logo_group(&reference).clone();
    let reference_attachment_table = task903_hwpx_h_01_attachment_title_table(&reference).clone();
    let reference_top_country_table = task903_hwpx_h_01_top_country_table(&reference).clone();

    let mut changes = Task903Stage22Changes::default();

    // Stage 22 공통 기준선: Stage 21의 08_logo_group_full_tuple_plus_attachment_title_table을 재현한다.
    for section in &mut core.document_mut().sections {
        task903_materialize_stage8_section_def_core(section);
    }
    task903_materialize_first_cell_list_header_tail_65(&mut core);
    task903_materialize_first_table_picture_patch_from_reference(
        &mut core,
        &reference_first_table,
        0,
        Task903FirstTablePicturePatch::Full,
    );
    task903_materialize_first_table_picture_patch_from_reference(
        &mut core,
        &reference_first_table,
        1,
        Task903FirstTablePicturePatch::Full,
    );
    task903_materialize_first_table_ctrl_header_from_reference(&mut core, &reference_first_table);
    task903_materialize_first_table_cell_list_headers_from_reference(
        &mut core,
        &reference_first_table,
    );
    task903_materialize_first_table_record_from_reference(&mut core, &reference_first_table);
    task903_materialize_first_table_cell_para_headers_from_reference(
        &mut core,
        &reference_first_table,
    );
    task903_materialize_next_table_child_headers_from_reference(&mut core, &reference_second_table);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 10);
    task903_materialize_chart_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_chart_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 13);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 14);
    task903_materialize_industry_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_industry_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 15);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 20);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 21);
    task903_materialize_country_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_country_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 22);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 23);
    task903_materialize_region_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_region_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 24);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 25);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 26);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 27);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 28);
    task903_materialize_notice_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_notice_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 29);
    task903_materialize_logo_group_full_object_from_reference(&mut core, &reference_logo_group);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 30);
    task903_materialize_attachment_title_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_attachment_table,
    );

    if top_country_title_para_record {
        changes.top_country_title_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 43);
    }
    if top_country_host_para_record {
        changes.top_country_host_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 44);
    }

    if top_country_table_full_object_with_tail {
        changes.top_country_table_full_objects_with_tail +=
            task903_materialize_top_country_table_full_object_with_encoded_tail_from_reference(
                &mut core,
                &reference_top_country_table,
            );
    } else {
        if top_country_table_ctrl_header {
            changes.top_country_table_ctrl_headers +=
                task903_materialize_top_country_table_ctrl_header_from_reference(
                    &mut core,
                    &reference_top_country_table,
                );
        }
        if top_country_table_record_with_tail {
            changes.top_country_table_records_with_tail +=
                task903_materialize_top_country_table_record_with_encoded_tail_from_reference(
                    &mut core,
                    &reference_top_country_table,
                );
        }
        if top_country_table_all_cell_headers {
            changes.top_country_table_all_cell_headers +=
                task903_materialize_top_country_table_all_cell_headers_from_reference(
                    &mut core,
                    &reference_top_country_table,
                );
        }
    }

    if next_boundary_para_record {
        changes.next_boundary_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 45);
    }

    let hwp_bytes = core.export_hwp_native().expect("Stage 22 HWP 직렬화 실패");
    let out_dir = std::path::Path::new("output/poc/hwpx2hwp/task903/stage22_top_country_probe");
    std::fs::create_dir_all(out_dir).expect("Stage 22 output dir 생성 실패");
    let out_path = out_dir.join(output_name);
    std::fs::write(&out_path, &hwp_bytes).expect("Stage 22 probe 파일 저장 실패");

    changes.bytes = hwp_bytes.len();

    let reloaded = DocumentCore::from_bytes(&hwp_bytes).expect("Stage 22 probe HWP 재로드 실패");
    assert_eq!(reloaded.page_count(), 9, "{} page count", output_name);
    assert_eq!(
        reloaded.document().sections[0].paragraphs[43].text,
        reference.document().sections[0].paragraphs[43].text,
        "{} top country title text",
        output_name
    );

    let top_country_table = task903_hwpx_h_01_top_country_table(&reloaded);
    assert_eq!(
        top_country_table.cells.len(),
        reference_top_country_table.cells.len(),
        "{} top country table cell count",
        output_name
    );

    if top_country_table_ctrl_header || top_country_table_full_object_with_tail {
        assert_eq!(
            top_country_table.raw_ctrl_data, reference_top_country_table.raw_ctrl_data,
            "{} top country table CTRL_HEADER payload",
            output_name
        );
    }
    if top_country_table_record_with_tail || top_country_table_full_object_with_tail {
        assert_eq!(
            top_country_table.raw_table_record_attr,
            reference_top_country_table.raw_table_record_attr,
            "{} top country table TABLE attr",
            output_name
        );
    }
    if top_country_table_all_cell_headers || top_country_table_full_object_with_tail {
        assert_eq!(
            top_country_table.cells[0].raw_list_extra.len(),
            reference_top_country_table.cells[0].raw_list_extra.len(),
            "{} top country table first cell LIST_HEADER extra length",
            output_name
        );
        assert_eq!(
            top_country_table.cells[0].paragraphs[0]
                .raw_header_extra
                .len(),
            reference_top_country_table.cells[0].paragraphs[0]
                .raw_header_extra
                .len(),
            "{} top country table first cell PARA_HEADER extra length",
            output_name
        );
    }

    eprintln!(
        "[#903 Stage 22] {}: bytes={}, top_country_title_para_records={}, top_country_host_para_records={}, top_country_table_ctrl_headers={}, top_country_table_records_with_tail={}, top_country_table_all_cell_headers={}, top_country_table_full_objects_with_tail={}, next_boundary_para_records={}, pages={}",
        out_path.display(),
        changes.bytes,
        changes.top_country_title_para_records,
        changes.top_country_host_para_records,
        changes.top_country_table_ctrl_headers,
        changes.top_country_table_records_with_tail,
        changes.top_country_table_all_cell_headers,
        changes.top_country_table_full_objects_with_tail,
        changes.next_boundary_para_records,
        reloaded.page_count()
    );

    changes
}

#[derive(Debug, Default)]
struct Task903Stage23Changes {
    bytes: usize,
    pre_year_empty_para_records: usize,
    year_title_para_records: usize,
    year_blank_para_records: usize,
    year_host_para_records: usize,
    year_table_ctrl_headers: usize,
    year_table_records_with_tail: usize,
    year_table_all_cell_headers: usize,
    year_table_full_objects_with_tail: usize,
    next_boundary_para_records: usize,
}

#[allow(clippy::too_many_arguments)]
fn task903_generate_stage23_probe_variant(
    output_name: &str,
    pre_year_empty_para_records: bool,
    year_title_para_record: bool,
    year_blank_para_record: bool,
    year_host_para_record: bool,
    year_table_ctrl_header: bool,
    year_table_record_with_tail: bool,
    year_table_all_cell_headers: bool,
    year_table_full_object_with_tail: bool,
    next_boundary_para_records: bool,
) -> Task903Stage23Changes {
    let bytes = load_sample("hwpx-h-01.hwpx");
    let mut core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");

    convert_if_hwpx_source(core.document_mut(), rhwp::parser::FileFormat::Hwpx);

    let reference = task903_reference_hwpx_h_01_core();
    let reference_first_table = task903_hwpx_h_01_first_table(&reference).clone();
    let reference_second_table = task903_hwpx_h_01_second_table(&reference).clone();
    let reference_chart_table = task903_hwpx_h_01_chart_table(&reference).clone();
    let reference_industry_table = task903_hwpx_h_01_industry_table(&reference).clone();
    let reference_country_table = task903_hwpx_h_01_country_table(&reference).clone();
    let reference_region_table = task903_hwpx_h_01_region_table(&reference).clone();
    let reference_notice_table = task903_hwpx_h_01_notice_table(&reference).clone();
    let reference_logo_group = task903_hwpx_h_01_logo_group(&reference).clone();
    let reference_attachment_table = task903_hwpx_h_01_attachment_title_table(&reference).clone();
    let reference_top_country_table = task903_hwpx_h_01_top_country_table(&reference).clone();
    let reference_year_trend_table = task903_hwpx_h_01_year_trend_table(&reference).clone();

    let mut changes = Task903Stage23Changes::default();

    // Stage 23 공통 기준선: Stage 22의 08_top_country_full_tuple_plus_next_boundary를 재현한다.
    for section in &mut core.document_mut().sections {
        task903_materialize_stage8_section_def_core(section);
    }
    task903_materialize_first_cell_list_header_tail_65(&mut core);
    task903_materialize_first_table_picture_patch_from_reference(
        &mut core,
        &reference_first_table,
        0,
        Task903FirstTablePicturePatch::Full,
    );
    task903_materialize_first_table_picture_patch_from_reference(
        &mut core,
        &reference_first_table,
        1,
        Task903FirstTablePicturePatch::Full,
    );
    task903_materialize_first_table_ctrl_header_from_reference(&mut core, &reference_first_table);
    task903_materialize_first_table_cell_list_headers_from_reference(
        &mut core,
        &reference_first_table,
    );
    task903_materialize_first_table_record_from_reference(&mut core, &reference_first_table);
    task903_materialize_first_table_cell_para_headers_from_reference(
        &mut core,
        &reference_first_table,
    );
    task903_materialize_next_table_child_headers_from_reference(&mut core, &reference_second_table);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 10);
    task903_materialize_chart_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_chart_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 13);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 14);
    task903_materialize_industry_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_industry_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 15);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 20);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 21);
    task903_materialize_country_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_country_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 22);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 23);
    task903_materialize_region_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_region_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 24);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 25);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 26);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 27);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 28);
    task903_materialize_notice_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_notice_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 29);
    task903_materialize_logo_group_full_object_from_reference(&mut core, &reference_logo_group);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 30);
    task903_materialize_attachment_title_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_attachment_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 43);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 44);
    task903_materialize_top_country_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_top_country_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 45);

    if pre_year_empty_para_records {
        changes.pre_year_empty_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 48);
        changes.pre_year_empty_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 49);
    }
    if year_title_para_record {
        changes.year_title_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 50);
    }
    if year_blank_para_record {
        changes.year_blank_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 51);
    }
    if year_host_para_record {
        changes.year_host_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 52);
    }

    if year_table_full_object_with_tail {
        changes.year_table_full_objects_with_tail +=
            task903_materialize_year_trend_table_full_object_with_encoded_tail_from_reference(
                &mut core,
                &reference_year_trend_table,
            );
    } else {
        if year_table_ctrl_header {
            changes.year_table_ctrl_headers +=
                task903_materialize_year_trend_table_ctrl_header_from_reference(
                    &mut core,
                    &reference_year_trend_table,
                );
        }
        if year_table_record_with_tail {
            changes.year_table_records_with_tail +=
                task903_materialize_year_trend_table_record_with_encoded_tail_from_reference(
                    &mut core,
                    &reference_year_trend_table,
                );
        }
        if year_table_all_cell_headers {
            changes.year_table_all_cell_headers +=
                task903_materialize_year_trend_table_all_cell_headers_from_reference(
                    &mut core,
                    &reference_year_trend_table,
                );
        }
    }

    if next_boundary_para_records {
        changes.next_boundary_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 53);
        changes.next_boundary_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 54);
    }

    let hwp_bytes = core.export_hwp_native().expect("Stage 23 HWP 직렬화 실패");
    let out_dir = std::path::Path::new("output/poc/hwpx2hwp/task903/stage23_year_trend_probe");
    std::fs::create_dir_all(out_dir).expect("Stage 23 output dir 생성 실패");
    let out_path = out_dir.join(output_name);
    std::fs::write(&out_path, &hwp_bytes).expect("Stage 23 probe 파일 저장 실패");

    changes.bytes = hwp_bytes.len();

    let reloaded = DocumentCore::from_bytes(&hwp_bytes).expect("Stage 23 probe HWP 재로드 실패");
    assert_eq!(reloaded.page_count(), 9, "{} page count", output_name);

    if year_title_para_record {
        assert_eq!(
            reloaded.document().sections[0].paragraphs[50].text,
            reference.document().sections[0].paragraphs[50].text,
            "{} year trend title text",
            output_name
        );
    }

    let year_trend_table = task903_hwpx_h_01_year_trend_table(&reloaded);
    assert_eq!(
        year_trend_table.cells.len(),
        reference_year_trend_table.cells.len(),
        "{} year trend table cell count",
        output_name
    );

    if year_table_ctrl_header || year_table_full_object_with_tail {
        assert_eq!(
            year_trend_table.raw_ctrl_data, reference_year_trend_table.raw_ctrl_data,
            "{} year trend table CTRL_HEADER payload",
            output_name
        );
    }
    if year_table_record_with_tail || year_table_full_object_with_tail {
        assert_eq!(
            year_trend_table.raw_table_record_attr,
            reference_year_trend_table.raw_table_record_attr,
            "{} year trend table TABLE attr",
            output_name
        );
    }
    if year_table_all_cell_headers || year_table_full_object_with_tail {
        assert_eq!(
            year_trend_table.cells[0].raw_list_extra.len(),
            reference_year_trend_table.cells[0].raw_list_extra.len(),
            "{} year trend table first cell LIST_HEADER extra length",
            output_name
        );
        assert_eq!(
            year_trend_table.cells[0].paragraphs[0]
                .raw_header_extra
                .len(),
            reference_year_trend_table.cells[0].paragraphs[0]
                .raw_header_extra
                .len(),
            "{} year trend table first cell PARA_HEADER extra length",
            output_name
        );
    }

    eprintln!(
        "[#903 Stage 23] {}: bytes={}, pre_year_empty_para_records={}, year_title_para_records={}, year_blank_para_records={}, year_host_para_records={}, year_table_ctrl_headers={}, year_table_records_with_tail={}, year_table_all_cell_headers={}, year_table_full_objects_with_tail={}, next_boundary_para_records={}, pages={}",
        out_path.display(),
        changes.bytes,
        changes.pre_year_empty_para_records,
        changes.year_title_para_records,
        changes.year_blank_para_records,
        changes.year_host_para_records,
        changes.year_table_ctrl_headers,
        changes.year_table_records_with_tail,
        changes.year_table_all_cell_headers,
        changes.year_table_full_objects_with_tail,
        changes.next_boundary_para_records,
        reloaded.page_count()
    );

    changes
}

#[derive(Debug, Default)]
struct Task903Stage24Changes {
    bytes: usize,
    pre_second_year_empty_para_records: usize,
    second_year_title_para_records: usize,
    second_year_blank_para_records: usize,
    second_year_host_para_records: usize,
    second_year_table_ctrl_headers: usize,
    second_year_table_records_with_tail: usize,
    second_year_table_all_cell_headers: usize,
    second_year_table_full_objects_with_tail: usize,
    next_boundary_para_records: usize,
}

#[allow(clippy::too_many_arguments)]
fn task903_generate_stage24_probe_variant(
    output_name: &str,
    pre_second_year_empty_para_records: bool,
    second_year_title_para_record: bool,
    second_year_blank_para_record: bool,
    second_year_host_para_record: bool,
    second_year_table_ctrl_header: bool,
    second_year_table_record_with_tail: bool,
    second_year_table_all_cell_headers: bool,
    second_year_table_full_object_with_tail: bool,
    next_boundary_para_records: bool,
) -> Task903Stage24Changes {
    let bytes = load_sample("hwpx-h-01.hwpx");
    let mut core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");

    convert_if_hwpx_source(core.document_mut(), rhwp::parser::FileFormat::Hwpx);

    let reference = task903_reference_hwpx_h_01_core();
    let reference_first_table = task903_hwpx_h_01_first_table(&reference).clone();
    let reference_second_table = task903_hwpx_h_01_second_table(&reference).clone();
    let reference_chart_table = task903_hwpx_h_01_chart_table(&reference).clone();
    let reference_industry_table = task903_hwpx_h_01_industry_table(&reference).clone();
    let reference_country_table = task903_hwpx_h_01_country_table(&reference).clone();
    let reference_region_table = task903_hwpx_h_01_region_table(&reference).clone();
    let reference_notice_table = task903_hwpx_h_01_notice_table(&reference).clone();
    let reference_logo_group = task903_hwpx_h_01_logo_group(&reference).clone();
    let reference_attachment_table = task903_hwpx_h_01_attachment_title_table(&reference).clone();
    let reference_top_country_table = task903_hwpx_h_01_top_country_table(&reference).clone();
    let reference_year_trend_table = task903_hwpx_h_01_year_trend_table(&reference).clone();
    let reference_second_year_trend_table =
        task903_hwpx_h_01_second_year_trend_table(&reference).clone();

    let mut changes = Task903Stage24Changes::default();

    // Stage 24 공통 기준선: Stage 23의 08_year_full_tuple_plus_next_boundary를 재현한다.
    for section in &mut core.document_mut().sections {
        task903_materialize_stage8_section_def_core(section);
    }
    task903_materialize_first_cell_list_header_tail_65(&mut core);
    task903_materialize_first_table_picture_patch_from_reference(
        &mut core,
        &reference_first_table,
        0,
        Task903FirstTablePicturePatch::Full,
    );
    task903_materialize_first_table_picture_patch_from_reference(
        &mut core,
        &reference_first_table,
        1,
        Task903FirstTablePicturePatch::Full,
    );
    task903_materialize_first_table_ctrl_header_from_reference(&mut core, &reference_first_table);
    task903_materialize_first_table_cell_list_headers_from_reference(
        &mut core,
        &reference_first_table,
    );
    task903_materialize_first_table_record_from_reference(&mut core, &reference_first_table);
    task903_materialize_first_table_cell_para_headers_from_reference(
        &mut core,
        &reference_first_table,
    );
    task903_materialize_next_table_child_headers_from_reference(&mut core, &reference_second_table);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 10);
    task903_materialize_chart_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_chart_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 13);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 14);
    task903_materialize_industry_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_industry_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 15);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 20);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 21);
    task903_materialize_country_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_country_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 22);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 23);
    task903_materialize_region_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_region_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 24);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 25);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 26);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 27);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 28);
    task903_materialize_notice_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_notice_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 29);
    task903_materialize_logo_group_full_object_from_reference(&mut core, &reference_logo_group);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 30);
    task903_materialize_attachment_title_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_attachment_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 43);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 44);
    task903_materialize_top_country_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_top_country_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 45);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 48);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 49);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 50);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 51);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 52);
    task903_materialize_year_trend_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_year_trend_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 53);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 54);

    if pre_second_year_empty_para_records {
        changes.pre_second_year_empty_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 85);
        changes.pre_second_year_empty_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 86);
    }
    if second_year_title_para_record {
        changes.second_year_title_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 87);
    }
    if second_year_blank_para_record {
        changes.second_year_blank_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 88);
    }
    if second_year_host_para_record {
        changes.second_year_host_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 89);
    }

    if second_year_table_full_object_with_tail {
        changes.second_year_table_full_objects_with_tail +=
            task903_materialize_second_year_trend_table_full_object_with_encoded_tail_from_reference(
                &mut core,
                &reference_second_year_trend_table,
            );
    } else {
        if second_year_table_ctrl_header {
            changes.second_year_table_ctrl_headers +=
                task903_materialize_second_year_trend_table_ctrl_header_from_reference(
                    &mut core,
                    &reference_second_year_trend_table,
                );
        }
        if second_year_table_record_with_tail {
            changes.second_year_table_records_with_tail +=
                task903_materialize_second_year_trend_table_record_with_encoded_tail_from_reference(
                    &mut core,
                    &reference_second_year_trend_table,
                );
        }
        if second_year_table_all_cell_headers {
            changes.second_year_table_all_cell_headers +=
                task903_materialize_second_year_trend_table_all_cell_headers_from_reference(
                    &mut core,
                    &reference_second_year_trend_table,
                );
        }
    }

    if next_boundary_para_records {
        changes.next_boundary_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 90);
        changes.next_boundary_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 91);
        changes.next_boundary_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 92);
    }

    let hwp_bytes = core.export_hwp_native().expect("Stage 24 HWP 직렬화 실패");
    let out_dir =
        std::path::Path::new("output/poc/hwpx2hwp/task903/stage24_second_year_trend_probe");
    std::fs::create_dir_all(out_dir).expect("Stage 24 output dir 생성 실패");
    let out_path = out_dir.join(output_name);
    std::fs::write(&out_path, &hwp_bytes).expect("Stage 24 probe 파일 저장 실패");

    changes.bytes = hwp_bytes.len();

    let reloaded = DocumentCore::from_bytes(&hwp_bytes).expect("Stage 24 probe HWP 재로드 실패");
    assert_eq!(reloaded.page_count(), 9, "{} page count", output_name);

    if second_year_title_para_record {
        assert_eq!(
            reloaded.document().sections[0].paragraphs[87].text,
            reference.document().sections[0].paragraphs[87].text,
            "{} second year trend title text",
            output_name
        );
    }

    let second_year_trend_table = task903_hwpx_h_01_second_year_trend_table(&reloaded);
    assert_eq!(
        second_year_trend_table.cells.len(),
        reference_second_year_trend_table.cells.len(),
        "{} second year trend table cell count",
        output_name
    );

    if second_year_table_ctrl_header || second_year_table_full_object_with_tail {
        assert_eq!(
            second_year_trend_table.raw_ctrl_data, reference_second_year_trend_table.raw_ctrl_data,
            "{} second year trend table CTRL_HEADER payload",
            output_name
        );
    }
    if second_year_table_record_with_tail || second_year_table_full_object_with_tail {
        assert_eq!(
            second_year_trend_table.raw_table_record_attr,
            reference_second_year_trend_table.raw_table_record_attr,
            "{} second year trend table TABLE attr",
            output_name
        );
    }
    if second_year_table_all_cell_headers || second_year_table_full_object_with_tail {
        assert_eq!(
            second_year_trend_table.cells[0].raw_list_extra.len(),
            reference_second_year_trend_table.cells[0]
                .raw_list_extra
                .len(),
            "{} second year trend table first cell LIST_HEADER extra length",
            output_name
        );
        assert_eq!(
            second_year_trend_table.cells[0].paragraphs[0]
                .raw_header_extra
                .len(),
            reference_second_year_trend_table.cells[0].paragraphs[0]
                .raw_header_extra
                .len(),
            "{} second year trend table first cell PARA_HEADER extra length",
            output_name
        );
    }

    eprintln!(
        "[#903 Stage 24] {}: bytes={}, pre_second_year_empty_para_records={}, second_year_title_para_records={}, second_year_blank_para_records={}, second_year_host_para_records={}, second_year_table_ctrl_headers={}, second_year_table_records_with_tail={}, second_year_table_all_cell_headers={}, second_year_table_full_objects_with_tail={}, next_boundary_para_records={}, pages={}",
        out_path.display(),
        changes.bytes,
        changes.pre_second_year_empty_para_records,
        changes.second_year_title_para_records,
        changes.second_year_blank_para_records,
        changes.second_year_host_para_records,
        changes.second_year_table_ctrl_headers,
        changes.second_year_table_records_with_tail,
        changes.second_year_table_all_cell_headers,
        changes.second_year_table_full_objects_with_tail,
        changes.next_boundary_para_records,
        reloaded.page_count()
    );

    changes
}

#[derive(Debug, Default)]
struct Task903Stage25Changes {
    bytes: usize,
    final_industry_blank_para_records: usize,
    final_industry_host_para_records: usize,
    final_industry_table_ctrl_headers: usize,
    final_industry_table_records_with_tail: usize,
    final_industry_table_all_cell_headers: usize,
    final_industry_table_full_objects_with_tail: usize,
    next_boundary_para_records: usize,
}

#[allow(clippy::too_many_arguments)]
fn task903_generate_stage25_probe_variant(
    output_name: &str,
    final_industry_blank_para_record: bool,
    final_industry_host_para_record: bool,
    final_industry_table_ctrl_header: bool,
    final_industry_table_record_with_tail: bool,
    final_industry_table_all_cell_headers: bool,
    final_industry_table_full_object_with_tail: bool,
    next_boundary_para_records: bool,
) -> Task903Stage25Changes {
    let bytes = load_sample("hwpx-h-01.hwpx");
    let mut core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");

    convert_if_hwpx_source(core.document_mut(), rhwp::parser::FileFormat::Hwpx);

    let reference = task903_reference_hwpx_h_01_core();
    let reference_first_table = task903_hwpx_h_01_first_table(&reference).clone();
    let reference_second_table = task903_hwpx_h_01_second_table(&reference).clone();
    let reference_chart_table = task903_hwpx_h_01_chart_table(&reference).clone();
    let reference_industry_table = task903_hwpx_h_01_industry_table(&reference).clone();
    let reference_country_table = task903_hwpx_h_01_country_table(&reference).clone();
    let reference_region_table = task903_hwpx_h_01_region_table(&reference).clone();
    let reference_notice_table = task903_hwpx_h_01_notice_table(&reference).clone();
    let reference_logo_group = task903_hwpx_h_01_logo_group(&reference).clone();
    let reference_attachment_table = task903_hwpx_h_01_attachment_title_table(&reference).clone();
    let reference_top_country_table = task903_hwpx_h_01_top_country_table(&reference).clone();
    let reference_year_trend_table = task903_hwpx_h_01_year_trend_table(&reference).clone();
    let reference_second_year_trend_table =
        task903_hwpx_h_01_second_year_trend_table(&reference).clone();
    let reference_final_industry_table = task903_hwpx_h_01_final_industry_table(&reference).clone();

    let mut changes = Task903Stage25Changes::default();

    // Stage 25 공통 기준선: Stage 24의 08_second_year_full_tuple_plus_next_boundary를 재현한다.
    for section in &mut core.document_mut().sections {
        task903_materialize_stage8_section_def_core(section);
    }
    task903_materialize_first_cell_list_header_tail_65(&mut core);
    task903_materialize_first_table_picture_patch_from_reference(
        &mut core,
        &reference_first_table,
        0,
        Task903FirstTablePicturePatch::Full,
    );
    task903_materialize_first_table_picture_patch_from_reference(
        &mut core,
        &reference_first_table,
        1,
        Task903FirstTablePicturePatch::Full,
    );
    task903_materialize_first_table_ctrl_header_from_reference(&mut core, &reference_first_table);
    task903_materialize_first_table_cell_list_headers_from_reference(
        &mut core,
        &reference_first_table,
    );
    task903_materialize_first_table_record_from_reference(&mut core, &reference_first_table);
    task903_materialize_first_table_cell_para_headers_from_reference(
        &mut core,
        &reference_first_table,
    );
    task903_materialize_next_table_child_headers_from_reference(&mut core, &reference_second_table);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 10);
    task903_materialize_chart_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_chart_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 13);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 14);
    task903_materialize_industry_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_industry_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 15);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 20);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 21);
    task903_materialize_country_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_country_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 22);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 23);
    task903_materialize_region_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_region_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 24);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 25);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 26);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 27);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 28);
    task903_materialize_notice_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_notice_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 29);
    task903_materialize_logo_group_full_object_from_reference(&mut core, &reference_logo_group);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 30);
    task903_materialize_attachment_title_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_attachment_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 43);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 44);
    task903_materialize_top_country_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_top_country_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 45);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 48);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 49);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 50);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 51);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 52);
    task903_materialize_year_trend_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_year_trend_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 53);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 54);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 85);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 86);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 87);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 88);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 89);
    task903_materialize_second_year_trend_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_second_year_trend_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 90);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 91);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 92);

    if final_industry_blank_para_record {
        changes.final_industry_blank_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 93);
    }
    if final_industry_host_para_record {
        changes.final_industry_host_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 94);
    }

    if final_industry_table_full_object_with_tail {
        changes.final_industry_table_full_objects_with_tail +=
            task903_materialize_final_industry_table_full_object_with_encoded_tail_from_reference(
                &mut core,
                &reference_final_industry_table,
            );
    } else {
        if final_industry_table_ctrl_header {
            changes.final_industry_table_ctrl_headers +=
                task903_materialize_final_industry_table_ctrl_header_from_reference(
                    &mut core,
                    &reference_final_industry_table,
                );
        }
        if final_industry_table_record_with_tail {
            changes.final_industry_table_records_with_tail +=
                task903_materialize_final_industry_table_record_with_encoded_tail_from_reference(
                    &mut core,
                    &reference_final_industry_table,
                );
        }
        if final_industry_table_all_cell_headers {
            changes.final_industry_table_all_cell_headers +=
                task903_materialize_final_industry_table_all_cell_headers_from_reference(
                    &mut core,
                    &reference_final_industry_table,
                );
        }
    }

    if next_boundary_para_records {
        changes.next_boundary_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 95);
        changes.next_boundary_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 96);
    }

    let hwp_bytes = core.export_hwp_native().expect("Stage 25 HWP 직렬화 실패");
    let out_dir = std::path::Path::new("output/poc/hwpx2hwp/task903/stage25_final_industry_probe");
    std::fs::create_dir_all(out_dir).expect("Stage 25 output dir 생성 실패");
    let out_path = out_dir.join(output_name);
    std::fs::write(&out_path, &hwp_bytes).expect("Stage 25 probe 파일 저장 실패");

    changes.bytes = hwp_bytes.len();

    let reloaded = DocumentCore::from_bytes(&hwp_bytes).expect("Stage 25 probe HWP 재로드 실패");
    assert_eq!(reloaded.page_count(), 9, "{} page count", output_name);
    assert_eq!(
        reloaded.document().sections[0].paragraphs[92].text,
        reference.document().sections[0].paragraphs[92].text,
        "{} final industry title text",
        output_name
    );

    let final_industry_table = task903_hwpx_h_01_final_industry_table(&reloaded);
    assert_eq!(
        final_industry_table.cells.len(),
        reference_final_industry_table.cells.len(),
        "{} final industry table cell count",
        output_name
    );

    if final_industry_table_ctrl_header || final_industry_table_full_object_with_tail {
        assert_eq!(
            final_industry_table.raw_ctrl_data, reference_final_industry_table.raw_ctrl_data,
            "{} final industry table CTRL_HEADER payload",
            output_name
        );
    }
    if final_industry_table_record_with_tail || final_industry_table_full_object_with_tail {
        assert_eq!(
            final_industry_table.raw_table_record_attr,
            reference_final_industry_table.raw_table_record_attr,
            "{} final industry table TABLE attr",
            output_name
        );
    }
    if final_industry_table_all_cell_headers || final_industry_table_full_object_with_tail {
        assert_eq!(
            final_industry_table.cells[0].raw_list_extra.len(),
            reference_final_industry_table.cells[0].raw_list_extra.len(),
            "{} final industry table first cell LIST_HEADER extra length",
            output_name
        );
        assert_eq!(
            final_industry_table.cells[0].paragraphs[0]
                .raw_header_extra
                .len(),
            reference_final_industry_table.cells[0].paragraphs[0]
                .raw_header_extra
                .len(),
            "{} final industry table first cell PARA_HEADER extra length",
            output_name
        );
    }

    eprintln!(
        "[#903 Stage 25] {}: bytes={}, final_industry_blank_para_records={}, final_industry_host_para_records={}, final_industry_table_ctrl_headers={}, final_industry_table_records_with_tail={}, final_industry_table_all_cell_headers={}, final_industry_table_full_objects_with_tail={}, next_boundary_para_records={}, pages={}",
        out_path.display(),
        changes.bytes,
        changes.final_industry_blank_para_records,
        changes.final_industry_host_para_records,
        changes.final_industry_table_ctrl_headers,
        changes.final_industry_table_records_with_tail,
        changes.final_industry_table_all_cell_headers,
        changes.final_industry_table_full_objects_with_tail,
        changes.next_boundary_para_records,
        reloaded.page_count()
    );

    changes
}

#[derive(Debug, Default)]
struct Task903Stage26Changes {
    bytes: usize,
    final_country_host_para_records: usize,
    final_country_table_ctrl_headers: usize,
    final_country_table_records_with_tail: usize,
    final_country_table_all_cell_headers: usize,
    final_country_table_full_objects_with_tail: usize,
    following_para_records: usize,
}

#[allow(clippy::too_many_arguments)]
fn task903_generate_stage26_probe_variant(
    output_name: &str,
    final_country_host_para_record: bool,
    final_country_table_ctrl_header: bool,
    final_country_table_record_with_tail: bool,
    final_country_table_all_cell_headers: bool,
    final_country_table_full_object_with_tail: bool,
    following_para_records: bool,
) -> Task903Stage26Changes {
    let bytes = load_sample("hwpx-h-01.hwpx");
    let mut core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");

    convert_if_hwpx_source(core.document_mut(), rhwp::parser::FileFormat::Hwpx);

    let reference = task903_reference_hwpx_h_01_core();
    let reference_first_table = task903_hwpx_h_01_first_table(&reference).clone();
    let reference_second_table = task903_hwpx_h_01_second_table(&reference).clone();
    let reference_chart_table = task903_hwpx_h_01_chart_table(&reference).clone();
    let reference_industry_table = task903_hwpx_h_01_industry_table(&reference).clone();
    let reference_country_table = task903_hwpx_h_01_country_table(&reference).clone();
    let reference_region_table = task903_hwpx_h_01_region_table(&reference).clone();
    let reference_notice_table = task903_hwpx_h_01_notice_table(&reference).clone();
    let reference_logo_group = task903_hwpx_h_01_logo_group(&reference).clone();
    let reference_attachment_table = task903_hwpx_h_01_attachment_title_table(&reference).clone();
    let reference_top_country_table = task903_hwpx_h_01_top_country_table(&reference).clone();
    let reference_year_trend_table = task903_hwpx_h_01_year_trend_table(&reference).clone();
    let reference_second_year_trend_table =
        task903_hwpx_h_01_second_year_trend_table(&reference).clone();
    let reference_final_industry_table = task903_hwpx_h_01_final_industry_table(&reference).clone();
    let reference_final_country_table = task903_hwpx_h_01_final_country_table(&reference).clone();

    let mut changes = Task903Stage26Changes::default();

    // Stage 26 공통 기준선: Stage 25의 08_final_industry_full_tuple_plus_next_boundary를 재현한다.
    for section in &mut core.document_mut().sections {
        task903_materialize_stage8_section_def_core(section);
    }
    task903_materialize_first_cell_list_header_tail_65(&mut core);
    task903_materialize_first_table_picture_patch_from_reference(
        &mut core,
        &reference_first_table,
        0,
        Task903FirstTablePicturePatch::Full,
    );
    task903_materialize_first_table_picture_patch_from_reference(
        &mut core,
        &reference_first_table,
        1,
        Task903FirstTablePicturePatch::Full,
    );
    task903_materialize_first_table_ctrl_header_from_reference(&mut core, &reference_first_table);
    task903_materialize_first_table_cell_list_headers_from_reference(
        &mut core,
        &reference_first_table,
    );
    task903_materialize_first_table_record_from_reference(&mut core, &reference_first_table);
    task903_materialize_first_table_cell_para_headers_from_reference(
        &mut core,
        &reference_first_table,
    );
    task903_materialize_next_table_child_headers_from_reference(&mut core, &reference_second_table);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 10);
    task903_materialize_chart_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_chart_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 13);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 14);
    task903_materialize_industry_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_industry_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 15);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 20);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 21);
    task903_materialize_country_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_country_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 22);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 23);
    task903_materialize_region_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_region_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 24);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 25);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 26);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 27);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 28);
    task903_materialize_notice_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_notice_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 29);
    task903_materialize_logo_group_full_object_from_reference(&mut core, &reference_logo_group);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 30);
    task903_materialize_attachment_title_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_attachment_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 43);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 44);
    task903_materialize_top_country_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_top_country_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 45);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 48);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 49);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 50);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 51);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 52);
    task903_materialize_year_trend_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_year_trend_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 53);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 54);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 85);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 86);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 87);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 88);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 89);
    task903_materialize_second_year_trend_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_second_year_trend_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 90);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 91);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 92);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 93);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 94);
    task903_materialize_final_industry_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_final_industry_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 95);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 96);

    if final_country_host_para_record {
        changes.final_country_host_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 97);
    }

    if final_country_table_full_object_with_tail {
        changes.final_country_table_full_objects_with_tail +=
            task903_materialize_final_country_table_full_object_with_encoded_tail_from_reference(
                &mut core,
                &reference_final_country_table,
            );
    } else {
        if final_country_table_ctrl_header {
            changes.final_country_table_ctrl_headers +=
                task903_materialize_final_country_table_ctrl_header_from_reference(
                    &mut core,
                    &reference_final_country_table,
                );
        }
        if final_country_table_record_with_tail {
            changes.final_country_table_records_with_tail +=
                task903_materialize_final_country_table_record_with_encoded_tail_from_reference(
                    &mut core,
                    &reference_final_country_table,
                );
        }
        if final_country_table_all_cell_headers {
            changes.final_country_table_all_cell_headers +=
                task903_materialize_final_country_table_all_cell_headers_from_reference(
                    &mut core,
                    &reference_final_country_table,
                );
        }
    }

    if following_para_records {
        changes.following_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 98);
        changes.following_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 99);
        changes.following_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 100);
        changes.following_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 101);
    }

    let hwp_bytes = core.export_hwp_native().expect("Stage 26 HWP 직렬화 실패");
    let out_dir = std::path::Path::new("output/poc/hwpx2hwp/task903/stage26_final_country_probe");
    std::fs::create_dir_all(out_dir).expect("Stage 26 output dir 생성 실패");
    let out_path = out_dir.join(output_name);
    std::fs::write(&out_path, &hwp_bytes).expect("Stage 26 probe 파일 저장 실패");

    changes.bytes = hwp_bytes.len();

    let reloaded = DocumentCore::from_bytes(&hwp_bytes).expect("Stage 26 probe HWP 재로드 실패");
    assert_eq!(reloaded.page_count(), 9, "{} page count", output_name);
    assert_eq!(
        reloaded.document().sections[0].paragraphs[95].text,
        reference.document().sections[0].paragraphs[95].text,
        "{} final country title text",
        output_name
    );

    let final_country_table = task903_hwpx_h_01_final_country_table(&reloaded);
    assert_eq!(
        final_country_table.cells.len(),
        reference_final_country_table.cells.len(),
        "{} final country table cell count",
        output_name
    );

    if final_country_table_ctrl_header || final_country_table_full_object_with_tail {
        assert_eq!(
            final_country_table.raw_ctrl_data, reference_final_country_table.raw_ctrl_data,
            "{} final country table CTRL_HEADER payload",
            output_name
        );
    }
    if final_country_table_record_with_tail || final_country_table_full_object_with_tail {
        assert_eq!(
            final_country_table.raw_table_record_attr,
            reference_final_country_table.raw_table_record_attr,
            "{} final country table TABLE attr",
            output_name
        );
    }
    if final_country_table_all_cell_headers || final_country_table_full_object_with_tail {
        assert_eq!(
            final_country_table.cells[0].raw_list_extra.len(),
            reference_final_country_table.cells[0].raw_list_extra.len(),
            "{} final country table first cell LIST_HEADER extra length",
            output_name
        );
        assert_eq!(
            final_country_table.cells[0].paragraphs[0]
                .raw_header_extra
                .len(),
            reference_final_country_table.cells[0].paragraphs[0]
                .raw_header_extra
                .len(),
            "{} final country table first cell PARA_HEADER extra length",
            output_name
        );
    }

    eprintln!(
        "[#903 Stage 26] {}: bytes={}, final_country_host_para_records={}, final_country_table_ctrl_headers={}, final_country_table_records_with_tail={}, final_country_table_all_cell_headers={}, final_country_table_full_objects_with_tail={}, following_para_records={}, pages={}",
        out_path.display(),
        changes.bytes,
        changes.final_country_host_para_records,
        changes.final_country_table_ctrl_headers,
        changes.final_country_table_records_with_tail,
        changes.final_country_table_all_cell_headers,
        changes.final_country_table_full_objects_with_tail,
        changes.following_para_records,
        reloaded.page_count()
    );

    changes
}

#[derive(Debug, Default)]
struct Task903Stage27Changes {
    bytes: usize,
    final_region_host_para_records: usize,
    final_region_table_records_with_tail: usize,
    final_region_table_full_objects_with_tail: usize,
    section1_para_records: usize,
    section1_full_paras: usize,
    section1_table_records_with_tail: usize,
    section1_table_full_objects_with_tail: usize,
}

#[allow(clippy::too_many_arguments)]
fn task903_generate_stage27_probe_variant(
    output_name: &str,
    final_region_host_para_record: bool,
    final_region_table_record_with_tail: bool,
    final_region_table_full_object_with_tail: bool,
    section1_para_record: bool,
    section1_full_para: bool,
    section1_table_record_with_tail: bool,
    section1_table_full_object_with_tail: bool,
) -> Task903Stage27Changes {
    let bytes = load_sample("hwpx-h-01.hwpx");
    let mut core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");

    convert_if_hwpx_source(core.document_mut(), rhwp::parser::FileFormat::Hwpx);

    let reference = task903_reference_hwpx_h_01_core();
    let reference_first_table = task903_hwpx_h_01_first_table(&reference).clone();
    let reference_second_table = task903_hwpx_h_01_second_table(&reference).clone();
    let reference_chart_table = task903_hwpx_h_01_chart_table(&reference).clone();
    let reference_industry_table = task903_hwpx_h_01_industry_table(&reference).clone();
    let reference_country_table = task903_hwpx_h_01_country_table(&reference).clone();
    let reference_region_table = task903_hwpx_h_01_region_table(&reference).clone();
    let reference_notice_table = task903_hwpx_h_01_notice_table(&reference).clone();
    let reference_logo_group = task903_hwpx_h_01_logo_group(&reference).clone();
    let reference_attachment_table = task903_hwpx_h_01_attachment_title_table(&reference).clone();
    let reference_top_country_table = task903_hwpx_h_01_top_country_table(&reference).clone();
    let reference_year_trend_table = task903_hwpx_h_01_year_trend_table(&reference).clone();
    let reference_second_year_trend_table =
        task903_hwpx_h_01_second_year_trend_table(&reference).clone();
    let reference_final_industry_table = task903_hwpx_h_01_final_industry_table(&reference).clone();
    let reference_final_country_table = task903_hwpx_h_01_final_country_table(&reference).clone();
    let reference_final_region_table = task903_hwpx_h_01_final_region_table(&reference).clone();
    let reference_section1_table = task903_hwpx_h_01_section1_reference_table(&reference).clone();

    let mut changes = Task903Stage27Changes::default();

    // Stage 27 공통 기준선: Stage 26의 07_final_country_full_tuple_plus_following_paras를 재현한다.
    for section in &mut core.document_mut().sections {
        task903_materialize_stage8_section_def_core(section);
    }
    task903_materialize_first_cell_list_header_tail_65(&mut core);
    task903_materialize_first_table_picture_patch_from_reference(
        &mut core,
        &reference_first_table,
        0,
        Task903FirstTablePicturePatch::Full,
    );
    task903_materialize_first_table_picture_patch_from_reference(
        &mut core,
        &reference_first_table,
        1,
        Task903FirstTablePicturePatch::Full,
    );
    task903_materialize_first_table_ctrl_header_from_reference(&mut core, &reference_first_table);
    task903_materialize_first_table_cell_list_headers_from_reference(
        &mut core,
        &reference_first_table,
    );
    task903_materialize_first_table_record_from_reference(&mut core, &reference_first_table);
    task903_materialize_first_table_cell_para_headers_from_reference(
        &mut core,
        &reference_first_table,
    );
    task903_materialize_next_table_child_headers_from_reference(&mut core, &reference_second_table);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 10);
    task903_materialize_chart_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_chart_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 13);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 14);
    task903_materialize_industry_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_industry_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 15);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 20);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 21);
    task903_materialize_country_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_country_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 22);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 23);
    task903_materialize_region_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_region_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 24);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 25);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 26);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 27);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 28);
    task903_materialize_notice_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_notice_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 29);
    task903_materialize_logo_group_full_object_from_reference(&mut core, &reference_logo_group);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 30);
    task903_materialize_attachment_title_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_attachment_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 43);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 44);
    task903_materialize_top_country_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_top_country_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 45);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 48);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 49);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 50);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 51);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 52);
    task903_materialize_year_trend_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_year_trend_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 53);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 54);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 85);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 86);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 87);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 88);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 89);
    task903_materialize_second_year_trend_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_second_year_trend_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 90);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 91);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 92);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 93);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 94);
    task903_materialize_final_industry_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_final_industry_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 95);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 96);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 97);
    task903_materialize_final_country_table_full_object_with_encoded_tail_from_reference(
        &mut core,
        &reference_final_country_table,
    );
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 98);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 99);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 100);
    task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 101);

    if final_region_host_para_record {
        changes.final_region_host_para_records +=
            task903_materialize_top_level_para_record_from_reference(&mut core, &reference, 102);
    }
    if final_region_table_full_object_with_tail {
        changes.final_region_table_full_objects_with_tail +=
            task903_materialize_final_region_table_full_object_with_encoded_tail_from_reference(
                &mut core,
                &reference_final_region_table,
            );
    } else if final_region_table_record_with_tail {
        changes.final_region_table_records_with_tail +=
            task903_materialize_final_region_table_record_with_encoded_tail_from_reference(
                &mut core,
                &reference_final_region_table,
            );
    }

    if section1_full_para {
        changes.section1_full_paras +=
            task903_materialize_full_para_from_reference(&mut core, &reference, 1, 0);
    } else {
        if section1_para_record {
            changes.section1_para_records +=
                task903_materialize_para_record_from_reference(&mut core, &reference, 1, 0);
        }
        if section1_table_full_object_with_tail {
            changes.section1_table_full_objects_with_tail +=
                task903_materialize_section1_reference_table_full_object_with_encoded_tail_from_reference(
                    &mut core,
                    &reference_section1_table,
                );
        } else if section1_table_record_with_tail {
            changes.section1_table_records_with_tail +=
                task903_materialize_section1_reference_table_record_with_encoded_tail_from_reference(
                    &mut core,
                    &reference_section1_table,
                );
        }
    }

    let hwp_bytes = core.export_hwp_native().expect("Stage 27 HWP 직렬화 실패");
    let out_dir = std::path::Path::new("output/poc/hwpx2hwp/task903/stage27_section1_probe");
    std::fs::create_dir_all(out_dir).expect("Stage 27 output dir 생성 실패");
    let out_path = out_dir.join(output_name);
    std::fs::write(&out_path, &hwp_bytes).expect("Stage 27 probe 파일 저장 실패");

    changes.bytes = hwp_bytes.len();

    let reloaded = DocumentCore::from_bytes(&hwp_bytes).expect("Stage 27 probe HWP 재로드 실패");
    assert_eq!(reloaded.page_count(), 9, "{} page count", output_name);

    let final_region_table = task903_hwpx_h_01_final_region_table(&reloaded);
    assert_eq!(
        final_region_table.cells.len(),
        reference_final_region_table.cells.len(),
        "{} final region table cell count",
        output_name
    );

    let section1_table = task903_hwpx_h_01_section1_reference_table(&reloaded);
    assert_eq!(
        section1_table.cells.len(),
        reference_section1_table.cells.len(),
        "{} section1 reference table cell count",
        output_name
    );

    eprintln!(
        "[#903 Stage 27] {}: bytes={}, final_region_host_para_records={}, final_region_table_records_with_tail={}, final_region_table_full_objects_with_tail={}, section1_para_records={}, section1_full_paras={}, section1_table_records_with_tail={}, section1_table_full_objects_with_tail={}, pages={}",
        out_path.display(),
        changes.bytes,
        changes.final_region_host_para_records,
        changes.final_region_table_records_with_tail,
        changes.final_region_table_full_objects_with_tail,
        changes.section1_para_records,
        changes.section1_full_paras,
        changes.section1_table_records_with_tail,
        changes.section1_table_full_objects_with_tail,
        reloaded.page_count()
    );

    changes
}

fn task903_stage27_baseline_section1_bytes() -> Vec<u8> {
    task903_generate_stage27_probe_variant(
        "09_final_region_full_plus_section1_full_para0.hwp",
        false,
        false,
        true,
        false,
        true,
        false,
        false,
    );
    std::fs::read(
        "output/poc/hwpx2hwp/task903/stage27_section1_probe/09_final_region_full_plus_section1_full_para0.hwp",
    )
    .expect("Stage 27 baseline HWP 읽기 실패")
}

fn task903_enable_hwp_compression(core: &mut DocumentCore) {
    let header = &mut core.document_mut().header;
    header.raw_data = None;
    header.compressed = true;
    header.flags |= 0x0000_0001;
}

fn task903_reference_header(core: &mut DocumentCore, reference: &DocumentCore) {
    core.document_mut().header = reference.document().header.clone();
}

fn task903_reference_docinfo(core: &mut DocumentCore, reference: &DocumentCore) {
    core.document_mut().doc_info = reference.document().doc_info.clone();
    core.document_mut().doc_properties = reference.document().doc_properties.clone();
}

fn task903_stream_path_for_cfb_entry_name(name: &str) -> Option<String> {
    match name {
        "Root Entry" | "BodyText" | "BinData" => None,
        "FileHeader" => Some("/FileHeader".to_string()),
        "DocInfo" => Some("/DocInfo".to_string()),
        "PrvText" => Some("/PrvText".to_string()),
        "PrvImage" => Some("/PrvImage".to_string()),
        name if name.starts_with("Section") => Some(format!("/BodyText/{}", name)),
        name if name.starts_with("BIN") => Some(format!("/BinData/{}", name)),
        name => Some(format!("/{}", name)),
    }
}

fn task903_read_raw_hwp_streams(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let cfb = LenientCfbReader::open(bytes).expect("HWP CFB 열기 실패");
    let mut streams = Vec::new();

    for (name, _, _, obj_type) in cfb.list_entries() {
        if *obj_type != 2 {
            continue;
        }
        let Some(path) = task903_stream_path_for_cfb_entry_name(name) else {
            continue;
        };
        let data = cfb
            .read_stream(name)
            .unwrap_or_else(|e| panic!("HWP CFB stream 읽기 실패 {}: {}", name, e));
        streams.push((path, data));
    }

    streams.sort_by(|a, b| a.0.cmp(&b.0));
    streams
}

fn task903_graft_raw_hwp_streams(
    base_bytes: &[u8],
    donor_bytes: &[u8],
    stream_paths: &[&str],
) -> Vec<u8> {
    let mut base_streams = task903_read_raw_hwp_streams(base_bytes);
    let donor_streams = task903_read_raw_hwp_streams(donor_bytes);

    for stream_path in stream_paths {
        let donor = donor_streams
            .iter()
            .find(|(path, _)| path == stream_path)
            .unwrap_or_else(|| panic!("donor stream 없음: {}", stream_path))
            .1
            .clone();
        let (_, base) = base_streams
            .iter_mut()
            .find(|(path, _)| path == stream_path)
            .unwrap_or_else(|| panic!("base stream 없음: {}", stream_path));
        *base = donor;
    }

    let named_streams: Vec<(&str, &[u8])> = base_streams
        .iter()
        .map(|(path, data)| (path.as_str(), data.as_slice()))
        .collect();
    mini_cfb::build_cfb(&named_streams).expect("raw HWP stream graft CFB 생성 실패")
}

#[derive(Debug, Default)]
struct Task903Stage28Changes {
    bytes: usize,
    compressed_header: bool,
    reference_header: bool,
    reference_docinfo: bool,
    raw_grafted_streams: Vec<String>,
}

fn task903_generate_stage28_probe_variant(
    output_name: &str,
    compressed_header: bool,
    reference_header: bool,
    reference_docinfo: bool,
    raw_grafted_streams: &[&str],
) -> Task903Stage28Changes {
    let base_bytes = task903_stage27_baseline_section1_bytes();
    let reference_bytes =
        std::fs::read("samples/hwpx/hancom-hwp/hwpx-h-01.hwp").expect("정답 HWP 읽기 실패");
    let reference = task903_reference_hwpx_h_01_core();

    let mut hwp_bytes = if raw_grafted_streams.is_empty() {
        let mut core = DocumentCore::from_bytes(&base_bytes).expect("Stage 28 base HWP 재로드 실패");
        if compressed_header {
            task903_enable_hwp_compression(&mut core);
        }
        if reference_header {
            task903_reference_header(&mut core, &reference);
        }
        if reference_docinfo {
            task903_reference_docinfo(&mut core, &reference);
        }
        core.export_hwp_native()
            .expect("Stage 28 HWP 직렬화 실패")
    } else {
        let mut core = DocumentCore::from_bytes(&base_bytes).expect("Stage 28 base HWP 재로드 실패");
        task903_reference_header(&mut core, &reference);
        if reference_docinfo {
            task903_reference_docinfo(&mut core, &reference);
        }
        let compressed_base = core
            .export_hwp_native()
            .expect("Stage 28 compressed base HWP 직렬화 실패");
        task903_graft_raw_hwp_streams(&compressed_base, &reference_bytes, raw_grafted_streams)
    };

    let out_dir = std::path::Path::new("output/poc/hwpx2hwp/task903/stage28_container_probe");
    std::fs::create_dir_all(out_dir).expect("Stage 28 output dir 생성 실패");
    let out_path = out_dir.join(output_name);
    std::fs::write(&out_path, &hwp_bytes).expect("Stage 28 probe 파일 저장 실패");

    let reloaded = DocumentCore::from_bytes(&hwp_bytes).expect("Stage 28 probe HWP 재로드 실패");
    assert_eq!(reloaded.document().sections.len(), 2, "{} section count", output_name);
    assert_eq!(reloaded.page_count(), 9, "{} page count", output_name);

    let changes = Task903Stage28Changes {
        bytes: hwp_bytes.len(),
        compressed_header,
        reference_header,
        reference_docinfo,
        raw_grafted_streams: raw_grafted_streams.iter().map(|s| s.to_string()).collect(),
    };

    eprintln!(
        "[#903 Stage 28] {}: bytes={}, compressed_header={}, reference_header={}, reference_docinfo={}, raw_grafted_streams={:?}, pages={}",
        out_path.display(),
        changes.bytes,
        changes.compressed_header,
        changes.reference_header,
        changes.reference_docinfo,
        changes.raw_grafted_streams,
        reloaded.page_count()
    );

    hwp_bytes.clear();
    changes
}

fn task903_force_docinfo_reserialize(core: &mut DocumentCore) {
    let doc_info = &mut core.document_mut().doc_info;
    doc_info.raw_stream = None;
    doc_info.raw_stream_dirty = true;
}

#[derive(Debug, Clone, Copy)]
enum Task903Stage29Patch {
    ReferenceModelAll,
    DocPropertiesOnly,
    FontFacesOnly,
    BorderFillsOnly,
    CharShapesOnly,
    ParaShapesOnly,
    StylesOnly,
    LayoutBundle,
    TabsNumberingBullets,
    BinDataListOnly,
    ExtraRecordsOnly,
    CountsExtraRecordsOnly,
}

impl Task903Stage29Patch {
    fn label(self) -> &'static str {
        match self {
            Task903Stage29Patch::ReferenceModelAll => "reference_model_all",
            Task903Stage29Patch::DocPropertiesOnly => "doc_properties_only",
            Task903Stage29Patch::FontFacesOnly => "font_faces_only",
            Task903Stage29Patch::BorderFillsOnly => "border_fills_only",
            Task903Stage29Patch::CharShapesOnly => "char_shapes_only",
            Task903Stage29Patch::ParaShapesOnly => "para_shapes_only",
            Task903Stage29Patch::StylesOnly => "styles_only",
            Task903Stage29Patch::LayoutBundle => "layout_bundle",
            Task903Stage29Patch::TabsNumberingBullets => "tabs_numbering_bullets",
            Task903Stage29Patch::BinDataListOnly => "bin_data_list_only",
            Task903Stage29Patch::ExtraRecordsOnly => "extra_records_only",
            Task903Stage29Patch::CountsExtraRecordsOnly => "counts_extra_records_only",
        }
    }
}

#[derive(Debug, Default)]
struct Task903Stage29Changes {
    bytes: usize,
    patch_label: String,
    docinfo_forced_reserialize: bool,
}

fn task903_apply_stage29_docinfo_patch(
    core: &mut DocumentCore,
    reference: &DocumentCore,
    patch: Task903Stage29Patch,
) {
    let reference_doc = reference.document();
    let target_doc = core.document_mut();

    match patch {
        Task903Stage29Patch::ReferenceModelAll => {
            target_doc.doc_info = reference_doc.doc_info.clone();
            target_doc.doc_properties = reference_doc.doc_properties.clone();
        }
        Task903Stage29Patch::DocPropertiesOnly => {
            target_doc.doc_properties = reference_doc.doc_properties.clone();
        }
        Task903Stage29Patch::FontFacesOnly => {
            target_doc.doc_info.font_faces = reference_doc.doc_info.font_faces.clone();
        }
        Task903Stage29Patch::BorderFillsOnly => {
            target_doc.doc_info.border_fills = reference_doc.doc_info.border_fills.clone();
        }
        Task903Stage29Patch::CharShapesOnly => {
            target_doc.doc_info.char_shapes = reference_doc.doc_info.char_shapes.clone();
        }
        Task903Stage29Patch::ParaShapesOnly => {
            target_doc.doc_info.para_shapes = reference_doc.doc_info.para_shapes.clone();
        }
        Task903Stage29Patch::StylesOnly => {
            target_doc.doc_info.styles = reference_doc.doc_info.styles.clone();
        }
        Task903Stage29Patch::LayoutBundle => {
            target_doc.doc_info.border_fills = reference_doc.doc_info.border_fills.clone();
            target_doc.doc_info.char_shapes = reference_doc.doc_info.char_shapes.clone();
            target_doc.doc_info.para_shapes = reference_doc.doc_info.para_shapes.clone();
            target_doc.doc_info.styles = reference_doc.doc_info.styles.clone();
        }
        Task903Stage29Patch::TabsNumberingBullets => {
            target_doc.doc_info.tab_defs = reference_doc.doc_info.tab_defs.clone();
            target_doc.doc_info.numberings = reference_doc.doc_info.numberings.clone();
            target_doc.doc_info.bullets = reference_doc.doc_info.bullets.clone();
            target_doc.doc_info.bullet_count = reference_doc.doc_info.bullet_count;
            target_doc.doc_info.memo_shape_count = reference_doc.doc_info.memo_shape_count;
        }
        Task903Stage29Patch::BinDataListOnly => {
            target_doc.doc_info.bin_data_list = reference_doc.doc_info.bin_data_list.clone();
        }
        Task903Stage29Patch::ExtraRecordsOnly => {
            target_doc.doc_info.extra_records = reference_doc.doc_info.extra_records.clone();
        }
        Task903Stage29Patch::CountsExtraRecordsOnly => {
            target_doc.doc_info.bullet_count = reference_doc.doc_info.bullet_count;
            target_doc.doc_info.memo_shape_count = reference_doc.doc_info.memo_shape_count;
            target_doc.doc_info.extra_records = reference_doc.doc_info.extra_records.clone();
        }
    }

    task903_force_docinfo_reserialize(core);
}

fn task903_generate_stage29_probe_variant(
    output_name: &str,
    patch: Task903Stage29Patch,
) -> Task903Stage29Changes {
    let base_bytes = task903_stage27_baseline_section1_bytes();
    let mut core = DocumentCore::from_bytes(&base_bytes).expect("Stage 29 base HWP 재로드 실패");
    let reference = task903_reference_hwpx_h_01_core();

    task903_enable_hwp_compression(&mut core);
    task903_apply_stage29_docinfo_patch(&mut core, &reference, patch);

    let hwp_bytes = core.export_hwp_native().expect("Stage 29 HWP 직렬화 실패");
    let out_dir = std::path::Path::new("output/poc/hwpx2hwp/task903/stage29_docinfo_probe");
    std::fs::create_dir_all(out_dir).expect("Stage 29 output dir 생성 실패");
    let out_path = out_dir.join(output_name);
    std::fs::write(&out_path, &hwp_bytes).expect("Stage 29 probe 파일 저장 실패");

    let reloaded = DocumentCore::from_bytes(&hwp_bytes).expect("Stage 29 probe HWP 재로드 실패");
    assert_eq!(reloaded.document().sections.len(), 2, "{} section count", output_name);
    assert_eq!(reloaded.page_count(), 9, "{} page count", output_name);

    let changes = Task903Stage29Changes {
        bytes: hwp_bytes.len(),
        patch_label: patch.label().to_string(),
        docinfo_forced_reserialize: true,
    };

    eprintln!(
        "[#903 Stage 29] {}: bytes={}, patch={}, docinfo_forced_reserialize={}, pages={}, docinfo_counts=(bin={}, fonts={}, bf={}, cs={}, tab={}, num={}, bullet={}, ps={}, style={}, extra={})",
        out_path.display(),
        changes.bytes,
        changes.patch_label,
        changes.docinfo_forced_reserialize,
        reloaded.page_count(),
        reloaded.document().doc_info.bin_data_list.len(),
        reloaded.document().doc_info.font_faces.iter().map(|v| v.len()).sum::<usize>(),
        reloaded.document().doc_info.border_fills.len(),
        reloaded.document().doc_info.char_shapes.len(),
        reloaded.document().doc_info.tab_defs.len(),
        reloaded.document().doc_info.numberings.len(),
        reloaded.document().doc_info.bullets.len(),
        reloaded.document().doc_info.para_shapes.len(),
        reloaded.document().doc_info.styles.len(),
        reloaded.document().doc_info.extra_records.len(),
    );

    changes
}

fn task903_clear_docinfo_item_raw_data(core: &mut DocumentCore) {
    let doc = core.document_mut();
    doc.doc_properties.raw_data = None;
    for bin_data in &mut doc.doc_info.bin_data_list {
        bin_data.raw_data = None;
    }
    for fonts in &mut doc.doc_info.font_faces {
        for font in fonts {
            font.raw_data = None;
        }
    }
    for border_fill in &mut doc.doc_info.border_fills {
        border_fill.raw_data = None;
    }
    for char_shape in &mut doc.doc_info.char_shapes {
        char_shape.raw_data = None;
    }
    for tab_def in &mut doc.doc_info.tab_defs {
        tab_def.raw_data = None;
    }
    for numbering in &mut doc.doc_info.numberings {
        numbering.raw_data = None;
    }
    for bullet in &mut doc.doc_info.bullets {
        bullet.raw_data = None;
    }
    for para_shape in &mut doc.doc_info.para_shapes {
        para_shape.raw_data = None;
    }
    for style in &mut doc.doc_info.styles {
        style.raw_data = None;
    }
}

#[derive(Debug, Clone, Copy)]
enum Task903Stage30Patch {
    SectionCountOnly,
    DocPropertiesValuesNoRaw,
    ParaShapesNoRawOnly,
    SectionCountParaShapesRaw,
    SectionCountParaShapesNoRaw,
    SectionCountLayoutBundleRaw,
    SectionCountLayoutBundleNoRaw,
    SectionCountReferenceModelAllNoRaw,
}

impl Task903Stage30Patch {
    fn label(self) -> &'static str {
        match self {
            Task903Stage30Patch::SectionCountOnly => "section_count_only",
            Task903Stage30Patch::DocPropertiesValuesNoRaw => "doc_properties_values_no_raw",
            Task903Stage30Patch::ParaShapesNoRawOnly => "para_shapes_no_raw_only",
            Task903Stage30Patch::SectionCountParaShapesRaw => "section_count_para_shapes_raw",
            Task903Stage30Patch::SectionCountParaShapesNoRaw => "section_count_para_shapes_no_raw",
            Task903Stage30Patch::SectionCountLayoutBundleRaw => "section_count_layout_bundle_raw",
            Task903Stage30Patch::SectionCountLayoutBundleNoRaw => {
                "section_count_layout_bundle_no_raw"
            }
            Task903Stage30Patch::SectionCountReferenceModelAllNoRaw => {
                "section_count_reference_model_all_no_raw"
            }
        }
    }
}

#[derive(Debug, Default)]
struct Task903Stage30Changes {
    bytes: usize,
    patch_label: String,
    section_count: u16,
    doc_properties_raw: bool,
    para_shape_raw_count: usize,
}

fn task903_set_actual_section_count(core: &mut DocumentCore) {
    let section_count = core.document().sections.len() as u16;
    core.document_mut().doc_properties.section_count = section_count;
    core.document_mut().doc_properties.raw_data = None;
}

fn task903_copy_reference_para_shapes(
    core: &mut DocumentCore,
    reference: &DocumentCore,
    clear_raw_data: bool,
) {
    core.document_mut().doc_info.para_shapes =
        reference.document().doc_info.para_shapes.clone();
    if clear_raw_data {
        for para_shape in &mut core.document_mut().doc_info.para_shapes {
            para_shape.raw_data = None;
        }
    }
}

fn task903_copy_reference_layout_bundle(
    core: &mut DocumentCore,
    reference: &DocumentCore,
    clear_raw_data: bool,
) {
    let reference_doc = reference.document();
    let target_doc = core.document_mut();
    target_doc.doc_info.border_fills = reference_doc.doc_info.border_fills.clone();
    target_doc.doc_info.char_shapes = reference_doc.doc_info.char_shapes.clone();
    target_doc.doc_info.para_shapes = reference_doc.doc_info.para_shapes.clone();
    target_doc.doc_info.styles = reference_doc.doc_info.styles.clone();
    if clear_raw_data {
        for border_fill in &mut target_doc.doc_info.border_fills {
            border_fill.raw_data = None;
        }
        for char_shape in &mut target_doc.doc_info.char_shapes {
            char_shape.raw_data = None;
        }
        for para_shape in &mut target_doc.doc_info.para_shapes {
            para_shape.raw_data = None;
        }
        for style in &mut target_doc.doc_info.styles {
            style.raw_data = None;
        }
    }
}

fn task903_apply_stage30_patch(
    core: &mut DocumentCore,
    reference: &DocumentCore,
    patch: Task903Stage30Patch,
) {
    match patch {
        Task903Stage30Patch::SectionCountOnly => {
            task903_set_actual_section_count(core);
        }
        Task903Stage30Patch::DocPropertiesValuesNoRaw => {
            core.document_mut().doc_properties = reference.document().doc_properties.clone();
            core.document_mut().doc_properties.raw_data = None;
        }
        Task903Stage30Patch::ParaShapesNoRawOnly => {
            task903_copy_reference_para_shapes(core, reference, true);
        }
        Task903Stage30Patch::SectionCountParaShapesRaw => {
            task903_set_actual_section_count(core);
            task903_copy_reference_para_shapes(core, reference, false);
        }
        Task903Stage30Patch::SectionCountParaShapesNoRaw => {
            task903_set_actual_section_count(core);
            task903_copy_reference_para_shapes(core, reference, true);
        }
        Task903Stage30Patch::SectionCountLayoutBundleRaw => {
            task903_set_actual_section_count(core);
            task903_copy_reference_layout_bundle(core, reference, false);
        }
        Task903Stage30Patch::SectionCountLayoutBundleNoRaw => {
            task903_set_actual_section_count(core);
            task903_copy_reference_layout_bundle(core, reference, true);
        }
        Task903Stage30Patch::SectionCountReferenceModelAllNoRaw => {
            core.document_mut().doc_info = reference.document().doc_info.clone();
            core.document_mut().doc_properties = reference.document().doc_properties.clone();
            task903_clear_docinfo_item_raw_data(core);
        }
    }

    task903_force_docinfo_reserialize(core);
}

fn task903_generate_stage30_probe_variant(
    output_name: &str,
    patch: Task903Stage30Patch,
) -> Task903Stage30Changes {
    let base_bytes = task903_stage27_baseline_section1_bytes();
    let mut core = DocumentCore::from_bytes(&base_bytes).expect("Stage 30 base HWP 재로드 실패");
    let reference = task903_reference_hwpx_h_01_core();

    task903_enable_hwp_compression(&mut core);
    task903_apply_stage30_patch(&mut core, &reference, patch);

    let hwp_bytes = core.export_hwp_native().expect("Stage 30 HWP 직렬화 실패");
    let out_dir = std::path::Path::new("output/poc/hwpx2hwp/task903/stage30_minimal_docinfo_probe");
    std::fs::create_dir_all(out_dir).expect("Stage 30 output dir 생성 실패");
    let out_path = out_dir.join(output_name);
    std::fs::write(&out_path, &hwp_bytes).expect("Stage 30 probe 파일 저장 실패");

    let reloaded = DocumentCore::from_bytes(&hwp_bytes).expect("Stage 30 probe HWP 재로드 실패");
    assert_eq!(reloaded.document().sections.len(), 2, "{} section count", output_name);
    assert_eq!(reloaded.page_count(), 9, "{} page count", output_name);

    let changes = Task903Stage30Changes {
        bytes: hwp_bytes.len(),
        patch_label: patch.label().to_string(),
        section_count: reloaded.document().doc_properties.section_count,
        doc_properties_raw: reloaded.document().doc_properties.raw_data.is_some(),
        para_shape_raw_count: reloaded
            .document()
            .doc_info
            .para_shapes
            .iter()
            .filter(|ps| ps.raw_data.is_some())
            .count(),
    };

    eprintln!(
        "[#903 Stage 30] {}: bytes={}, patch={}, section_count={}, doc_properties_raw={}, para_shape_raw_count={}, pages={}",
        out_path.display(),
        changes.bytes,
        changes.patch_label,
        changes.section_count,
        changes.doc_properties_raw,
        changes.para_shape_raw_count,
        reloaded.page_count()
    );

    changes
}

fn assert_approx_f64(label: &str, left: f64, right: f64) {
    let delta = (left - right).abs();
    assert!(
        delta <= 0.000_001,
        "{}: left={} right={} delta={}",
        label,
        left,
        right,
        delta
    );
}

#[test]
fn task903_hwpx_h_01_embedded_bindata_survives_hwp_save_reload() {
    if let Ok(reference_bytes) = std::fs::read("samples/hwpx/hancom-hwp/hwpx-h-01.hwp") {
        let reference =
            DocumentCore::from_bytes(&reference_bytes).expect("한컴 정답 HWP 파싱 실패");
        assert_hwp_embedded_bindata_loaded("hancom reference", &reference);
    } else {
        eprintln!("[skip] samples/hwpx/hancom-hwp/hwpx-h-01.hwp 없음");
    }

    let bytes = load_sample("hwpx-h-01.hwpx");
    let mut core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");
    assert_eq!(
        core.document().bin_data_content.len(),
        5,
        "HWPX 원본은 embedded image payload 5개를 로드해야 함"
    );

    let hwp_bytes = core.export_hwp_with_adapter().expect("HWP 직렬화 실패");
    let reloaded = DocumentCore::from_bytes(&hwp_bytes).expect("HWP 재로드 실패");

    assert_hwp_embedded_bindata_loaded("rhwp exported hwp", &reloaded);
}

#[test]
fn task903_hwpx_h_01_preserves_xml_entity_text() {
    let bytes = load_sample("hwpx-h-01.hwpx");
    let core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");
    let section = &core.document().sections[0];

    assert_eq!(
        section.paragraphs[9].text, "< 분기별 해외직접투자액 추이(억 달러, 전년동기 대비, %) >",
        "HWPX XML entity로 들어온 제목 꺾쇠 문자를 보존해야 함"
    );
    assert_eq!(
        section.paragraphs[13].text, "< 업종별 동향(억 달러, %) >",
        "HWPX XML entity로 들어온 다음 표 제목 꺾쇠 문자를 보존해야 함"
    );
}

#[test]
fn task903_hwpx_h_01_group_picture_current_size_is_materialized() {
    let bytes = load_sample("hwpx-h-01.hwpx");
    let core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");
    let para = &core.document().sections[0].paragraphs[29];
    let group = match &para.controls[0] {
        Control::Shape(shape) => match shape.as_ref() {
            ShapeObject::Group(group) => group,
            other => panic!("문단 0:29는 묶음이어야 함: {:?}", other),
        },
        other => panic!("문단 0:29 첫 컨트롤은 shape이어야 함: {:?}", other),
    };

    assert_eq!(group.children.len(), 3, "묶음 내부 그림 3개 기대");
    assert!(
        group.shape_attr.current_width > 0 && group.shape_attr.current_height > 0,
        "묶음 자체 current size도 orgSz에서 materialize되어야 함"
    );
    for (idx, child) in group.children.iter().enumerate() {
        let picture = match child {
            ShapeObject::Picture(pic) => pic,
            other => panic!("child[{}]는 그림이어야 함: {:?}", idx, other),
        };
        assert!(
            picture.shape_attr.original_width > 0 && picture.shape_attr.original_height > 0,
            "child[{}] original size must be present",
            idx
        );
        assert!(
            picture.shape_attr.current_width > 0 && picture.shape_attr.current_height > 0,
            "child[{}] current size must be materialized from orgSz when curSz is zero",
            idx
        );
    }
}

#[test]
fn task903_hwpx_h_01_rendering_info_matrix_survives_hwp_save_reload() {
    let bytes = load_sample("hwpx-h-01.hwpx");
    let mut core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");
    let source_matrices: Vec<(f64, f64, f64, f64, f64, f64)> =
        task903_hwpx_h_01_group_child_shape_attrs(&core)
            .iter()
            .map(|sa| {
                (
                    sa.render_sx,
                    sa.render_b,
                    sa.render_tx,
                    sa.render_c,
                    sa.render_sy,
                    sa.render_ty,
                )
            })
            .collect();

    assert_eq!(source_matrices.len(), 3, "묶음 내부 그림 3개 기대");
    assert!(
        source_matrices
            .iter()
            .any(|(sx, _, _, _, sy, _)| (*sx - 1.0).abs() > 0.000_001
                || (*sy - 1.0).abs() > 0.000_001),
        "HWPX 원본에는 identity가 아닌 renderingInfo scale이 있어야 함"
    );

    let hwp_bytes = core.export_hwp_with_adapter().expect("HWP 직렬화 실패");
    let reloaded = DocumentCore::from_bytes(&hwp_bytes).expect("HWP 재로드 실패");
    let reloaded_attrs = task903_hwpx_h_01_group_child_shape_attrs(&reloaded);

    assert_eq!(reloaded_attrs.len(), source_matrices.len());
    for (idx, (actual, expected)) in reloaded_attrs
        .iter()
        .zip(source_matrices.iter())
        .enumerate()
    {
        let (sx, b, tx, c, sy, ty) = expected;
        assert_approx_f64(&format!("child[{}].render_sx", idx), actual.render_sx, *sx);
        assert_approx_f64(&format!("child[{}].render_b", idx), actual.render_b, *b);
        assert_approx_f64(&format!("child[{}].render_tx", idx), actual.render_tx, *tx);
        assert_approx_f64(&format!("child[{}].render_c", idx), actual.render_c, *c);
        assert_approx_f64(&format!("child[{}].render_sy", idx), actual.render_sy, *sy);
        assert_approx_f64(&format!("child[{}].render_ty", idx), actual.render_ty, *ty);
    }
}

#[test]
fn task903_hwpx_h_01_table_cell_picture_common_attrs_survive_hwp_save_reload() {
    let bytes = load_sample("hwpx-h-01.hwpx");
    let mut core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");
    let source_attrs: Vec<CommonObjAttr> =
        task903_hwpx_h_01_first_table_picture_common_attrs(&core)
            .into_iter()
            .cloned()
            .collect();

    assert_eq!(source_attrs.len(), 2, "첫 표의 셀 그림 2개 기대");
    assert!(
        source_attrs
            .iter()
            .all(|attr| attr.treat_as_char && matches!(attr.text_wrap, TextWrap::TopAndBottom)),
        "HWPX 원본 셀 그림은 글자처럼/자리차지 배치여야 함"
    );
    assert!(matches!(source_attrs[0].vert_rel_to, VertRelTo::Para));
    assert!(matches!(source_attrs[0].horz_rel_to, HorzRelTo::Para));
    assert!(matches!(source_attrs[1].vert_rel_to, VertRelTo::Para));
    assert!(matches!(source_attrs[1].horz_rel_to, HorzRelTo::Column));

    let hwp_bytes = core.export_hwp_with_adapter().expect("HWP 직렬화 실패");
    let reloaded = DocumentCore::from_bytes(&hwp_bytes).expect("HWP 재로드 실패");
    let reloaded_attrs = task903_hwpx_h_01_first_table_picture_common_attrs(&reloaded);

    assert_eq!(reloaded_attrs.len(), source_attrs.len());
    for (idx, (actual, expected)) in reloaded_attrs.iter().zip(source_attrs.iter()).enumerate() {
        assert_eq!(
            actual.treat_as_char, expected.treat_as_char,
            "cell picture[{}].treat_as_char",
            idx
        );
        assert_eq!(
            actual.text_wrap, expected.text_wrap,
            "cell picture[{}].text_wrap",
            idx
        );
        assert_eq!(
            actual.vert_rel_to, expected.vert_rel_to,
            "cell picture[{}].vert_rel_to",
            idx
        );
        assert_eq!(
            actual.horz_rel_to, expected.horz_rel_to,
            "cell picture[{}].horz_rel_to",
            idx
        );
    }
}

#[test]
fn task903_hwpx_h_01_first_table_hancom_layout_attrs_are_materialized() {
    let bytes = load_sample("hwpx-h-01.hwpx");
    let mut core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");

    let hwp_bytes = core.export_hwp_with_adapter().expect("HWP 직렬화 실패");
    let reloaded = DocumentCore::from_bytes(&hwp_bytes).expect("HWP 재로드 실패");
    let table = task903_hwpx_h_01_first_table(&reloaded);

    assert_eq!(
        table.page_break,
        TablePageBreak::RowBreak,
        "첫 표 page_break는 한컴 정답 HWP와 같은 RowBreak로 materialize되어야 함"
    );
    assert_eq!(
        (
            table.outer_margin_left,
            table.outer_margin_right,
            table.outer_margin_top,
            table.outer_margin_bottom,
        ),
        (283, 283, 283, 283),
        "첫 표 outer_margin 1mm가 HWP 저장/재로드 후 유지되어야 함"
    );
}

#[test]
fn task903_hwpx_h_01_page1_flow_tables_common_attrs_survive_hwp_save_reload() {
    let bytes = load_sample("hwpx-h-01.hwpx");
    let mut core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");
    let table_locations = [10usize, 14, 21];
    let source_attrs: Vec<(TextWrap, VertRelTo, HorzRelTo, (i16, i16, i16, i16))> = table_locations
        .iter()
        .map(|para_idx| {
            let table = task903_hwpx_h_01_table_at(&core, *para_idx, 0);
            (
                table.common.text_wrap,
                table.common.vert_rel_to,
                table.common.horz_rel_to,
                (
                    table.outer_margin_left,
                    table.outer_margin_right,
                    table.outer_margin_top,
                    table.outer_margin_bottom,
                ),
            )
        })
        .collect();

    for (idx, (text_wrap, vert_rel, horz_rel, margin)) in source_attrs.iter().enumerate() {
        assert!(
            matches!(text_wrap, TextWrap::TopAndBottom),
            "source table[{}] must be TopAndBottom",
            idx
        );
        assert!(
            matches!(vert_rel, VertRelTo::Para),
            "source table[{}] must be vert=Para",
            idx
        );
        assert!(
            matches!(horz_rel, HorzRelTo::Para),
            "source table[{}] must be horz=Para",
            idx
        );
        assert_ne!(*margin, (0, 0, 0, 0), "source table[{}] margin", idx);
    }

    let hwp_bytes = core.export_hwp_with_adapter().expect("HWP 직렬화 실패");
    let reloaded = DocumentCore::from_bytes(&hwp_bytes).expect("HWP 재로드 실패");

    for (idx, para_idx) in table_locations.iter().enumerate() {
        let actual = task903_hwpx_h_01_table_at(&reloaded, *para_idx, 0);
        let (expected_wrap, expected_vert, expected_horz, expected_margin) = source_attrs[idx];
        assert_eq!(
            actual.common.text_wrap, expected_wrap,
            "문단 0:{} 표 text_wrap",
            para_idx
        );
        assert_eq!(
            actual.common.vert_rel_to, expected_vert,
            "문단 0:{} 표 vert_rel_to",
            para_idx
        );
        assert_eq!(
            actual.common.horz_rel_to, expected_horz,
            "문단 0:{} 표 horz_rel_to",
            para_idx
        );
        assert_eq!(
            (
                actual.outer_margin_left,
                actual.outer_margin_right,
                actual.outer_margin_top,
                actual.outer_margin_bottom,
            ),
            expected_margin,
            "문단 0:{} 표 outer_margin",
            para_idx
        );
    }
}

#[test]
fn task903_stage7_generate_record_tail_probe_variants() {
    let (_bytes1, cells1, tables1, paras1, sections1) = task903_generate_stage7_probe_variant(
        "01_cell_list_header_tail_13.hwp",
        true,
        false,
        false,
        false,
    );
    assert!(
        cells1 > 0,
        "variant 01 must materialize cell LIST_HEADER tails"
    );
    assert_eq!(tables1, 0, "variant 01 must not materialize TABLE tails");
    assert_eq!(
        paras1, 0,
        "variant 01 must not materialize PARA_HEADER tails"
    );
    assert_eq!(
        sections1, 0,
        "variant 01 must not materialize SectionDef tail"
    );

    let (_bytes2, cells2, tables2, paras2, sections2) = task903_generate_stage7_probe_variant(
        "02_table_record_tail_2.hwp",
        false,
        true,
        false,
        false,
    );
    assert_eq!(
        cells2, 0,
        "variant 02 must not materialize cell LIST_HEADER tails"
    );
    assert!(tables2 > 0, "variant 02 must materialize TABLE tails");
    assert_eq!(
        paras2, 0,
        "variant 02 must not materialize PARA_HEADER tails"
    );
    assert_eq!(
        sections2, 0,
        "variant 02 must not materialize SectionDef tail"
    );

    let (_bytes3, cells3, tables3, paras3, sections3) = task903_generate_stage7_probe_variant(
        "03_cell_list_header_tail_13_plus_table_tail_2.hwp",
        true,
        true,
        false,
        false,
    );
    assert!(
        cells3 > 0,
        "variant 03 must materialize cell LIST_HEADER tails"
    );
    assert!(tables3 > 0, "variant 03 must materialize TABLE tails");
    assert_eq!(
        paras3, 0,
        "variant 03 must not materialize PARA_HEADER tails"
    );
    assert_eq!(
        sections3, 0,
        "variant 03 must not materialize SectionDef tail"
    );

    let (_bytes4, cells4, tables4, paras4, sections4) = task903_generate_stage7_probe_variant(
        "04_para_header_tail_2.hwp",
        false,
        false,
        true,
        false,
    );
    assert_eq!(
        cells4, 0,
        "variant 04 must not materialize cell LIST_HEADER tails"
    );
    assert_eq!(tables4, 0, "variant 04 must not materialize TABLE tails");
    assert!(paras4 > 0, "variant 04 must materialize PARA_HEADER tails");
    assert_eq!(
        sections4, 0,
        "variant 04 must not materialize SectionDef tail"
    );

    let (_bytes5, cells5, tables5, paras5, sections5) = task903_generate_stage7_probe_variant(
        "05_section_def_ctrl_header_tail_19.hwp",
        false,
        false,
        false,
        true,
    );
    assert_eq!(
        cells5, 0,
        "variant 05 must not materialize cell LIST_HEADER tails"
    );
    assert_eq!(tables5, 0, "variant 05 must not materialize TABLE tails");
    assert_eq!(
        paras5, 0,
        "variant 05 must not materialize PARA_HEADER tails"
    );
    assert!(
        sections5 > 0,
        "variant 05 must materialize SectionDef CTRL_HEADER tail"
    );
}

#[test]
fn task903_stage8_generate_core_field_probe_variants() {
    let (_bytes1, sections1, paras1, first_cells1, first_pictures1) =
        task903_generate_stage8_probe_variant(
            "01_section_def_core_fields.hwp",
            true,
            false,
            false,
            false,
        );
    assert!(
        sections1 > 0,
        "variant 01 must materialize SectionDef core fields"
    );
    assert_eq!(
        paras1, 0,
        "variant 01 must not materialize PARA_HEADER tails"
    );
    assert_eq!(
        first_cells1, 0,
        "variant 01 must not materialize first cell tail"
    );
    assert_eq!(
        first_pictures1, 0,
        "variant 01 must not materialize first picture common"
    );

    let (_bytes2, sections2, paras2, first_cells2, first_pictures2) =
        task903_generate_stage8_probe_variant(
            "02_section_def_core_plus_para_header_tail.hwp",
            true,
            true,
            false,
            false,
        );
    assert!(
        sections2 > 0,
        "variant 02 must materialize SectionDef core fields"
    );
    assert!(paras2 > 0, "variant 02 must materialize PARA_HEADER tails");
    assert_eq!(
        first_cells2, 0,
        "variant 02 must not materialize first cell tail"
    );
    assert_eq!(
        first_pictures2, 0,
        "variant 02 must not materialize first picture common"
    );

    let (_bytes3, sections3, paras3, first_cells3, first_pictures3) =
        task903_generate_stage8_probe_variant(
            "03_first_cell_list_header_65.hwp",
            false,
            false,
            true,
            false,
        );
    assert_eq!(
        sections3, 0,
        "variant 03 must not materialize SectionDef core fields"
    );
    assert_eq!(
        paras3, 0,
        "variant 03 must not materialize PARA_HEADER tails"
    );
    assert!(
        first_cells3 > 0,
        "variant 03 must materialize first cell 65B tail"
    );
    assert_eq!(
        first_pictures3, 0,
        "variant 03 must not materialize first picture common"
    );

    let (_bytes4, sections4, paras4, first_cells4, first_pictures4) =
        task903_generate_stage8_probe_variant(
            "04_section_def_core_plus_first_cell_65.hwp",
            true,
            false,
            true,
            false,
        );
    assert!(
        sections4 > 0,
        "variant 04 must materialize SectionDef core fields"
    );
    assert_eq!(
        paras4, 0,
        "variant 04 must not materialize PARA_HEADER tails"
    );
    assert!(
        first_cells4 > 0,
        "variant 04 must materialize first cell 65B tail"
    );
    assert_eq!(
        first_pictures4, 0,
        "variant 04 must not materialize first picture common"
    );

    let (_bytes5, sections5, paras5, first_cells5, first_pictures5) =
        task903_generate_stage8_probe_variant(
            "05_first_picture_common_from_reference.hwp",
            false,
            false,
            false,
            true,
        );
    assert_eq!(
        sections5, 0,
        "variant 05 must not materialize SectionDef core fields"
    );
    assert_eq!(
        paras5, 0,
        "variant 05 must not materialize PARA_HEADER tails"
    );
    assert_eq!(
        first_cells5, 0,
        "variant 05 must not materialize first cell tail"
    );
    assert!(
        first_pictures5 > 0,
        "variant 05 must materialize first picture common"
    );

    let (_bytes6, sections6, paras6, first_cells6, first_pictures6) =
        task903_generate_stage8_probe_variant(
            "06_section_def_first_cell_picture_common.hwp",
            true,
            false,
            true,
            true,
        );
    assert!(
        sections6 > 0,
        "variant 06 must materialize SectionDef core fields"
    );
    assert_eq!(
        paras6, 0,
        "variant 06 must not materialize PARA_HEADER tails"
    );
    assert!(
        first_cells6 > 0,
        "variant 06 must materialize first cell 65B tail"
    );
    assert!(
        first_pictures6 > 0,
        "variant 06 must materialize first picture common"
    );
}

#[test]
fn task903_stage9_generate_first_table_payload_probe_variants() {
    let changes1 = task903_generate_stage9_probe_variant(
        "01_first_table_ctrl_header_from_reference.hwp",
        true,
        false,
        false,
        false,
        false,
    );
    assert!(
        changes1.table_ctrl_headers > 0,
        "variant 01 must copy first table CTRL_HEADER"
    );
    assert_eq!(
        changes1.cell_list_headers, 0,
        "variant 01 must not copy cell LIST_HEADER payloads"
    );
    assert_eq!(
        changes1.table_records, 0,
        "variant 01 must not copy TABLE record payload"
    );
    assert_eq!(
        changes1.cell_para_headers, 0,
        "variant 01 must not copy cell PARA_HEADER payloads"
    );
    assert_eq!(
        changes1.picture_records, 0,
        "variant 01 must not copy picture records"
    );

    let changes2 = task903_generate_stage9_probe_variant(
        "02_first_table_all_cell_list_headers_from_reference.hwp",
        false,
        true,
        false,
        false,
        false,
    );
    assert_eq!(
        changes2.table_ctrl_headers, 0,
        "variant 02 must not copy first table CTRL_HEADER"
    );
    assert!(
        changes2.cell_list_headers > 0,
        "variant 02 must copy cell LIST_HEADER payloads"
    );
    assert_eq!(
        changes2.table_records, 0,
        "variant 02 must not copy TABLE record payload"
    );
    assert_eq!(
        changes2.cell_para_headers, 0,
        "variant 02 must not copy cell PARA_HEADER payloads"
    );
    assert_eq!(
        changes2.picture_records, 0,
        "variant 02 must not copy picture records"
    );

    let changes3 = task903_generate_stage9_probe_variant(
        "03_first_table_table_record_from_reference.hwp",
        false,
        false,
        true,
        false,
        false,
    );
    assert_eq!(
        changes3.table_ctrl_headers, 0,
        "variant 03 must not copy first table CTRL_HEADER"
    );
    assert_eq!(
        changes3.cell_list_headers, 0,
        "variant 03 must not copy cell LIST_HEADER payloads"
    );
    assert!(
        changes3.table_records > 0,
        "variant 03 must copy TABLE record payload"
    );
    assert_eq!(
        changes3.cell_para_headers, 0,
        "variant 03 must not copy cell PARA_HEADER payloads"
    );
    assert_eq!(
        changes3.picture_records, 0,
        "variant 03 must not copy picture records"
    );

    let changes4 = task903_generate_stage9_probe_variant(
        "04_first_table_cell_para_header_from_reference.hwp",
        false,
        false,
        false,
        true,
        false,
    );
    assert_eq!(
        changes4.table_ctrl_headers, 0,
        "variant 04 must not copy first table CTRL_HEADER"
    );
    assert_eq!(
        changes4.cell_list_headers, 0,
        "variant 04 must not copy cell LIST_HEADER payloads"
    );
    assert_eq!(
        changes4.table_records, 0,
        "variant 04 must not copy TABLE record payload"
    );
    assert!(
        changes4.cell_para_headers > 0,
        "variant 04 must copy cell PARA_HEADER payloads"
    );
    assert_eq!(
        changes4.picture_records, 0,
        "variant 04 must not copy picture records"
    );

    let changes5 = task903_generate_stage9_probe_variant(
        "05_first_table_structural_payload_bundle.hwp",
        true,
        true,
        true,
        true,
        false,
    );
    assert!(
        changes5.table_ctrl_headers > 0,
        "variant 05 must copy first table CTRL_HEADER"
    );
    assert!(
        changes5.cell_list_headers > 0,
        "variant 05 must copy cell LIST_HEADER payloads"
    );
    assert!(
        changes5.table_records > 0,
        "variant 05 must copy TABLE record payload"
    );
    assert!(
        changes5.cell_para_headers > 0,
        "variant 05 must copy cell PARA_HEADER payloads"
    );
    assert_eq!(
        changes5.picture_records, 0,
        "variant 05 must not copy picture records"
    );

    let changes6 = task903_generate_stage9_probe_variant(
        "06_first_table_picture_records_from_reference.hwp",
        false,
        false,
        false,
        false,
        true,
    );
    assert_eq!(
        changes6.table_ctrl_headers, 0,
        "variant 06 must not copy first table CTRL_HEADER"
    );
    assert_eq!(
        changes6.cell_list_headers, 0,
        "variant 06 must not copy cell LIST_HEADER payloads"
    );
    assert_eq!(
        changes6.table_records, 0,
        "variant 06 must not copy TABLE record payload"
    );
    assert_eq!(
        changes6.cell_para_headers, 0,
        "variant 06 must not copy cell PARA_HEADER payloads"
    );
    assert!(
        changes6.picture_records > 0,
        "variant 06 must copy picture records"
    );
}

#[test]
fn task903_stage10_generate_minimal_read_error_probe_variants() {
    let changes1 = task903_generate_stage10_probe_variant(
        "01_first_picture_0_common_only.hwp",
        &[(0, Task903FirstTablePicturePatch::Common)],
    );
    assert!(
        changes1.base_sections > 0,
        "variant 01 must apply section core base"
    );
    assert!(
        changes1.base_first_cells > 0,
        "variant 01 must apply first cell base"
    );
    assert_eq!(
        changes1.common_records, 1,
        "variant 01 must copy picture[0] common"
    );
    assert_eq!(
        changes1.shape_components, 0,
        "variant 01 must not copy shape component"
    );
    assert_eq!(
        changes1.sc_pictures, 0,
        "variant 01 must not copy SC_PICTURE"
    );
    assert_eq!(
        changes1.full_pictures, 0,
        "variant 01 must not copy full picture"
    );

    let changes2 = task903_generate_stage10_probe_variant(
        "02_first_picture_0_shape_component_only.hwp",
        &[(0, Task903FirstTablePicturePatch::ShapeComponent)],
    );
    assert_eq!(
        changes2.common_records, 0,
        "variant 02 must not copy common"
    );
    assert_eq!(
        changes2.shape_components, 1,
        "variant 02 must copy picture[0] shape component"
    );
    assert_eq!(
        changes2.sc_pictures, 0,
        "variant 02 must not copy SC_PICTURE"
    );
    assert_eq!(
        changes2.full_pictures, 0,
        "variant 02 must not copy full picture"
    );

    let changes3 = task903_generate_stage10_probe_variant(
        "03_first_picture_0_sc_picture_only.hwp",
        &[(0, Task903FirstTablePicturePatch::ScPicture)],
    );
    assert_eq!(
        changes3.common_records, 0,
        "variant 03 must not copy common"
    );
    assert_eq!(
        changes3.shape_components, 0,
        "variant 03 must not copy shape component"
    );
    assert_eq!(
        changes3.sc_pictures, 1,
        "variant 03 must copy picture[0] SC_PICTURE payload"
    );
    assert_eq!(
        changes3.full_pictures, 0,
        "variant 03 must not copy full picture"
    );

    let changes4 = task903_generate_stage10_probe_variant(
        "04_first_picture_0_full.hwp",
        &[(0, Task903FirstTablePicturePatch::Full)],
    );
    assert_eq!(
        changes4.common_records, 0,
        "variant 04 counts full separately"
    );
    assert_eq!(
        changes4.shape_components, 0,
        "variant 04 counts full separately"
    );
    assert_eq!(changes4.sc_pictures, 0, "variant 04 counts full separately");
    assert_eq!(
        changes4.full_pictures, 1,
        "variant 04 must copy picture[0] full record"
    );

    let changes5 = task903_generate_stage10_probe_variant(
        "05_first_picture_1_full.hwp",
        &[(1, Task903FirstTablePicturePatch::Full)],
    );
    assert_eq!(
        changes5.full_pictures, 1,
        "variant 05 must copy picture[1] full record"
    );

    let changes6 = task903_generate_stage10_probe_variant(
        "06_first_picture_0_and_1_full.hwp",
        &[
            (0, Task903FirstTablePicturePatch::Full),
            (1, Task903FirstTablePicturePatch::Full),
        ],
    );
    assert_eq!(
        changes6.full_pictures, 2,
        "variant 06 must copy both first-table picture records"
    );
}

#[test]
fn task903_stage11_generate_picture_structural_combo_probe_variants() {
    let changes1 = task903_generate_stage11_probe_variant(
        "01_picture_full_plus_table_ctrl_header.hwp",
        true,
        false,
        false,
        false,
        false,
    );
    assert_eq!(
        changes1.base_full_pictures, 2,
        "variant 01 must copy both picture records as base"
    );
    assert!(
        changes1.table_ctrl_headers > 0,
        "variant 01 must copy first table CTRL_HEADER"
    );
    assert_eq!(
        changes1.cell_list_headers, 0,
        "variant 01 must not copy cell LIST_HEADER payloads"
    );
    assert_eq!(
        changes1.table_records, 0,
        "variant 01 must not copy TABLE record payload"
    );
    assert_eq!(
        changes1.cell_para_headers, 0,
        "variant 01 must not copy cell PARA_HEADER payloads"
    );
    assert_eq!(
        changes1.ctrl_data_paragraphs, 0,
        "variant 01 must not copy ctrl_data_records"
    );

    let changes2 = task903_generate_stage11_probe_variant(
        "02_picture_full_plus_all_cell_list_headers.hwp",
        false,
        true,
        false,
        false,
        false,
    );
    assert_eq!(
        changes2.base_full_pictures, 2,
        "variant 02 must copy both picture records as base"
    );
    assert_eq!(
        changes2.table_ctrl_headers, 0,
        "variant 02 must not copy first table CTRL_HEADER"
    );
    assert!(
        changes2.cell_list_headers > 0,
        "variant 02 must copy cell LIST_HEADER payloads"
    );
    assert_eq!(
        changes2.table_records, 0,
        "variant 02 must not copy TABLE record payload"
    );
    assert_eq!(
        changes2.cell_para_headers, 0,
        "variant 02 must not copy cell PARA_HEADER payloads"
    );
    assert_eq!(
        changes2.ctrl_data_paragraphs, 0,
        "variant 02 must not copy ctrl_data_records"
    );

    let changes3 = task903_generate_stage11_probe_variant(
        "03_picture_full_plus_table_record.hwp",
        false,
        false,
        true,
        false,
        false,
    );
    assert_eq!(
        changes3.base_full_pictures, 2,
        "variant 03 must copy both picture records as base"
    );
    assert_eq!(
        changes3.table_ctrl_headers, 0,
        "variant 03 must not copy first table CTRL_HEADER"
    );
    assert_eq!(
        changes3.cell_list_headers, 0,
        "variant 03 must not copy cell LIST_HEADER payloads"
    );
    assert!(
        changes3.table_records > 0,
        "variant 03 must copy TABLE record payload"
    );
    assert_eq!(
        changes3.cell_para_headers, 0,
        "variant 03 must not copy cell PARA_HEADER payloads"
    );
    assert_eq!(
        changes3.ctrl_data_paragraphs, 0,
        "variant 03 must not copy ctrl_data_records"
    );

    let changes4 = task903_generate_stage11_probe_variant(
        "04_picture_full_plus_cell_para_headers.hwp",
        false,
        false,
        false,
        true,
        false,
    );
    assert_eq!(
        changes4.base_full_pictures, 2,
        "variant 04 must copy both picture records as base"
    );
    assert_eq!(
        changes4.table_ctrl_headers, 0,
        "variant 04 must not copy first table CTRL_HEADER"
    );
    assert_eq!(
        changes4.cell_list_headers, 0,
        "variant 04 must not copy cell LIST_HEADER payloads"
    );
    assert_eq!(
        changes4.table_records, 0,
        "variant 04 must not copy TABLE record payload"
    );
    assert!(
        changes4.cell_para_headers > 0,
        "variant 04 must copy cell PARA_HEADER payloads"
    );
    assert_eq!(
        changes4.ctrl_data_paragraphs, 0,
        "variant 04 must not copy ctrl_data_records"
    );

    let changes5 = task903_generate_stage11_probe_variant(
        "05_picture_full_plus_structural_bundle.hwp",
        true,
        true,
        true,
        true,
        false,
    );
    assert_eq!(
        changes5.base_full_pictures, 2,
        "variant 05 must copy both picture records as base"
    );
    assert!(
        changes5.table_ctrl_headers > 0,
        "variant 05 must copy first table CTRL_HEADER"
    );
    assert!(
        changes5.cell_list_headers > 0,
        "variant 05 must copy cell LIST_HEADER payloads"
    );
    assert!(
        changes5.table_records > 0,
        "variant 05 must copy TABLE record payload"
    );
    assert!(
        changes5.cell_para_headers > 0,
        "variant 05 must copy cell PARA_HEADER payloads"
    );
    assert_eq!(
        changes5.ctrl_data_paragraphs, 0,
        "variant 05 must not copy ctrl_data_records"
    );

    let changes6 = task903_generate_stage11_probe_variant(
        "06_picture_full_plus_first_table_ctrl_data_records.hwp",
        false,
        false,
        false,
        false,
        true,
    );
    assert_eq!(
        changes6.base_full_pictures, 2,
        "variant 06 must copy both picture records as base"
    );
    assert_eq!(
        changes6.table_ctrl_headers, 0,
        "variant 06 must not copy first table CTRL_HEADER"
    );
    assert_eq!(
        changes6.cell_list_headers, 0,
        "variant 06 must not copy cell LIST_HEADER payloads"
    );
    assert_eq!(
        changes6.table_records, 0,
        "variant 06 must not copy TABLE record payload"
    );
    assert_eq!(
        changes6.cell_para_headers, 0,
        "variant 06 must not copy cell PARA_HEADER payloads"
    );
    assert!(
        changes6.ctrl_data_paragraphs > 0,
        "variant 06 must copy first table cell ctrl_data_records"
    );
}

#[test]
fn task903_stage13_generate_after_first_table_boundary_probe_variants() {
    let changes1 = task903_generate_stage13_probe_variant(
        "01_post_first_table_top_level_para_headers.hwp",
        true,
        false,
        false,
        false,
        false,
    );
    assert_eq!(
        changes1.post_top_level_para_headers, 3,
        "variant 01 must copy top-level PARA_HEADER payloads after first table"
    );
    assert_eq!(
        changes1.post_top_level_para_records, 0,
        "variant 01 must not copy full paragraph records"
    );
    assert_eq!(
        changes1.next_table_ctrl_headers, 0,
        "variant 01 must not copy next table CTRL_HEADER"
    );
    assert_eq!(
        changes1.next_table_child_headers, 0,
        "variant 01 must not copy next table child headers"
    );
    assert_eq!(
        changes1.next_table_record_spans, 0,
        "variant 01 must not copy next table record span"
    );

    let changes2 = task903_generate_stage13_probe_variant(
        "02_next_table_ctrl_header.hwp",
        false,
        false,
        true,
        false,
        false,
    );
    assert_eq!(
        changes2.post_top_level_para_headers, 0,
        "variant 02 must not copy post-first-table paragraph headers"
    );
    assert_eq!(
        changes2.next_table_ctrl_headers, 1,
        "variant 02 must copy next table CTRL_HEADER"
    );
    assert_eq!(
        changes2.next_table_child_headers, 0,
        "variant 02 must not copy next table child headers"
    );
    assert_eq!(
        changes2.next_table_record_spans, 0,
        "variant 02 must not copy next table record span"
    );

    let changes3 = task903_generate_stage13_probe_variant(
        "03_next_table_child_headers.hwp",
        false,
        false,
        false,
        true,
        false,
    );
    assert_eq!(
        changes3.next_table_ctrl_headers, 0,
        "variant 03 must not copy next table CTRL_HEADER"
    );
    assert!(
        changes3.next_table_child_headers >= 3,
        "variant 03 must copy next table TABLE/LIST_HEADER/PARA_HEADER payloads"
    );
    assert_eq!(
        changes3.next_table_record_spans, 0,
        "variant 03 must not copy next table record span"
    );

    let changes4 = task903_generate_stage13_probe_variant(
        "04_next_table_record_span_47_53.hwp",
        false,
        false,
        false,
        false,
        true,
    );
    assert_eq!(
        changes4.next_table_ctrl_headers, 0,
        "variant 04 counts the next table as a span, not CTRL_HEADER only"
    );
    assert_eq!(
        changes4.next_table_child_headers, 0,
        "variant 04 counts the next table as a span, not child headers only"
    );
    assert_eq!(
        changes4.next_table_record_spans, 1,
        "variant 04 must copy next table record span"
    );

    let changes5 = task903_generate_stage13_probe_variant(
        "05_post_first_table_paras_plus_next_table_span.hwp",
        false,
        true,
        false,
        false,
        true,
    );
    assert_eq!(
        changes5.post_top_level_para_records, 3,
        "variant 05 must copy post-first-table paragraph records"
    );
    assert_eq!(
        changes5.next_table_record_spans, 1,
        "variant 05 must copy next table record span"
    );
}

#[test]
fn task903_stage14_generate_chart_table_boundary_probe_variants() {
    let changes1 = task903_generate_stage14_probe_variant(
        "01_chart_table_ctrl_header.hwp",
        true,
        false,
        false,
        false,
        false,
    );
    assert!(
        changes1.base_next_table_child_headers > 0,
        "variant 01 must include Stage 13 child-header base"
    );
    assert_eq!(
        changes1.chart_table_ctrl_headers, 1,
        "variant 01 must copy chart table CTRL_HEADER"
    );
    assert_eq!(
        changes1.chart_table_records, 0,
        "variant 01 must not copy chart TABLE record"
    );
    assert_eq!(
        changes1.chart_table_first_cell_headers, 0,
        "variant 01 must not copy chart first cell headers"
    );
    assert_eq!(
        changes1.chart_table_all_cell_headers, 0,
        "variant 01 must not copy all chart cell headers"
    );
    assert_eq!(
        changes1.chart_table_full_objects, 0,
        "variant 01 must not copy full chart table object"
    );

    let changes2 = task903_generate_stage14_probe_variant(
        "02_chart_table_record_only.hwp",
        false,
        true,
        false,
        false,
        false,
    );
    assert_eq!(
        changes2.chart_table_ctrl_headers, 0,
        "variant 02 must not copy chart table CTRL_HEADER"
    );
    assert_eq!(
        changes2.chart_table_records, 1,
        "variant 02 must copy chart TABLE record"
    );
    assert_eq!(
        changes2.chart_table_first_cell_headers, 0,
        "variant 02 must not copy chart first cell headers"
    );
    assert_eq!(
        changes2.chart_table_all_cell_headers, 0,
        "variant 02 must not copy all chart cell headers"
    );

    let changes3 = task903_generate_stage14_probe_variant(
        "03_chart_table_first_cell_headers.hwp",
        false,
        false,
        true,
        false,
        false,
    );
    assert_eq!(
        changes3.chart_table_ctrl_headers, 0,
        "variant 03 must not copy chart table CTRL_HEADER"
    );
    assert_eq!(
        changes3.chart_table_records, 0,
        "variant 03 must not copy chart TABLE record"
    );
    assert!(
        changes3.chart_table_first_cell_headers >= 2,
        "variant 03 must copy first cell LIST_HEADER/PARA_HEADER payloads"
    );
    assert_eq!(
        changes3.chart_table_all_cell_headers, 0,
        "variant 03 must not copy all chart cell headers"
    );

    let changes4 = task903_generate_stage14_probe_variant(
        "04_chart_table_all_cell_headers.hwp",
        false,
        false,
        false,
        true,
        false,
    );
    assert_eq!(
        changes4.chart_table_ctrl_headers, 0,
        "variant 04 must not copy chart table CTRL_HEADER"
    );
    assert_eq!(
        changes4.chart_table_records, 0,
        "variant 04 must not copy chart TABLE record"
    );
    assert_eq!(
        changes4.chart_table_first_cell_headers, 0,
        "variant 04 counts all cell headers separately"
    );
    assert!(
        changes4.chart_table_all_cell_headers >= 66,
        "variant 04 must copy all chart cell LIST_HEADER/PARA_HEADER payloads"
    );

    let changes5 = task903_generate_stage14_probe_variant(
        "05_chart_table_structural_bundle.hwp",
        true,
        true,
        false,
        true,
        false,
    );
    assert_eq!(
        changes5.chart_table_ctrl_headers, 1,
        "variant 05 must copy chart table CTRL_HEADER"
    );
    assert_eq!(
        changes5.chart_table_records, 1,
        "variant 05 must copy chart TABLE record"
    );
    assert!(
        changes5.chart_table_all_cell_headers >= 66,
        "variant 05 must copy all chart cell headers"
    );
    assert_eq!(
        changes5.chart_table_full_objects, 0,
        "variant 05 must not copy full chart table object"
    );

    let changes6 = task903_generate_stage14_probe_variant(
        "06_chart_table_full_object.hwp",
        false,
        false,
        false,
        false,
        true,
    );
    assert_eq!(
        changes6.chart_table_ctrl_headers, 0,
        "variant 06 counts the chart table as full object"
    );
    assert_eq!(
        changes6.chart_table_records, 0,
        "variant 06 counts the chart table as full object"
    );
    assert_eq!(
        changes6.chart_table_all_cell_headers, 0,
        "variant 06 counts the chart table as full object"
    );
    assert_eq!(
        changes6.chart_table_full_objects, 1,
        "variant 06 must copy full chart table object"
    );
}

#[test]
fn task903_stage16_generate_chart_tuple_probe_variants() {
    let changes1 = task903_generate_stage16_probe_variant(
        "01_chart_host_para_raw_headers.hwp",
        true,
        false,
        false,
        false,
        false,
        false,
    );
    assert_eq!(
        changes1.chart_host_para_records, 1,
        "variant 01 must copy chart host paragraph records"
    );
    assert_eq!(
        changes1.chart_table_records_with_tail, 0,
        "variant 01 must not copy chart TABLE record"
    );

    let changes2 = task903_generate_stage16_probe_variant(
        "02_chart_ctrl_table_raw_pair.hwp",
        false,
        true,
        true,
        false,
        false,
        false,
    );
    assert_eq!(
        changes2.chart_table_ctrl_headers, 1,
        "variant 02 must copy chart CTRL_HEADER"
    );
    assert_eq!(
        changes2.chart_table_records_with_tail, 1,
        "variant 02 must copy chart TABLE record with encoded zone tail"
    );
    assert_eq!(
        changes2.chart_table_all_cell_headers, 0,
        "variant 02 must not copy all cell headers"
    );

    let changes3 = task903_generate_stage16_probe_variant(
        "03_chart_table_all_cell_headers_raw.hwp",
        false,
        false,
        true,
        true,
        false,
        false,
    );
    assert_eq!(
        changes3.chart_table_ctrl_headers, 0,
        "variant 03 must not copy chart CTRL_HEADER"
    );
    assert_eq!(
        changes3.chart_table_records_with_tail, 1,
        "variant 03 must copy chart TABLE record with encoded zone tail"
    );
    assert!(
        changes3.chart_table_all_cell_headers >= 66,
        "variant 03 must copy all chart cell LIST_HEADER/PARA_HEADER payloads"
    );

    let changes4 = task903_generate_stage16_probe_variant(
        "04_chart_host_para_plus_chart_full_raw_tuple.hwp",
        true,
        false,
        false,
        false,
        true,
        false,
    );
    assert_eq!(
        changes4.chart_host_para_records, 1,
        "variant 04 must copy chart host paragraph records"
    );
    assert_eq!(
        changes4.chart_table_full_objects_with_tail, 1,
        "variant 04 must copy full chart table object with encoded zone tail"
    );

    let changes5 = task903_generate_stage16_probe_variant(
        "05_chart_title_text_angle_brackets_only.hwp",
        false,
        false,
        false,
        false,
        false,
        true,
    );
    assert_eq!(
        changes5.following_title_records, 1,
        "variant 05 must copy following title paragraph records"
    );
    assert_eq!(
        changes5.chart_table_records_with_tail, 0,
        "variant 05 must not copy chart TABLE record"
    );

    let changes6 = task903_generate_stage16_probe_variant(
        "06_chart_tuple_plus_following_title_text.hwp",
        true,
        false,
        false,
        false,
        true,
        true,
    );
    assert_eq!(
        changes6.chart_host_para_records, 1,
        "variant 06 must copy chart host paragraph records"
    );
    assert_eq!(
        changes6.chart_table_full_objects_with_tail, 1,
        "variant 06 must copy full chart table object with encoded zone tail"
    );
    assert_eq!(
        changes6.following_title_records, 1,
        "variant 06 must copy following title paragraph records"
    );
}

#[test]
fn task903_stage17_generate_industry_table_probe_variants() {
    let changes1 = task903_generate_stage17_probe_variant(
        "01_industry_table_ctrl_header.hwp",
        false,
        true,
        false,
        false,
        false,
        false,
    );
    assert_eq!(
        changes1.base_chart_table_full_objects_with_tail, 1,
        "variant 01 must include Stage 16 chart tuple base"
    );
    assert_eq!(
        changes1.industry_table_ctrl_headers, 1,
        "variant 01 must copy industry CTRL_HEADER"
    );
    assert_eq!(
        changes1.industry_table_records_with_tail, 0,
        "variant 01 must not copy industry TABLE record"
    );

    let changes2 = task903_generate_stage17_probe_variant(
        "02_industry_table_record_with_tail.hwp",
        false,
        false,
        true,
        false,
        false,
        false,
    );
    assert_eq!(
        changes2.industry_table_ctrl_headers, 0,
        "variant 02 must not copy industry CTRL_HEADER"
    );
    assert_eq!(
        changes2.industry_table_records_with_tail, 1,
        "variant 02 must copy industry TABLE record with encoded zone tail"
    );
    assert_eq!(
        changes2.industry_table_all_cell_headers, 0,
        "variant 02 must not copy all industry cell headers"
    );

    let changes3 = task903_generate_stage17_probe_variant(
        "03_industry_table_all_cell_headers.hwp",
        false,
        false,
        false,
        true,
        false,
        false,
    );
    assert_eq!(
        changes3.industry_table_records_with_tail, 0,
        "variant 03 must not copy industry TABLE record"
    );
    assert!(
        changes3.industry_table_all_cell_headers >= 48,
        "variant 03 must copy all industry cell LIST_HEADER/PARA_HEADER payloads"
    );

    let changes4 = task903_generate_stage17_probe_variant(
        "04_industry_table_full_object_with_tail.hwp",
        false,
        false,
        false,
        false,
        true,
        false,
    );
    assert_eq!(
        changes4.industry_table_full_objects_with_tail, 1,
        "variant 04 must copy full industry table object with encoded zone tail"
    );
    assert_eq!(
        changes4.industry_host_para_records, 0,
        "variant 04 must not copy industry host paragraph"
    );

    let changes5 = task903_generate_stage17_probe_variant(
        "05_industry_host_para_plus_table_full_tuple.hwp",
        true,
        false,
        false,
        false,
        true,
        false,
    );
    assert_eq!(
        changes5.industry_host_para_records, 1,
        "variant 05 must copy industry host paragraph"
    );
    assert_eq!(
        changes5.industry_table_full_objects_with_tail, 1,
        "variant 05 must copy full industry table object"
    );

    let changes6 = task903_generate_stage17_probe_variant(
        "06_industry_tuple_plus_next_boundary.hwp",
        true,
        false,
        false,
        false,
        true,
        true,
    );
    assert_eq!(
        changes6.industry_host_para_records, 1,
        "variant 06 must copy industry host paragraph"
    );
    assert_eq!(
        changes6.industry_table_full_objects_with_tail, 1,
        "variant 06 must copy full industry table object"
    );
    assert_eq!(
        changes6.next_boundary_para_records, 1,
        "variant 06 must copy next boundary paragraph"
    );
}

#[test]
fn task903_stage18_generate_country_reflow_probe_variants() {
    let changes1 = task903_generate_stage18_probe_variant(
        "01_country_title_para_record.hwp",
        true,
        false,
        false,
        true,
        true,
        false,
        false,
        true,
        false,
        false,
        false,
        false,
        false,
    );
    assert_eq!(
        changes1.country_title_para_records, 1,
        "variant 01 must copy country title paragraph"
    );
    assert_eq!(
        changes1.country_table_records_with_tail, 0,
        "variant 01 must not copy country TABLE record"
    );

    let changes2 = task903_generate_stage18_probe_variant(
        "02_country_table_ctrl_header.hwp",
        true,
        false,
        false,
        true,
        true,
        false,
        false,
        true,
        false,
        true,
        false,
        false,
        false,
    );
    assert_eq!(
        changes2.country_table_ctrl_headers, 1,
        "variant 02 must copy country CTRL_HEADER"
    );

    let changes3 = task903_generate_stage18_probe_variant(
        "03_country_table_record_with_tail.hwp",
        true,
        false,
        false,
        true,
        true,
        false,
        false,
        true,
        false,
        false,
        true,
        false,
        false,
    );
    assert_eq!(
        changes3.country_table_records_with_tail, 1,
        "variant 03 must copy country TABLE record with encoded zone tail"
    );

    let changes4 = task903_generate_stage18_probe_variant(
        "04_country_table_all_cell_headers.hwp",
        true,
        false,
        false,
        true,
        true,
        false,
        false,
        true,
        false,
        false,
        false,
        true,
        false,
    );
    assert!(
        changes4.country_table_all_cell_headers >= 48,
        "variant 04 must copy all country cell LIST_HEADER/PARA_HEADER payloads"
    );

    let changes5 = task903_generate_stage18_probe_variant(
        "05_country_table_full_object_with_tail.hwp",
        true,
        false,
        false,
        true,
        true,
        false,
        false,
        true,
        false,
        false,
        false,
        false,
        true,
    );
    assert_eq!(
        changes5.country_table_full_objects_with_tail, 1,
        "variant 05 must copy country full table object with encoded zone tail"
    );

    let changes6 = task903_generate_stage18_probe_variant(
        "06_country_host_para_plus_table_full_tuple.hwp",
        true,
        false,
        false,
        true,
        true,
        false,
        false,
        true,
        true,
        false,
        false,
        false,
        true,
    );
    assert_eq!(
        changes6.country_host_para_records, 1,
        "variant 06 must copy country host paragraph"
    );
    assert_eq!(
        changes6.country_table_full_objects_with_tail, 1,
        "variant 06 must copy country full table object"
    );

    let changes7 = task903_generate_stage18_probe_variant(
        "07_industry_record_plus_cell_linesegs.hwp",
        false,
        true,
        false,
        false,
        false,
        true,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
    );
    assert_eq!(
        changes7.industry_table_records_with_tail, 1,
        "variant 07 must copy industry TABLE record with encoded zone tail"
    );
    assert!(
        changes7.industry_cell_linesegs >= 24,
        "variant 07 must copy industry cell lineSegs"
    );

    let changes8 = task903_generate_stage18_probe_variant(
        "08_industry_record_plus_cell_para_records.hwp",
        false,
        true,
        false,
        false,
        false,
        false,
        true,
        false,
        false,
        false,
        false,
        false,
        false,
    );
    assert_eq!(
        changes8.industry_table_records_with_tail, 1,
        "variant 08 must copy industry TABLE record with encoded zone tail"
    );
    assert!(
        changes8.industry_cell_para_records >= 24,
        "variant 08 must copy industry cell paragraph records"
    );

    let changes9 = task903_generate_stage18_probe_variant(
        "09_industry_record_headers_plus_cell_para_records.hwp",
        false,
        true,
        true,
        false,
        false,
        false,
        true,
        false,
        false,
        false,
        false,
        false,
        false,
    );
    assert_eq!(
        changes9.industry_table_records_with_tail, 1,
        "variant 09 must copy industry TABLE record with encoded zone tail"
    );
    assert!(
        changes9.industry_table_all_cell_headers >= 48,
        "variant 09 must copy industry cell headers"
    );
    assert!(
        changes9.industry_cell_para_records >= 24,
        "variant 09 must copy industry cell paragraph records"
    );
}

#[test]
fn task903_stage19_generate_region_boundary_probe_variants() {
    let changes1 = task903_generate_stage19_probe_variant(
        "01_region_title_para_record.hwp",
        true,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
    );
    assert_eq!(
        changes1.region_title_para_records, 1,
        "variant 01 must copy region title paragraph"
    );
    assert_eq!(
        changes1.region_table_records_with_tail, 0,
        "variant 01 must not copy region TABLE record"
    );

    let changes2 = task903_generate_stage19_probe_variant(
        "02_region_table_ctrl_header.hwp",
        true,
        false,
        true,
        false,
        false,
        false,
        false,
        false,
    );
    assert_eq!(
        changes2.region_table_ctrl_headers, 1,
        "variant 02 must copy region CTRL_HEADER"
    );

    let changes3 = task903_generate_stage19_probe_variant(
        "03_region_table_record_with_tail.hwp",
        true,
        false,
        false,
        true,
        false,
        false,
        false,
        false,
    );
    assert_eq!(
        changes3.region_table_records_with_tail, 1,
        "variant 03 must copy region TABLE record with encoded zone tail"
    );

    let changes4 = task903_generate_stage19_probe_variant(
        "04_region_table_all_cell_headers.hwp",
        true,
        false,
        false,
        false,
        true,
        false,
        false,
        false,
    );
    assert!(
        changes4.region_table_all_cell_headers >= 64,
        "variant 04 must copy all region cell LIST_HEADER/PARA_HEADER payloads"
    );

    let changes5 = task903_generate_stage19_probe_variant(
        "05_region_table_full_object_with_tail.hwp",
        true,
        false,
        false,
        false,
        false,
        true,
        false,
        false,
    );
    assert_eq!(
        changes5.region_table_full_objects_with_tail, 1,
        "variant 05 must copy region full table object with encoded zone tail"
    );

    let changes6 = task903_generate_stage19_probe_variant(
        "06_region_host_para_plus_table_full_tuple.hwp",
        true,
        true,
        false,
        false,
        false,
        true,
        false,
        false,
    );
    assert_eq!(
        changes6.region_host_para_records, 1,
        "variant 06 must copy region host paragraph"
    );
    assert_eq!(
        changes6.region_table_full_objects_with_tail, 1,
        "variant 06 must copy region full table object"
    );

    let changes7 = task903_generate_stage19_probe_variant(
        "07_region_record_plus_following_empty_para.hwp",
        true,
        false,
        false,
        true,
        false,
        false,
        true,
        false,
    );
    assert_eq!(
        changes7.region_table_records_with_tail, 1,
        "variant 07 must copy region TABLE record"
    );
    assert_eq!(
        changes7.following_empty_para_records, 1,
        "variant 07 must copy paragraph 0:24"
    );

    let changes8 = task903_generate_stage19_probe_variant(
        "08_region_full_tuple_plus_following_text_para.hwp",
        true,
        true,
        false,
        false,
        false,
        true,
        true,
        true,
    );
    assert_eq!(
        changes8.region_table_full_objects_with_tail, 1,
        "variant 08 must copy region full table object"
    );
    assert_eq!(
        changes8.following_text_para_records, 1,
        "variant 08 must copy paragraph 0:25"
    );
}

#[test]
fn task903_stage20_generate_notice_boundary_probe_variants() {
    let changes1 = task903_generate_stage20_probe_variant(
        "01_pre_notice_empty_paras.hwp",
        true,
        false,
        false,
        false,
        false,
        false,
        false,
    );
    assert_eq!(
        changes1.pre_notice_empty_para_records, 2,
        "variant 01 must copy paragraphs 0:26 and 0:27"
    );

    let changes2 = task903_generate_stage20_probe_variant(
        "02_notice_para_record.hwp",
        true,
        true,
        false,
        false,
        false,
        false,
        false,
    );
    assert_eq!(
        changes2.notice_para_records, 1,
        "variant 02 must copy paragraph 0:28"
    );

    let changes3 = task903_generate_stage20_probe_variant(
        "03_notice_table_ctrl_header.hwp",
        true,
        true,
        true,
        false,
        false,
        false,
        false,
    );
    assert_eq!(
        changes3.notice_table_ctrl_headers, 1,
        "variant 03 must copy notice table CTRL_HEADER"
    );

    let changes4 = task903_generate_stage20_probe_variant(
        "04_notice_table_record_with_tail.hwp",
        true,
        true,
        false,
        true,
        false,
        false,
        false,
    );
    assert_eq!(
        changes4.notice_table_records_with_tail, 1,
        "variant 04 must copy notice TABLE record with encoded zone tail"
    );

    let changes5 = task903_generate_stage20_probe_variant(
        "05_notice_table_all_cell_headers.hwp",
        true,
        true,
        false,
        false,
        true,
        false,
        false,
    );
    assert!(
        changes5.notice_table_all_cell_headers >= 24,
        "variant 05 must copy all notice cell LIST_HEADER/PARA_HEADER payloads"
    );

    let changes6 = task903_generate_stage20_probe_variant(
        "06_notice_table_full_object_with_tail.hwp",
        true,
        true,
        false,
        false,
        false,
        true,
        false,
    );
    assert_eq!(
        changes6.notice_table_full_objects_with_tail, 1,
        "variant 06 must copy notice full table object"
    );

    let changes7 = task903_generate_stage20_probe_variant(
        "07_notice_table_full_object_without_para_record.hwp",
        true,
        false,
        false,
        false,
        false,
        true,
        false,
    );
    assert_eq!(
        changes7.notice_para_records, 0,
        "variant 07 must not copy notice paragraph record"
    );
    assert_eq!(
        changes7.notice_table_full_objects_with_tail, 1,
        "variant 07 must copy notice full table object"
    );

    let changes8 = task903_generate_stage20_probe_variant(
        "08_notice_full_tuple_plus_logo_group_para_record.hwp",
        true,
        true,
        false,
        false,
        false,
        true,
        true,
    );
    assert_eq!(
        changes8.notice_table_full_objects_with_tail, 1,
        "variant 08 must copy notice full table object"
    );
    assert_eq!(
        changes8.logo_group_para_records, 1,
        "variant 08 must copy paragraph 0:29"
    );
}

#[test]
fn task903_stage21_generate_logo_group_probe_variants() {
    let changes1 = task903_generate_stage21_probe_variant(
        "01_logo_group_para_record.hwp",
        true,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
    );
    assert_eq!(
        changes1.logo_group_para_records, 1,
        "variant 01 must copy paragraph 0:29"
    );

    let changes2 = task903_generate_stage21_probe_variant(
        "02_logo_group_full_object_without_para_record.hwp",
        false,
        false,
        false,
        false,
        false,
        true,
        false,
        false,
    );
    assert_eq!(
        changes2.logo_group_full_objects, 1,
        "variant 02 must copy logo group full object"
    );

    let changes3 = task903_generate_stage21_probe_variant(
        "03_logo_group_common_attr.hwp",
        true,
        true,
        false,
        false,
        false,
        false,
        false,
        false,
    );
    assert_eq!(
        changes3.logo_group_common_attrs, 1,
        "variant 03 must copy logo group common attr"
    );

    let changes4 = task903_generate_stage21_probe_variant(
        "04_logo_group_shape_attr.hwp",
        true,
        false,
        true,
        false,
        false,
        false,
        false,
        false,
    );
    assert_eq!(
        changes4.logo_group_shape_attrs, 1,
        "variant 04 must copy logo group shape attr"
    );

    let changes5 = task903_generate_stage21_probe_variant(
        "05_logo_group_common_shape_child_shape_attrs.hwp",
        true,
        true,
        true,
        true,
        false,
        false,
        false,
        false,
    );
    assert_eq!(
        changes5.logo_group_child_shape_attrs, 3,
        "variant 05 must copy all child shape attrs"
    );

    let changes6 = task903_generate_stage21_probe_variant(
        "06_logo_group_common_shape_child_full_pictures.hwp",
        true,
        true,
        true,
        false,
        true,
        false,
        false,
        false,
    );
    assert_eq!(
        changes6.logo_group_child_full_pictures, 3,
        "variant 06 must copy all child pictures"
    );

    let changes7 = task903_generate_stage21_probe_variant(
        "07_logo_group_full_tuple.hwp",
        true,
        false,
        false,
        false,
        false,
        true,
        false,
        false,
    );
    assert_eq!(
        changes7.logo_group_para_records, 1,
        "variant 07 must copy paragraph 0:29"
    );
    assert_eq!(
        changes7.logo_group_full_objects, 1,
        "variant 07 must copy logo group full object"
    );

    let changes8 = task903_generate_stage21_probe_variant(
        "08_logo_group_full_tuple_plus_attachment_title_table.hwp",
        true,
        false,
        false,
        false,
        false,
        true,
        true,
        true,
    );
    assert_eq!(
        changes8.logo_group_full_objects, 1,
        "variant 08 must copy logo group full object"
    );
    assert_eq!(
        changes8.attachment_title_para_records, 1,
        "variant 08 must copy paragraph 0:30"
    );
    assert_eq!(
        changes8.attachment_title_table_full_objects, 1,
        "variant 08 must copy attachment title table"
    );
}

#[test]
fn task903_stage22_generate_top_country_probe_variants() {
    let changes1 = task903_generate_stage22_probe_variant(
        "01_top_country_title_para_record.hwp",
        true,
        false,
        false,
        false,
        false,
        false,
        false,
    );
    assert_eq!(
        changes1.top_country_title_para_records, 1,
        "variant 01 must copy paragraph 0:43"
    );

    let changes2 = task903_generate_stage22_probe_variant(
        "02_top_country_table_ctrl_header.hwp",
        true,
        false,
        true,
        false,
        false,
        false,
        false,
    );
    assert_eq!(
        changes2.top_country_table_ctrl_headers, 1,
        "variant 02 must copy top country CTRL_HEADER"
    );

    let changes3 = task903_generate_stage22_probe_variant(
        "03_top_country_table_record_with_tail.hwp",
        true,
        false,
        false,
        true,
        false,
        false,
        false,
    );
    assert_eq!(
        changes3.top_country_table_records_with_tail, 1,
        "variant 03 must copy top country TABLE record with encoded zone tail"
    );

    let changes4 = task903_generate_stage22_probe_variant(
        "04_top_country_table_all_cell_headers.hwp",
        true,
        false,
        false,
        false,
        true,
        false,
        false,
    );
    assert!(
        changes4.top_country_table_all_cell_headers >= 216,
        "variant 04 must copy all top country cell LIST_HEADER/PARA_HEADER payloads"
    );

    let changes5 = task903_generate_stage22_probe_variant(
        "05_top_country_table_full_object_with_tail.hwp",
        true,
        false,
        false,
        false,
        false,
        true,
        false,
    );
    assert_eq!(
        changes5.top_country_table_full_objects_with_tail, 1,
        "variant 05 must copy top country full table object"
    );

    let changes6 = task903_generate_stage22_probe_variant(
        "06_top_country_host_para_plus_table_full_tuple.hwp",
        true,
        true,
        false,
        false,
        false,
        true,
        false,
    );
    assert_eq!(
        changes6.top_country_host_para_records, 1,
        "variant 06 must copy paragraph 0:44"
    );
    assert_eq!(
        changes6.top_country_table_full_objects_with_tail, 1,
        "variant 06 must copy top country full table object"
    );

    let changes7 = task903_generate_stage22_probe_variant(
        "07_top_country_record_plus_next_boundary.hwp",
        true,
        false,
        false,
        true,
        false,
        false,
        true,
    );
    assert_eq!(
        changes7.top_country_table_records_with_tail, 1,
        "variant 07 must copy top country TABLE record"
    );
    assert_eq!(
        changes7.next_boundary_para_records, 1,
        "variant 07 must copy paragraph 0:45"
    );

    let changes8 = task903_generate_stage22_probe_variant(
        "08_top_country_full_tuple_plus_next_boundary.hwp",
        true,
        true,
        false,
        false,
        false,
        true,
        true,
    );
    assert_eq!(
        changes8.top_country_table_full_objects_with_tail, 1,
        "variant 08 must copy top country full table object"
    );
    assert_eq!(
        changes8.next_boundary_para_records, 1,
        "variant 08 must copy paragraph 0:45"
    );
}

#[test]
fn task903_stage23_generate_year_trend_probe_variants() {
    let changes1 = task903_generate_stage23_probe_variant(
        "01_pre_year_empty_paras.hwp",
        true,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
    );
    assert_eq!(
        changes1.pre_year_empty_para_records, 2,
        "variant 01 must copy paragraphs 0:48 and 0:49"
    );

    let changes2 = task903_generate_stage23_probe_variant(
        "02_year_title_para_record.hwp",
        true,
        true,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
    );
    assert_eq!(
        changes2.year_title_para_records, 1,
        "variant 02 must copy paragraph 0:50"
    );

    let changes3 = task903_generate_stage23_probe_variant(
        "03_year_table_ctrl_header.hwp",
        true,
        true,
        true,
        false,
        true,
        false,
        false,
        false,
        false,
    );
    assert_eq!(
        changes3.year_table_ctrl_headers, 1,
        "variant 03 must copy year trend CTRL_HEADER"
    );

    let changes4 = task903_generate_stage23_probe_variant(
        "04_year_table_record_with_tail.hwp",
        true,
        true,
        true,
        false,
        false,
        true,
        false,
        false,
        false,
    );
    assert_eq!(
        changes4.year_table_records_with_tail, 1,
        "variant 04 must copy year trend TABLE record with encoded zone tail"
    );

    let changes5 = task903_generate_stage23_probe_variant(
        "05_year_table_all_cell_headers.hwp",
        true,
        true,
        true,
        false,
        false,
        false,
        true,
        false,
        false,
    );
    assert!(
        changes5.year_table_all_cell_headers >= 42,
        "variant 05 must copy all year trend cell LIST_HEADER/PARA_HEADER payloads"
    );

    let changes6 = task903_generate_stage23_probe_variant(
        "06_year_table_full_object_with_tail.hwp",
        true,
        true,
        true,
        false,
        false,
        false,
        false,
        true,
        false,
    );
    assert_eq!(
        changes6.year_table_full_objects_with_tail, 1,
        "variant 06 must copy year trend full table object"
    );

    let changes7 = task903_generate_stage23_probe_variant(
        "07_year_host_para_plus_table_full_tuple.hwp",
        true,
        true,
        true,
        true,
        false,
        false,
        false,
        true,
        false,
    );
    assert_eq!(
        changes7.year_host_para_records, 1,
        "variant 07 must copy paragraph 0:52"
    );
    assert_eq!(
        changes7.year_table_full_objects_with_tail, 1,
        "variant 07 must copy year trend full table object"
    );

    let changes8 = task903_generate_stage23_probe_variant(
        "08_year_full_tuple_plus_next_boundary.hwp",
        true,
        true,
        true,
        true,
        false,
        false,
        false,
        true,
        true,
    );
    assert_eq!(
        changes8.year_table_full_objects_with_tail, 1,
        "variant 08 must copy year trend full table object"
    );
    assert_eq!(
        changes8.next_boundary_para_records, 2,
        "variant 08 must copy paragraphs 0:53 and 0:54"
    );
}

#[test]
fn task903_stage24_generate_second_year_trend_probe_variants() {
    let changes1 = task903_generate_stage24_probe_variant(
        "01_pre_second_year_empty_paras.hwp",
        true,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
    );
    assert_eq!(
        changes1.pre_second_year_empty_para_records, 2,
        "variant 01 must copy paragraphs 0:85 and 0:86"
    );

    let changes2 = task903_generate_stage24_probe_variant(
        "02_second_year_title_para_record.hwp",
        true,
        true,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
    );
    assert_eq!(
        changes2.second_year_title_para_records, 1,
        "variant 02 must copy paragraph 0:87"
    );

    let changes3 = task903_generate_stage24_probe_variant(
        "03_second_year_table_ctrl_header.hwp",
        true,
        true,
        true,
        false,
        true,
        false,
        false,
        false,
        false,
    );
    assert_eq!(
        changes3.second_year_table_ctrl_headers, 1,
        "variant 03 must copy second year trend CTRL_HEADER"
    );

    let changes4 = task903_generate_stage24_probe_variant(
        "04_second_year_table_record_with_tail.hwp",
        true,
        true,
        true,
        false,
        false,
        true,
        false,
        false,
        false,
    );
    assert_eq!(
        changes4.second_year_table_records_with_tail, 1,
        "variant 04 must copy second year trend TABLE record with encoded zone tail"
    );

    let changes5 = task903_generate_stage24_probe_variant(
        "05_second_year_table_all_cell_headers.hwp",
        true,
        true,
        true,
        false,
        false,
        false,
        true,
        false,
        false,
    );
    assert!(
        changes5.second_year_table_all_cell_headers >= 42,
        "variant 05 must copy all second year trend cell LIST_HEADER/PARA_HEADER payloads"
    );

    let changes6 = task903_generate_stage24_probe_variant(
        "06_second_year_table_full_object_with_tail.hwp",
        true,
        true,
        true,
        false,
        false,
        false,
        false,
        true,
        false,
    );
    assert_eq!(
        changes6.second_year_table_full_objects_with_tail, 1,
        "variant 06 must copy second year trend full table object"
    );

    let changes7 = task903_generate_stage24_probe_variant(
        "07_second_year_host_para_plus_table_full_tuple.hwp",
        true,
        true,
        true,
        true,
        false,
        false,
        false,
        true,
        false,
    );
    assert_eq!(
        changes7.second_year_host_para_records, 1,
        "variant 07 must copy paragraph 0:89"
    );
    assert_eq!(
        changes7.second_year_table_full_objects_with_tail, 1,
        "variant 07 must copy second year trend full table object"
    );

    let changes8 = task903_generate_stage24_probe_variant(
        "08_second_year_full_tuple_plus_next_boundary.hwp",
        true,
        true,
        true,
        true,
        false,
        false,
        false,
        true,
        true,
    );
    assert_eq!(
        changes8.second_year_table_full_objects_with_tail, 1,
        "variant 08 must copy second year trend full table object"
    );
    assert_eq!(
        changes8.next_boundary_para_records, 3,
        "variant 08 must copy paragraphs 0:90, 0:91, and 0:92"
    );
}

#[test]
fn task903_stage25_generate_final_industry_probe_variants() {
    let changes1 = task903_generate_stage25_probe_variant(
        "01_final_industry_blank_para.hwp",
        true,
        false,
        false,
        false,
        false,
        false,
        false,
    );
    assert_eq!(
        changes1.final_industry_blank_para_records, 1,
        "variant 01 must copy paragraph 0:93"
    );

    let changes2 = task903_generate_stage25_probe_variant(
        "02_final_industry_table_ctrl_header.hwp",
        true,
        false,
        true,
        false,
        false,
        false,
        false,
    );
    assert_eq!(
        changes2.final_industry_table_ctrl_headers, 1,
        "variant 02 must copy final industry CTRL_HEADER"
    );

    let changes3 = task903_generate_stage25_probe_variant(
        "03_final_industry_table_record_with_tail.hwp",
        true,
        false,
        false,
        true,
        false,
        false,
        false,
    );
    assert_eq!(
        changes3.final_industry_table_records_with_tail, 1,
        "variant 03 must copy final industry TABLE record with encoded zone tail"
    );

    let changes4 = task903_generate_stage25_probe_variant(
        "04_final_industry_table_all_cell_headers.hwp",
        true,
        false,
        false,
        false,
        true,
        false,
        false,
    );
    assert!(
        changes4.final_industry_table_all_cell_headers >= 144,
        "variant 04 must copy all final industry cell LIST_HEADER/PARA_HEADER payloads"
    );

    let changes5 = task903_generate_stage25_probe_variant(
        "05_final_industry_table_full_object_with_tail.hwp",
        true,
        false,
        false,
        false,
        false,
        true,
        false,
    );
    assert_eq!(
        changes5.final_industry_table_full_objects_with_tail, 1,
        "variant 05 must copy final industry full table object"
    );

    let changes6 = task903_generate_stage25_probe_variant(
        "06_final_industry_host_para_plus_table_full_tuple.hwp",
        true,
        true,
        false,
        false,
        false,
        true,
        false,
    );
    assert_eq!(
        changes6.final_industry_host_para_records, 1,
        "variant 06 must copy paragraph 0:94"
    );
    assert_eq!(
        changes6.final_industry_table_full_objects_with_tail, 1,
        "variant 06 must copy final industry full table object"
    );

    let changes7 = task903_generate_stage25_probe_variant(
        "07_final_industry_record_plus_next_boundary.hwp",
        true,
        false,
        false,
        true,
        false,
        false,
        true,
    );
    assert_eq!(
        changes7.final_industry_table_records_with_tail, 1,
        "variant 07 must copy final industry TABLE record"
    );
    assert_eq!(
        changes7.next_boundary_para_records, 2,
        "variant 07 must copy paragraphs 0:95 and 0:96"
    );

    let changes8 = task903_generate_stage25_probe_variant(
        "08_final_industry_full_tuple_plus_next_boundary.hwp",
        true,
        true,
        false,
        false,
        false,
        true,
        true,
    );
    assert_eq!(
        changes8.final_industry_table_full_objects_with_tail, 1,
        "variant 08 must copy final industry full table object"
    );
    assert_eq!(
        changes8.next_boundary_para_records, 2,
        "variant 08 must copy paragraphs 0:95 and 0:96"
    );
}

#[test]
fn task903_stage26_generate_final_country_probe_variants() {
    let changes1 = task903_generate_stage26_probe_variant(
        "01_final_country_table_ctrl_header.hwp",
        false,
        true,
        false,
        false,
        false,
        false,
    );
    assert_eq!(
        changes1.final_country_table_ctrl_headers, 1,
        "variant 01 must copy final country CTRL_HEADER"
    );

    let changes2 = task903_generate_stage26_probe_variant(
        "02_final_country_table_record_with_tail.hwp",
        false,
        false,
        true,
        false,
        false,
        false,
    );
    assert_eq!(
        changes2.final_country_table_records_with_tail, 1,
        "variant 02 must copy final country TABLE record with encoded zone tail"
    );

    let changes3 = task903_generate_stage26_probe_variant(
        "03_final_country_table_all_cell_headers.hwp",
        false,
        false,
        false,
        true,
        false,
        false,
    );
    assert!(
        changes3.final_country_table_all_cell_headers >= 144,
        "variant 03 must copy all final country cell LIST_HEADER/PARA_HEADER payloads"
    );

    let changes4 = task903_generate_stage26_probe_variant(
        "04_final_country_table_full_object_with_tail.hwp",
        false,
        false,
        false,
        false,
        true,
        false,
    );
    assert_eq!(
        changes4.final_country_table_full_objects_with_tail, 1,
        "variant 04 must copy final country full table object"
    );

    let changes5 = task903_generate_stage26_probe_variant(
        "05_final_country_host_para_plus_table_full_tuple.hwp",
        true,
        false,
        false,
        false,
        true,
        false,
    );
    assert_eq!(
        changes5.final_country_host_para_records, 1,
        "variant 05 must copy paragraph 0:97"
    );
    assert_eq!(
        changes5.final_country_table_full_objects_with_tail, 1,
        "variant 05 must copy final country full table object"
    );

    let changes6 = task903_generate_stage26_probe_variant(
        "06_final_country_record_plus_following_paras.hwp",
        false,
        false,
        true,
        false,
        false,
        true,
    );
    assert_eq!(
        changes6.final_country_table_records_with_tail, 1,
        "variant 06 must copy final country TABLE record"
    );
    assert_eq!(
        changes6.following_para_records, 4,
        "variant 06 must copy paragraphs 0:98 through 0:101"
    );

    let changes7 = task903_generate_stage26_probe_variant(
        "07_final_country_full_tuple_plus_following_paras.hwp",
        true,
        false,
        false,
        false,
        true,
        true,
    );
    assert_eq!(
        changes7.final_country_table_full_objects_with_tail, 1,
        "variant 07 must copy final country full table object"
    );
    assert_eq!(
        changes7.following_para_records, 4,
        "variant 07 must copy paragraphs 0:98 through 0:101"
    );
}

#[test]
fn task903_stage27_generate_section1_probe_variants() {
    let changes1 = task903_generate_stage27_probe_variant(
        "01_final_region_table_record_with_tail.hwp",
        false,
        true,
        false,
        false,
        false,
        false,
        false,
    );
    assert_eq!(
        changes1.final_region_table_records_with_tail, 1,
        "variant 01 must copy final region TABLE record with encoded zone tail"
    );

    let changes2 = task903_generate_stage27_probe_variant(
        "02_final_region_table_full_object_with_tail.hwp",
        false,
        false,
        true,
        false,
        false,
        false,
        false,
    );
    assert_eq!(
        changes2.final_region_table_full_objects_with_tail, 1,
        "variant 02 must copy final region full table object"
    );

    let changes3 = task903_generate_stage27_probe_variant(
        "03_final_region_host_para_plus_table_full_tuple.hwp",
        true,
        false,
        true,
        false,
        false,
        false,
        false,
    );
    assert_eq!(
        changes3.final_region_host_para_records, 1,
        "variant 03 must copy paragraph 0:102"
    );
    assert_eq!(
        changes3.final_region_table_full_objects_with_tail, 1,
        "variant 03 must copy final region full table object"
    );

    let changes4 = task903_generate_stage27_probe_variant(
        "04_section1_para0_record_without_controls.hwp",
        false,
        false,
        false,
        true,
        false,
        false,
        false,
    );
    assert_eq!(
        changes4.section1_para_records, 1,
        "variant 04 must copy section 1 paragraph 0 records"
    );

    let changes5 = task903_generate_stage27_probe_variant(
        "05_section1_table_record_with_tail.hwp",
        false,
        false,
        false,
        false,
        false,
        true,
        false,
    );
    assert_eq!(
        changes5.section1_table_records_with_tail, 1,
        "variant 05 must copy section 1 table record with encoded zone tail"
    );

    let changes6 = task903_generate_stage27_probe_variant(
        "06_section1_table_full_object_with_tail.hwp",
        false,
        false,
        false,
        false,
        false,
        false,
        true,
    );
    assert_eq!(
        changes6.section1_table_full_objects_with_tail, 1,
        "variant 06 must copy section 1 full table object"
    );

    let changes7 = task903_generate_stage27_probe_variant(
        "07_section1_full_para0_tuple.hwp",
        false,
        false,
        false,
        false,
        true,
        false,
        false,
    );
    assert_eq!(
        changes7.section1_full_paras, 1,
        "variant 07 must copy section 1 full paragraph tuple"
    );

    let changes8 = task903_generate_stage27_probe_variant(
        "08_final_region_full_plus_section1_table_record.hwp",
        false,
        false,
        true,
        false,
        false,
        true,
        false,
    );
    assert_eq!(
        changes8.final_region_table_full_objects_with_tail, 1,
        "variant 08 must copy final region full table object"
    );
    assert_eq!(
        changes8.section1_table_records_with_tail, 1,
        "variant 08 must copy section 1 table record"
    );

    let changes9 = task903_generate_stage27_probe_variant(
        "09_final_region_full_plus_section1_full_para0.hwp",
        false,
        false,
        true,
        false,
        true,
        false,
        false,
    );
    assert_eq!(
        changes9.final_region_table_full_objects_with_tail, 1,
        "variant 09 must copy final region full table object"
    );
    assert_eq!(
        changes9.section1_full_paras, 1,
        "variant 09 must copy section 1 full paragraph tuple"
    );
}

#[test]
fn task903_stage28_generate_container_probe_variants() {
    let changes1 = task903_generate_stage28_probe_variant(
        "01_compressed_header_only.hwp",
        true,
        false,
        false,
        &[],
    );
    assert!(changes1.compressed_header, "variant 01 must enable compression");

    let changes2 = task903_generate_stage28_probe_variant(
        "02_reference_file_header.hwp",
        false,
        true,
        false,
        &[],
    );
    assert!(changes2.reference_header, "variant 02 must copy reference FileHeader");

    let changes3 = task903_generate_stage28_probe_variant(
        "03_reference_docinfo.hwp",
        true,
        false,
        true,
        &[],
    );
    assert!(changes3.reference_docinfo, "variant 03 must copy reference DocInfo");

    let changes4 = task903_generate_stage28_probe_variant(
        "04_reference_file_header_docinfo.hwp",
        false,
        true,
        true,
        &[],
    );
    assert!(changes4.reference_header, "variant 04 must copy reference FileHeader");
    assert!(changes4.reference_docinfo, "variant 04 must copy reference DocInfo");

    let changes5 = task903_generate_stage28_probe_variant(
        "05_raw_graft_bodytext_section1.hwp",
        false,
        true,
        false,
        &["/BodyText/Section1"],
    );
    assert_eq!(
        changes5.raw_grafted_streams,
        vec!["/BodyText/Section1".to_string()],
        "variant 05 must graft BodyText/Section1"
    );

    let changes6 = task903_generate_stage28_probe_variant(
        "06_raw_graft_bodytext_section0.hwp",
        false,
        true,
        false,
        &["/BodyText/Section0"],
    );
    assert_eq!(
        changes6.raw_grafted_streams,
        vec!["/BodyText/Section0".to_string()],
        "variant 06 must graft BodyText/Section0"
    );

    let changes7 = task903_generate_stage28_probe_variant(
        "07_raw_graft_docinfo.hwp",
        false,
        true,
        false,
        &["/DocInfo"],
    );
    assert_eq!(
        changes7.raw_grafted_streams,
        vec!["/DocInfo".to_string()],
        "variant 07 must graft DocInfo stream"
    );

    let changes8 = task903_generate_stage28_probe_variant(
        "08_raw_graft_bodytext_section0_section1.hwp",
        false,
        true,
        false,
        &["/BodyText/Section0", "/BodyText/Section1"],
    );
    assert_eq!(
        changes8.raw_grafted_streams,
        vec![
            "/BodyText/Section0".to_string(),
            "/BodyText/Section1".to_string()
        ],
        "variant 08 must graft both BodyText streams"
    );

    let changes9 = task903_generate_stage28_probe_variant(
        "09_raw_graft_docinfo_bodytext.hwp",
        false,
        true,
        false,
        &["/DocInfo", "/BodyText/Section0", "/BodyText/Section1"],
    );
    assert_eq!(
        changes9.raw_grafted_streams,
        vec![
            "/DocInfo".to_string(),
            "/BodyText/Section0".to_string(),
            "/BodyText/Section1".to_string()
        ],
        "variant 09 must graft DocInfo and BodyText streams"
    );
}

#[test]
fn task903_stage29_generate_docinfo_probe_variants() {
    let variants = [
        (
            "01_reference_model_all_reserialized.hwp",
            Task903Stage29Patch::ReferenceModelAll,
        ),
        (
            "02_doc_properties_only.hwp",
            Task903Stage29Patch::DocPropertiesOnly,
        ),
        ("03_font_faces_only.hwp", Task903Stage29Patch::FontFacesOnly),
        (
            "04_border_fills_only.hwp",
            Task903Stage29Patch::BorderFillsOnly,
        ),
        ("05_char_shapes_only.hwp", Task903Stage29Patch::CharShapesOnly),
        ("06_para_shapes_only.hwp", Task903Stage29Patch::ParaShapesOnly),
        ("07_styles_only.hwp", Task903Stage29Patch::StylesOnly),
        ("08_layout_bundle.hwp", Task903Stage29Patch::LayoutBundle),
        (
            "09_tabs_numbering_bullets.hwp",
            Task903Stage29Patch::TabsNumberingBullets,
        ),
        (
            "10_bin_data_list_only.hwp",
            Task903Stage29Patch::BinDataListOnly,
        ),
        (
            "11_extra_records_only.hwp",
            Task903Stage29Patch::ExtraRecordsOnly,
        ),
        (
            "12_counts_extra_records_only.hwp",
            Task903Stage29Patch::CountsExtraRecordsOnly,
        ),
    ];

    for (output_name, patch) in variants {
        let changes = task903_generate_stage29_probe_variant(output_name, patch);
        assert_eq!(
            changes.patch_label,
            patch.label(),
            "{} must record patch label",
            output_name
        );
        assert!(
            changes.docinfo_forced_reserialize,
            "{} must force DocInfo reserialization",
            output_name
        );
    }
}

#[test]
fn task903_stage30_generate_minimal_docinfo_probe_variants() {
    let variants = [
        (
            "01_section_count_only.hwp",
            Task903Stage30Patch::SectionCountOnly,
        ),
        (
            "02_doc_properties_values_no_raw.hwp",
            Task903Stage30Patch::DocPropertiesValuesNoRaw,
        ),
        (
            "03_para_shapes_no_raw_only.hwp",
            Task903Stage30Patch::ParaShapesNoRawOnly,
        ),
        (
            "04_section_count_para_shapes_raw.hwp",
            Task903Stage30Patch::SectionCountParaShapesRaw,
        ),
        (
            "05_section_count_para_shapes_no_raw.hwp",
            Task903Stage30Patch::SectionCountParaShapesNoRaw,
        ),
        (
            "06_section_count_layout_bundle_raw.hwp",
            Task903Stage30Patch::SectionCountLayoutBundleRaw,
        ),
        (
            "07_section_count_layout_bundle_no_raw.hwp",
            Task903Stage30Patch::SectionCountLayoutBundleNoRaw,
        ),
        (
            "08_section_count_reference_model_all_no_raw.hwp",
            Task903Stage30Patch::SectionCountReferenceModelAllNoRaw,
        ),
    ];

    for (output_name, patch) in variants {
        let changes = task903_generate_stage30_probe_variant(output_name, patch);
        assert_eq!(
            changes.patch_label,
            patch.label(),
            "{} must record patch label",
            output_name
        );
        if !matches!(patch, Task903Stage30Patch::ParaShapesNoRawOnly) {
            assert_eq!(
                changes.section_count, 2,
                "{} must serialize section_count=2",
                output_name
            );
        }
    }
}

#[test]
fn task903_hwpx_h_01_section_count_is_materialized_by_adapter() {
    let bytes = load_sample("hwpx-h-01.hwpx");
    let mut core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");
    let expected_section_count = core.document().sections.len() as u16;

    assert_eq!(
        expected_section_count, 2,
        "fixture는 2개 section으로 구성되어야 함"
    );
    assert_ne!(
        core.document().doc_properties.section_count,
        expected_section_count,
        "HWPX parser baseline은 section_count를 아직 보정하지 않아야 함"
    );

    let report = convert_hwpx_to_hwp_ir(core.document_mut());

    assert_eq!(
        report.doc_properties_section_count_normalized, 1,
        "어댑터가 DocProperties.section_count를 한 번 보정해야 함"
    );
    assert_eq!(
        core.document().doc_properties.section_count,
        expected_section_count,
        "HWP 저장 전 DocProperties.section_count는 실제 section 수와 같아야 함"
    );
    assert!(
        core.document().doc_properties.raw_data.is_none(),
        "DOCUMENT_PROPERTIES raw_data가 남으면 보정값이 직렬화되지 않음"
    );
    assert!(
        core.document().doc_info.raw_stream_dirty,
        "DocInfo raw stream을 재직렬화해야 section_count 보정이 HWP에 반영됨"
    );
}

#[test]
fn task903_hwpx_h_01_para_shape_margin_children_are_parsed() {
    let bytes = load_sample("hwpx-h-01.hwpx");
    let core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");
    let reference_bytes =
        std::fs::read("samples/hwpx/hancom-hwp/hwpx-h-01.hwp").expect("한컴 정답 HWP 필요");
    let reference = DocumentCore::from_bytes(&reference_bytes).expect("한컴 정답 HWP 파싱 실패");

    let source_shapes = &core.document().doc_info.para_shapes;
    let reference_shapes = &reference.document().doc_info.para_shapes;
    assert!(
        source_shapes.len() > 20 && reference_shapes.len() > 20,
        "fixture는 ParaShape 20개 이상을 가져야 함"
    );

    for idx in [10usize, 17, 20] {
        let actual = &source_shapes[idx];
        let expected = &reference_shapes[idx];
        assert_eq!(actual.indent, expected.indent, "ParaShape[{}].indent", idx);
        assert_eq!(
            actual.margin_left, expected.margin_left,
            "ParaShape[{}].margin_left",
            idx
        );
        assert_eq!(
            actual.margin_right, expected.margin_right,
            "ParaShape[{}].margin_right",
            idx
        );
        assert_eq!(
            actual.spacing_before, expected.spacing_before,
            "ParaShape[{}].spacing_before",
            idx
        );
        assert_eq!(
            actual.spacing_after, expected.spacing_after,
            "ParaShape[{}].spacing_after",
            idx
        );
    }

    assert_eq!(source_shapes[10].indent, -2800);
    assert_eq!(source_shapes[17].margin_left, 3000);
    assert_eq!(source_shapes[20].spacing_after, 1000);
}

#[test]
fn task903_stage31_generate_impl_verify_hwpx_h_01() {
    let bytes = load_sample("hwpx-h-01.hwpx");
    let mut core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");
    let hwp_bytes = core.export_hwp_with_adapter().expect("HWP 직렬화 실패");

    let out_dir = std::path::Path::new("output/poc/hwpx2hwp/task903/stage31_impl_verify");
    std::fs::create_dir_all(out_dir).expect("Stage 31 output dir 생성 실패");
    let out_path = out_dir.join("hwpx-h-01.hwp");
    std::fs::write(&out_path, &hwp_bytes).expect("Stage 31 HWP 저장 실패");

    let reloaded = DocumentCore::from_bytes(&hwp_bytes).expect("HWP 재로드 실패");
    assert_eq!(
        reloaded.document().doc_properties.section_count,
        2,
        "Stage 31 산출물은 DOCUMENT_PROPERTIES section_count=2를 가져야 함"
    );
    assert_eq!(
        reloaded.page_count(),
        9,
        "Stage 31 산출물은 rhwp-studio 기준 9페이지를 유지해야 함"
    );

    eprintln!(
        "[#903 Stage 31] generated {} bytes={} pages={}",
        out_path.display(),
        hwp_bytes.len(),
        reloaded.page_count()
    );
}

#[test]
fn task903_stage32_generate_file_header_probe_variants() {
    let variants = [
        (
            "01_compressed_header_only.hwp",
            false,
            "compressed_header_only",
        ),
        ("02_reference_file_header.hwp", true, "reference_file_header"),
    ];

    for (output_name, use_reference_header, label) in variants {
        let bytes = load_sample("hwpx-h-01.hwpx");
        let mut core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");

        if use_reference_header {
            let reference = task903_reference_hwpx_h_01_core();
            task903_reference_header(&mut core, &reference);
        } else {
            task903_enable_hwp_compression(&mut core);
        }

        let hwp_bytes = core.export_hwp_with_adapter().expect("HWP 직렬화 실패");
        let out_dir =
            std::path::Path::new("output/poc/hwpx2hwp/task903/stage32_file_header_probe");
        std::fs::create_dir_all(out_dir).expect("Stage 32 output dir 생성 실패");
        let out_path = out_dir.join(output_name);
        std::fs::write(&out_path, &hwp_bytes).expect("Stage 32 HWP 저장 실패");

        let reloaded = DocumentCore::from_bytes(&hwp_bytes).expect("Stage 32 HWP 재로드 실패");
        assert!(
            reloaded.document().header.compressed,
            "{} must serialize compressed HWP",
            output_name
        );
        assert_eq!(
            reloaded.page_count(),
            9,
            "{} must keep 9 pages in rhwp reload",
            output_name
        );

        eprintln!(
            "[#903 Stage 32] {}: bytes={}, label={}, pages={}, compressed={}",
            out_path.display(),
            hwp_bytes.len(),
            label,
            reloaded.page_count(),
            reloaded.document().header.compressed
        );
    }
}

#[derive(Debug, Clone, Copy)]
enum Task903Stage33Patch {
    ClearTableAttrHighRepeatBit,
    ReferenceTableRecordAttrOnly,
    ReferenceTableRecordPayload,
}

impl Task903Stage33Patch {
    fn label(self) -> &'static str {
        match self {
            Task903Stage33Patch::ClearTableAttrHighRepeatBit => {
                "clear_table_attr_high_repeat_bit"
            }
            Task903Stage33Patch::ReferenceTableRecordAttrOnly => "reference_table_record_attr_only",
            Task903Stage33Patch::ReferenceTableRecordPayload => "reference_table_record_payload",
        }
    }
}

fn task903_apply_stage33_patch(core: &mut DocumentCore, reference: &DocumentCore, patch: Task903Stage33Patch) -> usize {
    let reference_tables = task903_collect_all_tables(reference);
    let mut idx = 0usize;
    let mut changed = 0usize;

    for section in &mut core.document_mut().sections {
        task903_visit_tables_mut(&mut section.paragraphs, &mut |table| {
            match patch {
                Task903Stage33Patch::ClearTableAttrHighRepeatBit => {
                    if table.raw_table_record_attr & 0x0400_0000 != 0 {
                        table.raw_table_record_attr &= !0x0400_0000;
                        table.attr &= !0x0400_0000;
                        changed += 1;
                    }
                }
                Task903Stage33Patch::ReferenceTableRecordAttrOnly => {
                    let reference_table = reference_tables
                        .get(idx)
                        .unwrap_or_else(|| panic!("reference table[{}] 없음", idx));
                    table.attr = reference_table.attr;
                    table.page_break = reference_table.page_break;
                    table.repeat_header = reference_table.repeat_header;
                    table.raw_table_record_attr = reference_table.raw_table_record_attr;
                    changed += 1;
                }
                Task903Stage33Patch::ReferenceTableRecordPayload => {
                    let reference_table = reference_tables
                        .get(idx)
                        .unwrap_or_else(|| panic!("reference table[{}] 없음", idx));
                    task903_copy_table_record_payload_with_encoded_tail(table, reference_table);
                    changed += 1;
                }
            }
            idx += 1;
        });
    }

    if !matches!(patch, Task903Stage33Patch::ClearTableAttrHighRepeatBit) {
        assert_eq!(
            idx,
            reference_tables.len(),
            "target/reference table count must match"
        );
    }

    changed
}

#[test]
fn task903_stage33_generate_table_attr_probe_variants() {
    let variants = [
        (
            "01_clear_table_attr_high_repeat_bit.hwp",
            Task903Stage33Patch::ClearTableAttrHighRepeatBit,
        ),
        (
            "02_reference_table_record_attr_only.hwp",
            Task903Stage33Patch::ReferenceTableRecordAttrOnly,
        ),
        (
            "03_reference_table_record_payload.hwp",
            Task903Stage33Patch::ReferenceTableRecordPayload,
        ),
    ];

    for (output_name, patch) in variants {
        let bytes = load_sample("hwpx-h-01.hwpx");
        let mut core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");
        let reference = task903_reference_hwpx_h_01_core();

        task903_enable_hwp_compression(&mut core);
        convert_if_hwpx_source(core.document_mut(), rhwp::parser::FileFormat::Hwpx);
        let changed = task903_apply_stage33_patch(&mut core, &reference, patch);

        let hwp_bytes = core.export_hwp_native().expect("Stage 33 HWP 직렬화 실패");
        let out_dir = std::path::Path::new("output/poc/hwpx2hwp/task903/stage33_table_attr_probe");
        std::fs::create_dir_all(out_dir).expect("Stage 33 output dir 생성 실패");
        let out_path = out_dir.join(output_name);
        std::fs::write(&out_path, &hwp_bytes).expect("Stage 33 HWP 저장 실패");

        let reloaded = DocumentCore::from_bytes(&hwp_bytes).expect("Stage 33 HWP 재로드 실패");
        assert_eq!(reloaded.page_count(), 9, "{} page count", output_name);

        eprintln!(
            "[#903 Stage 33] {}: bytes={}, patch={}, changed={}, pages={}",
            out_path.display(),
            hwp_bytes.len(),
            patch.label(),
            changed,
            reloaded.page_count()
        );
    }
}

fn task903_stage34_write_probe(output_name: &str, mut core: DocumentCore) -> usize {
    let hwp_bytes = core.export_hwp_native().expect("Stage 34 HWP 직렬화 실패");
    let out_dir =
        std::path::Path::new("output/poc/hwpx2hwp/task903/stage34_baseline_reconcile");
    std::fs::create_dir_all(out_dir).expect("Stage 34 output dir 생성 실패");
    let out_path = out_dir.join(output_name);
    std::fs::write(&out_path, &hwp_bytes).expect("Stage 34 HWP 저장 실패");

    let reloaded = DocumentCore::from_bytes(&hwp_bytes).expect("Stage 34 HWP 재로드 실패");
    assert_eq!(reloaded.page_count(), 9, "{} page count", output_name);

    eprintln!(
        "[#903 Stage 34] {}: bytes={}, pages={}, section_count={}",
        out_path.display(),
        hwp_bytes.len(),
        reloaded.page_count(),
        reloaded.document().doc_properties.section_count
    );

    hwp_bytes.len()
}

fn task903_stage34_clean_adapter_core() -> DocumentCore {
    let bytes = load_sample("hwpx-h-01.hwpx");
    let mut core = DocumentCore::from_bytes(&bytes).expect("HWPX 로드 실패");
    task903_enable_hwp_compression(&mut core);
    convert_if_hwpx_source(core.document_mut(), rhwp::parser::FileFormat::Hwpx);
    core
}

fn task903_stage34_stage27_baseline_core() -> DocumentCore {
    let base_bytes = task903_stage27_baseline_section1_bytes();
    let mut core = DocumentCore::from_bytes(&base_bytes).expect("Stage 27 baseline HWP 재로드 실패");
    task903_enable_hwp_compression(&mut core);
    core
}

#[test]
fn task903_stage34_generate_baseline_reconcile_variants() {
    let reference = task903_reference_hwpx_h_01_core();

    let clean = task903_stage34_clean_adapter_core();
    task903_stage34_write_probe("01_clean_adapter_compressed.hwp", clean);

    let mut clean_para_shapes = task903_stage34_clean_adapter_core();
    task903_apply_stage30_patch(
        &mut clean_para_shapes,
        &reference,
        Task903Stage30Patch::SectionCountParaShapesNoRaw,
    );
    task903_stage34_write_probe(
        "02_clean_adapter_plus_section_count_para_shapes_no_raw.hwp",
        clean_para_shapes,
    );

    let mut clean_docinfo = task903_stage34_clean_adapter_core();
    task903_apply_stage30_patch(
        &mut clean_docinfo,
        &reference,
        Task903Stage30Patch::SectionCountReferenceModelAllNoRaw,
    );
    task903_stage34_write_probe(
        "03_clean_adapter_plus_reference_docinfo_no_raw.hwp",
        clean_docinfo,
    );

    let mut control_para_shapes = task903_stage34_stage27_baseline_core();
    task903_apply_stage30_patch(
        &mut control_para_shapes,
        &reference,
        Task903Stage30Patch::SectionCountParaShapesNoRaw,
    );
    task903_stage34_write_probe(
        "04_stage27_baseline_plus_section_count_para_shapes_no_raw.hwp",
        control_para_shapes,
    );

    let mut control_docinfo = task903_stage34_stage27_baseline_core();
    task903_apply_stage30_patch(
        &mut control_docinfo,
        &reference,
        Task903Stage30Patch::SectionCountReferenceModelAllNoRaw,
    );
    task903_stage34_write_probe(
        "05_stage27_baseline_plus_reference_docinfo_no_raw.hwp",
        control_docinfo,
    );
}

const TASK903_STAGE27_TOP_LEVEL_PARA_RECORDS: &[usize] = &[
    10, 13, 14, 15, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 43, 44, 45, 48, 49, 50, 51,
    52, 53, 54, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95, 96, 97, 98, 99, 100, 101,
];

fn task903_stage35_apply_top_level_para_records(
    core: &mut DocumentCore,
    reference: &DocumentCore,
) -> usize {
    let mut changed = 0usize;
    for &para_idx in TASK903_STAGE27_TOP_LEVEL_PARA_RECORDS {
        changed += task903_materialize_top_level_para_record_from_reference(
            core, reference, para_idx,
        );
    }
    changed
}

fn task903_stage35_apply_section_def_core(core: &mut DocumentCore) -> usize {
    let mut changed = 0usize;
    for section in &mut core.document_mut().sections {
        changed += task903_materialize_stage8_section_def_core(section);
    }
    changed
}

fn task903_stage35_apply_common_table_object_records(
    core: &mut DocumentCore,
    reference: &DocumentCore,
) -> usize {
    let reference_first_table = task903_hwpx_h_01_first_table(reference).clone();
    let reference_second_table = task903_hwpx_h_01_second_table(reference).clone();
    let reference_chart_table = task903_hwpx_h_01_chart_table(reference).clone();
    let reference_industry_table = task903_hwpx_h_01_industry_table(reference).clone();
    let reference_country_table = task903_hwpx_h_01_country_table(reference).clone();
    let reference_region_table = task903_hwpx_h_01_region_table(reference).clone();
    let reference_notice_table = task903_hwpx_h_01_notice_table(reference).clone();
    let reference_logo_group = task903_hwpx_h_01_logo_group(reference).clone();
    let reference_attachment_table = task903_hwpx_h_01_attachment_title_table(reference).clone();
    let reference_top_country_table = task903_hwpx_h_01_top_country_table(reference).clone();
    let reference_year_trend_table = task903_hwpx_h_01_year_trend_table(reference).clone();
    let reference_second_year_trend_table =
        task903_hwpx_h_01_second_year_trend_table(reference).clone();
    let reference_final_industry_table = task903_hwpx_h_01_final_industry_table(reference).clone();
    let reference_final_country_table = task903_hwpx_h_01_final_country_table(reference).clone();

    let mut changed = 0usize;
    changed += task903_stage35_apply_section_def_core(core);
    changed += task903_materialize_first_cell_list_header_tail_65(core);
    changed += task903_materialize_first_table_picture_patch_from_reference(
        core,
        &reference_first_table,
        0,
        Task903FirstTablePicturePatch::Full,
    );
    changed += task903_materialize_first_table_picture_patch_from_reference(
        core,
        &reference_first_table,
        1,
        Task903FirstTablePicturePatch::Full,
    );
    changed += task903_materialize_first_table_ctrl_header_from_reference(
        core,
        &reference_first_table,
    );
    changed += task903_materialize_first_table_cell_list_headers_from_reference(
        core,
        &reference_first_table,
    );
    changed += task903_materialize_first_table_record_from_reference(core, &reference_first_table);
    changed += task903_materialize_first_table_cell_para_headers_from_reference(
        core,
        &reference_first_table,
    );
    changed += task903_materialize_next_table_child_headers_from_reference(
        core,
        &reference_second_table,
    );
    changed += task903_materialize_chart_table_full_object_with_encoded_tail_from_reference(
        core,
        &reference_chart_table,
    );
    changed += task903_materialize_industry_table_full_object_with_encoded_tail_from_reference(
        core,
        &reference_industry_table,
    );
    changed += task903_materialize_country_table_full_object_with_encoded_tail_from_reference(
        core,
        &reference_country_table,
    );
    changed += task903_materialize_region_table_full_object_with_encoded_tail_from_reference(
        core,
        &reference_region_table,
    );
    changed += task903_materialize_notice_table_full_object_with_encoded_tail_from_reference(
        core,
        &reference_notice_table,
    );
    changed += task903_materialize_logo_group_full_object_from_reference(
        core,
        &reference_logo_group,
    );
    changed +=
        task903_materialize_attachment_title_table_full_object_with_encoded_tail_from_reference(
            core,
            &reference_attachment_table,
        );
    changed += task903_materialize_top_country_table_full_object_with_encoded_tail_from_reference(
        core,
        &reference_top_country_table,
    );
    changed += task903_materialize_year_trend_table_full_object_with_encoded_tail_from_reference(
        core,
        &reference_year_trend_table,
    );
    changed +=
        task903_materialize_second_year_trend_table_full_object_with_encoded_tail_from_reference(
            core,
            &reference_second_year_trend_table,
        );
    changed += task903_materialize_final_industry_table_full_object_with_encoded_tail_from_reference(
        core,
        &reference_final_industry_table,
    );
    changed += task903_materialize_final_country_table_full_object_with_encoded_tail_from_reference(
        core,
        &reference_final_country_table,
    );

    changed
}

fn task903_stage35_apply_final_region_table(
    core: &mut DocumentCore,
    reference: &DocumentCore,
) -> usize {
    let reference_final_region_table = task903_hwpx_h_01_final_region_table(reference).clone();
    task903_materialize_final_region_table_full_object_with_encoded_tail_from_reference(
        core,
        &reference_final_region_table,
    )
}

fn task903_stage35_apply_section1_full_para(
    core: &mut DocumentCore,
    reference: &DocumentCore,
) -> usize {
    task903_materialize_full_para_from_reference(core, reference, 1, 0)
}

fn task903_stage35_write_probe(
    output_name: &str,
    mut core: DocumentCore,
    changed: usize,
) -> usize {
    let hwp_bytes = core.export_hwp_native().expect("Stage 35 HWP 직렬화 실패");
    let out_dir =
        std::path::Path::new("output/poc/hwpx2hwp/task903/stage35_stage27_block_probe");
    std::fs::create_dir_all(out_dir).expect("Stage 35 output dir 생성 실패");
    let out_path = out_dir.join(output_name);
    std::fs::write(&out_path, &hwp_bytes).expect("Stage 35 HWP 저장 실패");

    let reloaded = DocumentCore::from_bytes(&hwp_bytes).expect("Stage 35 HWP 재로드 실패");
    assert_eq!(reloaded.page_count(), 9, "{} page count", output_name);

    eprintln!(
        "[#903 Stage 35] {}: bytes={}, changed={}, pages={}, section_count={}",
        out_path.display(),
        hwp_bytes.len(),
        changed,
        reloaded.page_count(),
        reloaded.document().doc_properties.section_count
    );

    hwp_bytes.len()
}

#[test]
fn task903_stage35_generate_stage27_block_probe_variants() {
    let reference = task903_reference_hwpx_h_01_core();

    let mut top_level_para_only = task903_stage34_clean_adapter_core();
    let mut changed = task903_stage35_apply_top_level_para_records(
        &mut top_level_para_only,
        &reference,
    );
    task903_apply_stage30_patch(
        &mut top_level_para_only,
        &reference,
        Task903Stage30Patch::SectionCountParaShapesNoRaw,
    );
    task903_stage35_write_probe(
        "01_top_level_para_records_only.hwp",
        top_level_para_only,
        changed,
    );

    let mut table_object_only = task903_stage34_clean_adapter_core();
    changed = task903_stage35_apply_common_table_object_records(
        &mut table_object_only,
        &reference,
    );
    task903_apply_stage30_patch(
        &mut table_object_only,
        &reference,
        Task903Stage30Patch::SectionCountParaShapesNoRaw,
    );
    task903_stage35_write_probe(
        "02_table_object_records_only.hwp",
        table_object_only,
        changed,
    );

    let mut para_plus_table = task903_stage34_clean_adapter_core();
    changed = task903_stage35_apply_top_level_para_records(&mut para_plus_table, &reference);
    changed += task903_stage35_apply_common_table_object_records(&mut para_plus_table, &reference);
    task903_apply_stage30_patch(
        &mut para_plus_table,
        &reference,
        Task903Stage30Patch::SectionCountParaShapesNoRaw,
    );
    task903_stage35_write_probe(
        "03_para_plus_table_object_records.hwp",
        para_plus_table,
        changed,
    );

    let mut common_without_section1 = task903_stage34_clean_adapter_core();
    changed = task903_stage35_apply_top_level_para_records(
        &mut common_without_section1,
        &reference,
    );
    changed += task903_stage35_apply_common_table_object_records(
        &mut common_without_section1,
        &reference,
    );
    changed += task903_stage35_apply_final_region_table(&mut common_without_section1, &reference);
    task903_apply_stage30_patch(
        &mut common_without_section1,
        &reference,
        Task903Stage30Patch::SectionCountParaShapesNoRaw,
    );
    task903_stage35_write_probe(
        "04_common_stage27_without_section1.hwp",
        common_without_section1,
        changed,
    );

    let mut full_control = task903_stage34_clean_adapter_core();
    changed = task903_stage35_apply_top_level_para_records(&mut full_control, &reference);
    changed += task903_stage35_apply_common_table_object_records(&mut full_control, &reference);
    changed += task903_stage35_apply_final_region_table(&mut full_control, &reference);
    changed += task903_stage35_apply_section1_full_para(&mut full_control, &reference);
    task903_apply_stage30_patch(
        &mut full_control,
        &reference,
        Task903Stage30Patch::SectionCountParaShapesNoRaw,
    );
    task903_stage35_write_probe("05_full_stage27_control.hwp", full_control, changed);
}

fn task903_stage36_apply_section_def_first_cell_tail(core: &mut DocumentCore) -> usize {
    let mut changed = task903_stage35_apply_section_def_core(core);
    changed += task903_materialize_first_cell_list_header_tail_65(core);
    changed
}

fn task903_stage36_apply_first_table_full(
    core: &mut DocumentCore,
    reference: &DocumentCore,
) -> usize {
    let reference_first_table = task903_hwpx_h_01_first_table(reference).clone();
    let mut changed = task903_materialize_first_cell_list_header_tail_65(core);
    changed += task903_materialize_first_table_picture_patch_from_reference(
        core,
        &reference_first_table,
        0,
        Task903FirstTablePicturePatch::Full,
    );
    changed += task903_materialize_first_table_picture_patch_from_reference(
        core,
        &reference_first_table,
        1,
        Task903FirstTablePicturePatch::Full,
    );
    changed += task903_materialize_first_table_ctrl_header_from_reference(
        core,
        &reference_first_table,
    );
    changed += task903_materialize_first_table_cell_list_headers_from_reference(
        core,
        &reference_first_table,
    );
    changed += task903_materialize_first_table_record_from_reference(core, &reference_first_table);
    changed += task903_materialize_first_table_cell_para_headers_from_reference(
        core,
        &reference_first_table,
    );
    changed
}

fn task903_stage36_apply_second_table_child_headers(
    core: &mut DocumentCore,
    reference: &DocumentCore,
) -> usize {
    let reference_second_table = task903_hwpx_h_01_second_table(reference).clone();
    task903_materialize_next_table_child_headers_from_reference(core, &reference_second_table)
}

fn task903_stage36_apply_chart_to_notice_tables(
    core: &mut DocumentCore,
    reference: &DocumentCore,
) -> usize {
    let reference_chart_table = task903_hwpx_h_01_chart_table(reference).clone();
    let reference_industry_table = task903_hwpx_h_01_industry_table(reference).clone();
    let reference_country_table = task903_hwpx_h_01_country_table(reference).clone();
    let reference_region_table = task903_hwpx_h_01_region_table(reference).clone();
    let reference_notice_table = task903_hwpx_h_01_notice_table(reference).clone();

    let mut changed = 0usize;
    changed += task903_materialize_chart_table_full_object_with_encoded_tail_from_reference(
        core,
        &reference_chart_table,
    );
    changed += task903_materialize_industry_table_full_object_with_encoded_tail_from_reference(
        core,
        &reference_industry_table,
    );
    changed += task903_materialize_country_table_full_object_with_encoded_tail_from_reference(
        core,
        &reference_country_table,
    );
    changed += task903_materialize_region_table_full_object_with_encoded_tail_from_reference(
        core,
        &reference_region_table,
    );
    changed += task903_materialize_notice_table_full_object_with_encoded_tail_from_reference(
        core,
        &reference_notice_table,
    );
    changed
}

fn task903_stage36_apply_logo_attachment_tables(
    core: &mut DocumentCore,
    reference: &DocumentCore,
) -> usize {
    let reference_logo_group = task903_hwpx_h_01_logo_group(reference).clone();
    let reference_attachment_table = task903_hwpx_h_01_attachment_title_table(reference).clone();
    let mut changed =
        task903_materialize_logo_group_full_object_from_reference(core, &reference_logo_group);
    changed +=
        task903_materialize_attachment_title_table_full_object_with_encoded_tail_from_reference(
            core,
            &reference_attachment_table,
        );
    changed
}

fn task903_stage36_apply_late_tables(
    core: &mut DocumentCore,
    reference: &DocumentCore,
) -> usize {
    let reference_top_country_table = task903_hwpx_h_01_top_country_table(reference).clone();
    let reference_year_trend_table = task903_hwpx_h_01_year_trend_table(reference).clone();
    let reference_second_year_trend_table =
        task903_hwpx_h_01_second_year_trend_table(reference).clone();
    let reference_final_industry_table = task903_hwpx_h_01_final_industry_table(reference).clone();
    let reference_final_country_table = task903_hwpx_h_01_final_country_table(reference).clone();

    let mut changed = 0usize;
    changed += task903_materialize_top_country_table_full_object_with_encoded_tail_from_reference(
        core,
        &reference_top_country_table,
    );
    changed += task903_materialize_year_trend_table_full_object_with_encoded_tail_from_reference(
        core,
        &reference_year_trend_table,
    );
    changed +=
        task903_materialize_second_year_trend_table_full_object_with_encoded_tail_from_reference(
            core,
            &reference_second_year_trend_table,
        );
    changed += task903_materialize_final_industry_table_full_object_with_encoded_tail_from_reference(
        core,
        &reference_final_industry_table,
    );
    changed += task903_materialize_final_country_table_full_object_with_encoded_tail_from_reference(
        core,
        &reference_final_country_table,
    );
    changed
}

fn task903_stage36_write_probe(
    output_name: &str,
    mut core: DocumentCore,
    changed: usize,
) -> usize {
    task903_apply_stage30_patch(
        &mut core,
        &task903_reference_hwpx_h_01_core(),
        Task903Stage30Patch::SectionCountParaShapesNoRaw,
    );

    let hwp_bytes = core.export_hwp_native().expect("Stage 36 HWP 직렬화 실패");
    let out_dir =
        std::path::Path::new("output/poc/hwpx2hwp/task903/stage36_table_object_block_probe");
    std::fs::create_dir_all(out_dir).expect("Stage 36 output dir 생성 실패");
    let out_path = out_dir.join(output_name);
    std::fs::write(&out_path, &hwp_bytes).expect("Stage 36 HWP 저장 실패");

    let reloaded = DocumentCore::from_bytes(&hwp_bytes).expect("Stage 36 HWP 재로드 실패");
    assert_eq!(reloaded.page_count(), 9, "{} page count", output_name);

    eprintln!(
        "[#903 Stage 36] {}: bytes={}, changed={}, pages={}, section_count={}",
        out_path.display(),
        hwp_bytes.len(),
        changed,
        reloaded.page_count(),
        reloaded.document().doc_properties.section_count
    );

    hwp_bytes.len()
}

#[test]
fn task903_stage36_generate_table_object_block_probe_variants() {
    let reference = task903_reference_hwpx_h_01_core();

    let mut core = task903_stage34_clean_adapter_core();
    let changed = task903_stage36_apply_section_def_first_cell_tail(&mut core);
    task903_stage36_write_probe("01_section_def_first_cell_tail_only.hwp", core, changed);

    let mut core = task903_stage34_clean_adapter_core();
    let changed = task903_stage36_apply_first_table_full(&mut core, &reference);
    task903_stage36_write_probe("02_first_table_full_only.hwp", core, changed);

    let mut core = task903_stage34_clean_adapter_core();
    let changed = task903_stage36_apply_second_table_child_headers(&mut core, &reference);
    task903_stage36_write_probe("03_second_table_child_headers_only.hwp", core, changed);

    let mut core = task903_stage34_clean_adapter_core();
    let changed = task903_stage36_apply_chart_to_notice_tables(&mut core, &reference);
    task903_stage36_write_probe("04_chart_to_notice_tables_only.hwp", core, changed);

    let mut core = task903_stage34_clean_adapter_core();
    let changed = task903_stage36_apply_logo_attachment_tables(&mut core, &reference);
    task903_stage36_write_probe("05_logo_attachment_tables_only.hwp", core, changed);

    let mut core = task903_stage34_clean_adapter_core();
    let changed = task903_stage36_apply_late_tables(&mut core, &reference);
    task903_stage36_write_probe("06_late_tables_only.hwp", core, changed);

    let mut core = task903_stage34_clean_adapter_core();
    let mut changed = task903_stage36_apply_section_def_first_cell_tail(&mut core);
    changed += task903_stage36_apply_first_table_full(&mut core, &reference);
    changed += task903_stage36_apply_second_table_child_headers(&mut core, &reference);
    changed += task903_stage36_apply_chart_to_notice_tables(&mut core, &reference);
    task903_stage36_write_probe("07_first_second_chart_to_notice.hwp", core, changed);

    let mut core = task903_stage34_clean_adapter_core();
    let mut changed = task903_stage35_apply_section_def_core(&mut core);
    changed += task903_stage36_apply_second_table_child_headers(&mut core, &reference);
    changed += task903_stage36_apply_chart_to_notice_tables(&mut core, &reference);
    changed += task903_stage36_apply_logo_attachment_tables(&mut core, &reference);
    changed += task903_stage36_apply_late_tables(&mut core, &reference);
    task903_stage36_write_probe("08_all_tables_without_first_table.hwp", core, changed);

    let mut core = task903_stage34_clean_adapter_core();
    let mut changed = task903_stage36_apply_section_def_first_cell_tail(&mut core);
    changed += task903_stage36_apply_first_table_full(&mut core, &reference);
    changed += task903_stage36_apply_second_table_child_headers(&mut core, &reference);
    changed += task903_stage36_apply_chart_to_notice_tables(&mut core, &reference);
    changed += task903_stage36_apply_logo_attachment_tables(&mut core, &reference);
    task903_stage36_write_probe("09_all_tables_without_late_tables.hwp", core, changed);
}
