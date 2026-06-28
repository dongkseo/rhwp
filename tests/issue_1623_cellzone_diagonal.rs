use std::path::{Path, PathBuf};

use rhwp::model::control::Control;
use rhwp::model::style::CenterLine;
use rhwp::{parse_document, wasm_api::HwpDocument, DocumentCore};
use serde_json::Value;

fn sample_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(name)
}

fn read_sample(name: &str) -> Vec<u8> {
    std::fs::read(sample_path(name)).unwrap_or_else(|err| panic!("{name} 읽기 실패: {err}"))
}

fn first_table(doc: &rhwp::model::document::Document) -> &rhwp::model::table::Table {
    doc.sections
        .iter()
        .flat_map(|section| &section.paragraphs)
        .flat_map(|para| &para.controls)
        .find_map(|control| match control {
            Control::Table(table) => Some(table),
            _ => None,
        })
        .expect("대각선샘플 표를 찾지 못함")
}

fn line_attr(line: &str, key: &str) -> Option<f64> {
    let needle = format!("{key}=\"");
    let start = line.find(&needle)? + needle.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    rest[..end].parse().ok()
}

fn rendered_lines(svg: &str) -> Vec<(f64, f64, f64, f64)> {
    svg.split("<line")
        .skip(1)
        .filter_map(|part| {
            let tag = part.split('>').next()?;
            Some((
                line_attr(tag, "x1")?,
                line_attr(tag, "y1")?,
                line_attr(tag, "x2")?,
                line_attr(tag, "y2")?,
            ))
        })
        .collect()
}

#[test]
fn issue_1623_sample_preserves_centerline_and_cellzones() {
    let doc = parse_document(&read_sample("samples/대각선샘플.hwpx")).expect("HWPX 파싱 실패");
    let table = first_table(&doc);

    assert_eq!(table.zones.len(), 2, "cellzoneList 보존");
    assert_eq!(table.zones[0].border_fill_id, 9);
    assert_eq!(table.zones[1].border_fill_id, 11);
    assert!(
        table.cells.iter().any(|cell| cell.border_fill_id == 10),
        "개별 셀 중심선 BorderFill 참조 보존"
    );

    let bf9 = &doc.doc_info.border_fills[8];
    let bf10 = &doc.doc_info.border_fills[9];
    let bf11 = &doc.doc_info.border_fills[10];
    let bf5 = &doc.doc_info.border_fills[4];
    assert_eq!((bf5.attr >> 8) & 0x03, 2, "bf5 slash Crooked=2 보존");
    assert_eq!((bf5.attr >> 5) & 0x07, 0b010, "bf5 backSlash CENTER");
    assert_eq!((bf9.attr >> 2) & 0x07, 0b010, "zone #9 slash");
    assert_eq!((bf9.attr >> 5) & 0x07, 0b010, "zone #9 backSlash");
    assert_eq!(bf10.center_line, CenterLine::Vertical);
    assert_eq!((bf11.attr >> 2) & 0x07, 0b010, "zone #11 slash");
    assert_eq!((bf11.attr >> 5) & 0x07, 0b010, "zone #11 backSlash");
}

#[test]
fn issue_1623_cellzone_diagonal_renders_across_zone_bbox() {
    for sample in ["samples/대각선샘플.hwpx", "samples/대각선샘플.hwp"] {
        let core = DocumentCore::from_bytes(&read_sample(sample))
            .unwrap_or_else(|err| panic!("{sample}: {err}"));
        let svg = core
            .render_page_svg_native(0)
            .unwrap_or_else(|err| panic!("{sample} SVG 렌더 실패: {err}"));
        let lines = rendered_lines(&svg);

        assert!(
            lines
                .iter()
                .any(|(x1, y1, x2, y2)| { (x1 - x2).abs() > 250.0 && (y1 - y2).abs() > 250.0 }),
            "{sample}: cellzone 전체 bbox를 가로지르는 대각선이 없음"
        );
    }
}

#[test]
fn issue_1633_crooked_diagonal_renders_middle_segment() {
    for sample in ["samples/대각선샘플.hwpx", "samples/대각선샘플.hwp"] {
        let core = DocumentCore::from_bytes(&read_sample(sample))
            .unwrap_or_else(|err| panic!("{sample}: {err}"));
        let svg = core
            .render_page_svg_native(0)
            .unwrap_or_else(|err| panic!("{sample} SVG 렌더 실패: {err}"));
        let lines = rendered_lines(&svg);

        assert!(
            lines.iter().any(|(x1, y1, x2, y2)| {
                let len = (x2 - x1).abs();
                (y1 - y2).abs() < 0.01 && (20.0..=35.0).contains(&len) && *y1 < 350.0
            }),
            "{sample}: 첫 줄 두 번째 칸의 꺾은 대각선 가운데 수평 선분이 없음"
        );
    }
}

#[test]
fn issue_1633_get_cell_properties_reflects_cellzone_diagonal() {
    for sample in ["samples/대각선샘플.hwpx", "samples/대각선샘플.hwp"] {
        let bytes = read_sample(sample);
        let parsed = parse_document(&bytes).unwrap_or_else(|err| panic!("{sample}: {err}"));
        let table = first_table(&parsed);
        let cell_idx = table
            .cells
            .iter()
            .position(|cell| cell.row == 2 && cell.col == 2)
            .unwrap_or_else(|| panic!("{sample}: row=2 col=2 셀을 찾지 못함"));
        assert_ne!(
            table.cells[cell_idx].border_fill_id, 11,
            "{sample}: 회귀 가드는 cellzone overlay 조회를 검증해야 함"
        );

        let doc = HwpDocument::from_bytes(&bytes).unwrap_or_else(|err| panic!("{sample}: {err}"));
        let json = doc
            .get_cell_properties(0, 0, 2, cell_idx as u32)
            .unwrap_or_else(|err| panic!("{sample}: getCellProperties 실패: {err:?}"));
        let props: Value = serde_json::from_str(&json).unwrap_or_else(|err| {
            panic!("{sample}: getCellProperties JSON 파싱 실패: {err}: {json}")
        });

        assert_eq!(props["borderFillId"].as_u64(), Some(11), "{sample}: {json}");
        assert_eq!(props["diagonalLine"].as_u64(), Some(10), "{sample}: {json}");
        assert_eq!(props["diagonalSlash"].as_u64(), Some(2), "{sample}: {json}");
        assert_eq!(
            props["diagonalBackSlash"].as_u64(),
            Some(2),
            "{sample}: {json}"
        );
        assert_eq!(
            props["diagonalWidth"].as_u64(),
            Some(13),
            "{sample}: {json}"
        );
        assert_eq!(
            props["diagonalColor"].as_str(),
            Some("#000000"),
            "{sample}: {json}"
        );
    }
}

#[test]
fn issue_1633_as_one_cell_diagonal_uses_cellzone_range() {
    let mut doc = HwpDocument::create_empty();
    let created = doc
        .create_table_ex_native(
            0,
            0,
            0,
            3,
            3,
            true,
            Some(&[10000, 10000, 10000]),
            Some(&[7200, 7200, 7200]),
        )
        .expect("표 생성");
    let created: Value = serde_json::from_str(&created).expect("createTable JSON");
    let ppi = created["paraIdx"].as_u64().expect("paraIdx") as u32;
    let ci = created["controlIdx"].as_u64().expect("controlIdx") as u32;
    let props = r##"{
        "borderFillId":1,
        "borderLeft":{"type":1,"width":0,"color":"#000000"},
        "borderRight":{"type":1,"width":0,"color":"#000000"},
        "borderTop":{"type":1,"width":0,"color":"#000000"},
        "borderBottom":{"type":1,"width":0,"color":"#000000"},
        "fillType":"none",
        "diagonalLine":1,
        "diagonalSlash":2,
        "diagonalBackSlash":2,
        "diagonalWidth":0,
        "diagonalColor":"#000000",
        "centerLine":"NONE"
    }"##;

    doc.set_cell_zone_properties(0, ppi, ci, 0, 0, 1, 1, props)
        .expect("cellzone 적용");

    let table = first_table(doc.document());
    let zone = table.zones.last().expect("asOne cellzone");
    assert_eq!(
        (zone.start_row, zone.start_col, zone.end_row, zone.end_col),
        (0, 0, 1, 1)
    );

    let cell_props = doc
        .get_cell_properties(0, ppi, ci, 4)
        .expect("zone 내부 셀 속성 조회");
    let cell_props: Value = serde_json::from_str(&cell_props).expect("cell props JSON");
    assert_eq!(cell_props["diagonalLine"].as_u64(), Some(1));
    assert_eq!(cell_props["diagonalSlash"].as_u64(), Some(2));
    assert_eq!(cell_props["diagonalBackSlash"].as_u64(), Some(2));

    let svg = doc.render_page_svg_native(0).expect("SVG 렌더");
    let lines = rendered_lines(&svg);
    assert!(
        lines
            .iter()
            .any(|(x1, y1, x2, y2)| (x1 - x2).abs() > 200.0 && (y1 - y2).abs() > 150.0),
        "asOne 대각선은 첫 셀이 아니라 선택 영역 전체 cellzone bbox를 가로질러야 함"
    );
}
