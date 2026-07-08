//! Issue #2069: 한셀 OLE 미리보기 RawSvg/placeholder도 선택 가능한 개체여야 한다.
//!
//! `samples/한셀OLE.hwp`/`.hwpx`는 빈 문단에 비-TAC OLE 하나가 놓인 형태다.
//! 렌더 트리는 OLE preview를 RawSvg로 만들지만, 원본 control 좌표를 잃으면 Studio가
//! 클릭 선택/개체 속성 진입을 할 수 없고 빈 문단 커서 rect도 찾지 못한다.

use std::fs;
use std::path::Path;

use rhwp::document_core::DocumentCore;
use serde_json::Value;

fn load_core(rel: &str) -> DocumentCore {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    let bytes = fs::read(&path).unwrap_or_else(|e| panic!("read {}: {}", rel, e));
    DocumentCore::from_bytes(&bytes).unwrap_or_else(|e| panic!("parse {}: {:?}", rel, e))
}

fn assert_ole_layout_and_caret(rel: &str) {
    let core = load_core(rel);
    let layout_json = core
        .get_page_control_layout_native(0)
        .unwrap_or_else(|e| panic!("layout {}: {:?}", rel, e));
    let layout: Value = serde_json::from_str(&layout_json)
        .unwrap_or_else(|e| panic!("parse layout {} `{}`: {}", rel, layout_json, e));
    let controls = layout["controls"]
        .as_array()
        .unwrap_or_else(|| panic!("layout controls missing for {}", rel));
    let ole = controls
        .iter()
        .find(|control| control["type"] == "ole")
        .unwrap_or_else(|| panic!("OLE control missing for {}: {}", rel, layout_json));

    assert_eq!(ole["secIdx"], 0, "OLE section index");
    assert_eq!(ole["paraIdx"], 0, "OLE paragraph index");
    assert_eq!(ole["controlIdx"], 2, "OLE control index");
    assert!(
        ole["w"].as_f64().unwrap_or_default() > 300.0
            && ole["h"].as_f64().unwrap_or_default() > 30.0,
        "OLE bbox should expose the preview area: {}",
        ole
    );

    let cursor_json = core
        .get_cursor_rect_native(0, 0, 0)
        .unwrap_or_else(|e| panic!("cursor {}: {:?}", rel, e));
    let cursor: Value = serde_json::from_str(&cursor_json)
        .unwrap_or_else(|e| panic!("parse cursor {} `{}`: {}", rel, cursor_json, e));
    let expected_x = ole["x"].as_f64().unwrap() + ole["w"].as_f64().unwrap();
    let actual_x = cursor["x"].as_f64().unwrap();
    let expected_y = ole["y"].as_f64().unwrap();
    let actual_y = cursor["y"].as_f64().unwrap();

    assert_eq!(cursor["pageIndex"], 0, "cursor page index");
    assert!(
        (actual_x - expected_x).abs() <= 0.6,
        "cursor x should be at OLE right edge for {}: cursor={}, ole={}",
        rel,
        cursor,
        ole
    );
    assert!(
        (actual_y - expected_y).abs() <= 0.6,
        "cursor y should follow OLE top for {}: cursor={}, ole={}",
        rel,
        cursor,
        ole
    );
}

#[test]
fn hwp_ole_preview_is_selectable_and_drives_empty_para_caret() {
    assert_ole_layout_and_caret("samples/한셀OLE.hwp");
}

#[test]
fn hwpx_ole_preview_is_selectable_and_drives_empty_para_caret() {
    assert_ole_layout_and_caret("samples/한셀OLE.hwpx");
}
