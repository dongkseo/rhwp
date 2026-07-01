//! Issue #1692: HWP3 글자색 인덱스가 CharShape.text_color로 보존되는지 검증한다.

use rhwp::parser::parse_document;

fn load(path: &str) -> rhwp::model::document::Document {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    parse_document(&bytes).unwrap_or_else(|e| panic!("parse {path}: {e:?}"))
}

#[test]
fn issue_1692_so_sueop_hwp3_preserves_blue_text_color_like_hwpx_reference() {
    let hwp3_doc = load("samples/SO-SUEOP.hwp");
    let hwpx_doc = load("samples/SO-SUEOP.hwpx");

    let blue = 0x00FF0000;
    let hwp3_blue_count = hwp3_doc
        .doc_info
        .char_shapes
        .iter()
        .filter(|cs| cs.text_color == blue)
        .count();
    let hwpx_blue_count = hwpx_doc
        .doc_info
        .char_shapes
        .iter()
        .filter(|cs| cs.text_color == blue)
        .count();

    assert!(
        hwp3_blue_count > 0,
        "SO-SUEOP.hwp must preserve HWP3 blue text_color into CharShape.text_color"
    );
    assert!(
        hwpx_blue_count > 0,
        "SO-SUEOP.hwpx reference must contain blue CharShape.text_color"
    );
    assert!(
        hwp3_doc
            .doc_info
            .char_shapes
            .iter()
            .any(|cs| cs.text_color == 0),
        "existing black text CharShape must remain available"
    );
}
