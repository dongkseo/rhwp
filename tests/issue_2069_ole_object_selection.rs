//! Issue #2069: 한셀 OLE 미리보기 RawSvg/placeholder도 선택 가능한 개체여야 한다.
//!
//! `samples/한셀OLE.hwp`/`.hwpx`는 빈 문단에 비-TAC OLE 하나가 놓인 형태다.
//! 렌더 트리는 OLE preview를 RawSvg로 만들지만, 원본 control 좌표를 잃으면 Studio가
//! 클릭 선택/개체 속성 진입을 할 수 없고 빈 문단 커서 rect도 찾지 못한다.

use std::fs;
use std::path::Path;

use rhwp::document_core::DocumentCore;
use rhwp::model::control::Control;
use rhwp::model::shape::{ShapeObject, TextWrap};
use rhwp::renderer::hwpunit_to_px;
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};
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
    let ole_left = ole["x"].as_f64().unwrap();
    let expected_x = ole_left + ole["w"].as_f64().unwrap();
    let actual_x = cursor["x"].as_f64().unwrap();
    let expected_y = ole["y"].as_f64().unwrap();
    let actual_y = cursor["y"].as_f64().unwrap();
    let cursor_h = cursor["height"].as_f64().unwrap();
    let ole_h = ole["h"].as_f64().unwrap();

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
    assert!(
        (10.0..ole_h / 2.0).contains(&cursor_h),
        "cursor height should use text line metrics, not full OLE height for {}: cursor={}, ole={}",
        rel,
        cursor,
        ole
    );

    fn collect_para_end_anchors<'a>(node: &'a RenderNode, out: &mut Vec<&'a RenderNode>) {
        if let RenderNodeType::TextRun(run) = &node.node_type {
            if run.text.is_empty()
                && run.section_index == Some(0)
                && run.para_index == Some(0)
                && run.char_start == Some(0)
                && run.is_para_end
            {
                out.push(node);
            }
        }
        for child in &node.children {
            collect_para_end_anchors(child, out);
        }
    }

    let tree = core
        .build_page_render_tree(0)
        .unwrap_or_else(|e| panic!("render tree {}: {:?}", rel, e));
    let mut anchors = Vec::new();
    collect_para_end_anchors(&tree.root, &mut anchors);
    assert_eq!(
        anchors.len(),
        1,
        "OLE paragraph mark count should follow the stored paragraph structure, not preview rows for {}",
        rel
    );
    let anchor = anchors[0];
    assert!(
        (anchor.bbox.x - expected_x).abs() <= 0.6,
        "paragraph mark anchor should follow OLE right edge for {}: anchor={:?}, ole={}",
        rel,
        anchor.bbox,
        ole
    );
    assert!(
        (anchor.bbox.y - expected_y).abs() <= 0.6,
        "paragraph mark anchor should follow the single stored paragraph line for {}: anchor={:?}, ole={}",
        rel,
        anchor.bbox,
        ole
    );
}

fn collect_para_end_anchors<'a>(node: &'a RenderNode, out: &mut Vec<&'a RenderNode>) {
    if let RenderNodeType::TextRun(run) = &node.node_type {
        if run.text.is_empty() && run.section_index == Some(0) && run.is_para_end {
            out.push(node);
        }
    }
    for child in &node.children {
        collect_para_end_anchors(child, out);
    }
}

fn assert_enter_after_square_ole_keeps_wrap_zone(rel: &str) {
    let mut core = load_core(rel);
    let original_line_seg = core.document().sections[0].paragraphs[0].line_segs[0].clone();
    assert!(
        original_line_seg.column_start > 0 && original_line_seg.segment_width > 0,
        "{} should encode the OLE-side wrap zone in LINE_SEG: {:?}",
        rel,
        original_line_seg
    );
    let expected_line_pitch_px = hwpunit_to_px(
        original_line_seg.line_height + original_line_seg.line_spacing,
        96.0,
    );

    core.split_paragraph_native(0, 0, 0)
        .unwrap_or_else(|e| panic!("split after OLE {}: {:?}", rel, e));

    let section = &core.document().sections[0];
    assert_eq!(
        section.paragraphs.len(),
        2,
        "Enter after OLE should create one following paragraph for {}",
        rel
    );
    assert!(
        matches!(
            section.paragraphs[0].controls.get(2),
            Some(Control::Shape(shape))
                if matches!(shape.as_ref(), ShapeObject::Ole(_))
                    && matches!(shape.common().text_wrap, TextWrap::Square)
                    && !shape.common().treat_as_char
        ),
        "Square OLE should stay anchored to the original paragraph after Enter for {}",
        rel
    );
    assert!(
        section.paragraphs[1].controls.is_empty(),
        "the paragraph inserted by Enter should be an empty following paragraph for {}",
        rel
    );
    assert_eq!(
        section.paragraphs[1].line_segs[0].column_start, original_line_seg.column_start,
        "the following empty paragraph should preserve the stored wrap-zone x for {}",
        rel
    );
    assert_eq!(
        section.paragraphs[1].line_segs[0].segment_width, original_line_seg.segment_width,
        "the following empty paragraph should preserve the stored wrap-zone width for {}",
        rel
    );

    let layout_json = core
        .get_page_control_layout_native(0)
        .unwrap_or_else(|e| panic!("layout after Enter {}: {:?}", rel, e));
    let layout: Value = serde_json::from_str(&layout_json)
        .unwrap_or_else(|e| panic!("parse layout {} `{}`: {}", rel, layout_json, e));
    let controls = layout["controls"]
        .as_array()
        .unwrap_or_else(|| panic!("layout controls missing after Enter for {}", rel));
    let ole = controls
        .iter()
        .find(|control| control["type"] == "ole")
        .unwrap_or_else(|| {
            panic!(
                "OLE control missing after Enter for {}: {}",
                rel, layout_json
            )
        });
    assert_eq!(ole["paraIdx"], 0, "OLE should remain in paragraph 0");

    let ole_left = ole["x"].as_f64().unwrap();
    let expected_x = ole_left + ole["w"].as_f64().unwrap();
    let tree = core
        .build_page_render_tree(0)
        .unwrap_or_else(|e| panic!("render tree after Enter {}: {:?}", rel, e));
    let mut anchors = Vec::new();
    collect_para_end_anchors(&tree.root, &mut anchors);
    let para0_anchor = anchors
        .iter()
        .find(|node| {
            matches!(
                &node.node_type,
                RenderNodeType::TextRun(run)
                    if run.para_index == Some(0) && run.char_start == Some(0)
            )
        })
        .unwrap_or_else(|| panic!("original paragraph mark missing after Enter for {}", rel));
    let para1_anchor = anchors
        .iter()
        .find(|node| {
            matches!(
                &node.node_type,
                RenderNodeType::TextRun(run)
                    if run.para_index == Some(1) && run.char_start == Some(0)
            )
        })
        .unwrap_or_else(|| panic!("following paragraph mark missing after Enter for {}", rel));

    assert!(
        (para0_anchor.bbox.x - expected_x).abs() <= 0.6,
        "original OLE paragraph mark should remain at OLE right edge for {}: anchor={:?}, ole={}",
        rel,
        para0_anchor.bbox,
        ole
    );
    assert!(
        (para1_anchor.bbox.x - expected_x).abs() <= 0.6,
        "Enter-created paragraph mark should stay in the OLE-side wrap zone for {}: anchor={:?}, ole={}",
        rel,
        para1_anchor.bbox,
        ole
    );
    assert!(
        para1_anchor.bbox.y > para0_anchor.bbox.y,
        "Enter-created paragraph mark should be on the following line for {}: para0={:?}, para1={:?}",
        rel,
        para0_anchor.bbox,
        para1_anchor.bbox
    );
    assert!(
        (para1_anchor.bbox.y - para0_anchor.bbox.y - expected_line_pitch_px).abs() <= 1.0,
        "Enter-created paragraph mark should follow the stored OLE line pitch for {}: para0={:?}, para1={:?}, pitch={:.2}",
        rel,
        para0_anchor.bbox,
        para1_anchor.bbox,
        expected_line_pitch_px
    );

    let cursor_json = core
        .get_cursor_rect_native(0, 1, 0)
        .unwrap_or_else(|e| panic!("cursor after Enter {}: {:?}", rel, e));
    let cursor: Value = serde_json::from_str(&cursor_json)
        .unwrap_or_else(|e| panic!("parse cursor after Enter {} `{}`: {}", rel, cursor_json, e));
    let actual_cursor_x = cursor["x"].as_f64().unwrap();
    let actual_cursor_y = cursor["y"].as_f64().unwrap();
    assert!(
        (actual_cursor_x - expected_x).abs() <= 0.6,
        "active caret after Enter should stay in the OLE-side wrap zone for {}: cursor={}, ole={}",
        rel,
        cursor,
        ole
    );
    assert!(
        actual_cursor_y > para0_anchor.bbox.y,
        "active caret after Enter should move to the following line for {}: cursor={}, para0={:?}",
        rel,
        cursor,
        para0_anchor.bbox
    );

    core.split_paragraph_native(0, 1, 0)
        .unwrap_or_else(|e| panic!("second split after OLE {}: {:?}", rel, e));

    let section = &core.document().sections[0];
    assert_eq!(
        section.paragraphs.len(),
        3,
        "two consecutive Enters after OLE should create two following paragraphs for {}",
        rel
    );
    assert!(
        section.paragraphs[2].controls.is_empty(),
        "the second Enter-created paragraph should be empty for {}",
        rel
    );
    assert_eq!(
        section.paragraphs[2].line_segs[0].column_start, original_line_seg.column_start,
        "the second following paragraph should preserve the stored wrap-zone x for {}",
        rel
    );
    assert_eq!(
        section.paragraphs[2].line_segs[0].segment_width, original_line_seg.segment_width,
        "the second following paragraph should preserve the stored wrap-zone width for {}",
        rel
    );

    let tree = core
        .build_page_render_tree(0)
        .unwrap_or_else(|e| panic!("render tree after second Enter {}: {:?}", rel, e));
    let mut anchors = Vec::new();
    collect_para_end_anchors(&tree.root, &mut anchors);
    let para1_anchor_after_second = anchors
        .iter()
        .find(|node| {
            matches!(
                &node.node_type,
                RenderNodeType::TextRun(run)
                    if run.para_index == Some(1) && run.char_start == Some(0)
            )
        })
        .unwrap_or_else(|| {
            panic!(
                "first following paragraph mark missing after second Enter for {}",
                rel
            )
        });
    let para2_anchor = anchors
        .iter()
        .find(|node| {
            matches!(
                &node.node_type,
                RenderNodeType::TextRun(run)
                    if run.para_index == Some(2) && run.char_start == Some(0)
            )
        })
        .unwrap_or_else(|| {
            panic!(
                "second following paragraph mark missing after second Enter for {}",
                rel
            )
        });

    assert!(
        (para1_anchor_after_second.bbox.x - expected_x).abs() <= 0.6,
        "first following paragraph mark should remain in the OLE-side wrap zone after second Enter for {}: anchor={:?}, ole={}",
        rel,
        para1_anchor_after_second.bbox,
        ole
    );
    assert!(
        (para2_anchor.bbox.x - expected_x).abs() <= 0.6,
        "second following paragraph mark should stay in the OLE-side wrap zone for {}: anchor={:?}, ole={}",
        rel,
        para2_anchor.bbox,
        ole
    );
    assert!(
        para2_anchor.bbox.y > para1_anchor_after_second.bbox.y,
        "second following paragraph mark should be below the first for {}: para1={:?}, para2={:?}",
        rel,
        para1_anchor_after_second.bbox,
        para2_anchor.bbox
    );
    assert!(
        (para2_anchor.bbox.y
            - para1_anchor_after_second.bbox.y
            - expected_line_pitch_px)
            .abs()
            <= 1.0,
        "second following paragraph mark should follow the stored OLE line pitch for {}: para1={:?}, para2={:?}, pitch={:.2}",
        rel,
        para1_anchor_after_second.bbox,
        para2_anchor.bbox,
        expected_line_pitch_px
    );

    let cursor_json = core
        .get_cursor_rect_native(0, 2, 0)
        .unwrap_or_else(|e| panic!("cursor after second Enter {}: {:?}", rel, e));
    let cursor: Value = serde_json::from_str(&cursor_json).unwrap_or_else(|e| {
        panic!(
            "parse cursor after second Enter {} `{}`: {}",
            rel, cursor_json, e
        )
    });
    let actual_cursor_x = cursor["x"].as_f64().unwrap();
    let actual_cursor_y = cursor["y"].as_f64().unwrap();
    assert!(
        (actual_cursor_x - expected_x).abs() <= 0.6,
        "active caret after second Enter should stay in the OLE-side wrap zone for {}: cursor={}, ole={}",
        rel,
        cursor,
        ole
    );
    assert!(
        actual_cursor_y > para1_anchor_after_second.bbox.y,
        "active caret after second Enter should move to the second following line for {}: cursor={}, para1={:?}",
        rel,
        cursor,
        para1_anchor_after_second.bbox
    );

    core.split_paragraph_native(0, 2, 0)
        .unwrap_or_else(|e| panic!("third split after OLE {}: {:?}", rel, e));

    let section = &core.document().sections[0];
    assert_eq!(
        section.paragraphs.len(),
        4,
        "three consecutive Enters after OLE should create three following paragraphs for {}",
        rel
    );
    assert!(
        section.paragraphs[3].controls.is_empty(),
        "the paragraph created after leaving the OLE height should be empty for {}",
        rel
    );
    assert_eq!(
        section.paragraphs[2].line_segs[0].column_start, original_line_seg.column_start,
        "the last line that overlaps the OLE height should still preserve the wrap-zone x for {}",
        rel
    );
    assert_eq!(
        section.paragraphs[3].line_segs[0].column_start, 0,
        "the first line below the OLE height should return to normal body flow for {}",
        rel
    );
    assert!(
        section.paragraphs[3].line_segs[0].segment_width > original_line_seg.segment_width,
        "the first line below the OLE height should recover body-width line metrics for {}: {:?}",
        rel,
        section.paragraphs[3].line_segs[0]
    );

    let tree = core
        .build_page_render_tree(0)
        .unwrap_or_else(|e| panic!("render tree after third Enter {}: {:?}", rel, e));
    let mut anchors = Vec::new();
    collect_para_end_anchors(&tree.root, &mut anchors);
    let para2_anchor_after_third = anchors
        .iter()
        .find(|node| {
            matches!(
                &node.node_type,
                RenderNodeType::TextRun(run)
                    if run.para_index == Some(2) && run.char_start == Some(0)
            )
        })
        .unwrap_or_else(|| {
            panic!(
                "second following paragraph mark missing after third Enter for {}",
                rel
            )
        });
    let para3_anchor = anchors
        .iter()
        .find(|node| {
            matches!(
                &node.node_type,
                RenderNodeType::TextRun(run)
                    if run.para_index == Some(3) && run.char_start == Some(0)
            )
        })
        .unwrap_or_else(|| {
            panic!(
                "body-flow paragraph mark missing after third Enter for {}",
                rel
            )
        });

    assert!(
        (para2_anchor_after_third.bbox.x - expected_x).abs() <= 0.6,
        "the final OLE-overlapping paragraph mark should stay at OLE right edge for {}: anchor={:?}, ole={}",
        rel,
        para2_anchor_after_third.bbox,
        ole
    );
    assert!(
        para3_anchor.bbox.x <= ole_left + 0.6 && expected_x - para3_anchor.bbox.x > 100.0,
        "paragraph mark below the OLE height should return to body-left flow for {}: anchor={:?}, ole={}",
        rel,
        para3_anchor.bbox,
        ole
    );
    assert!(
        para3_anchor.bbox.y > para2_anchor_after_third.bbox.y,
        "body-flow paragraph mark should be below the last OLE-overlapping line for {}: para2={:?}, para3={:?}",
        rel,
        para2_anchor_after_third.bbox,
        para3_anchor.bbox
    );
    assert!(
        (para3_anchor.bbox.y - para2_anchor_after_third.bbox.y - expected_line_pitch_px).abs()
            <= 1.0,
        "first body-flow paragraph mark below OLE should continue the stored line pitch for {}: para2={:?}, para3={:?}, pitch={:.2}",
        rel,
        para2_anchor_after_third.bbox,
        para3_anchor.bbox,
        expected_line_pitch_px
    );

    let cursor_json = core
        .get_cursor_rect_native(0, 3, 0)
        .unwrap_or_else(|e| panic!("cursor after third Enter {}: {:?}", rel, e));
    let cursor: Value = serde_json::from_str(&cursor_json).unwrap_or_else(|e| {
        panic!(
            "parse cursor after third Enter {} `{}`: {}",
            rel, cursor_json, e
        )
    });
    let actual_cursor_x = cursor["x"].as_f64().unwrap();
    let actual_cursor_y = cursor["y"].as_f64().unwrap();
    assert!(
        actual_cursor_x <= ole_left + 0.6 && expected_x - actual_cursor_x > 100.0,
        "active caret below the OLE height should return to body-left flow for {}: cursor={}, ole={}",
        rel,
        cursor,
        ole
    );
    assert!(
        actual_cursor_y > para2_anchor_after_third.bbox.y,
        "active caret after third Enter should move below the last OLE-overlapping line for {}: cursor={}, para2={:?}",
        rel,
        cursor,
        para2_anchor_after_third.bbox
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

#[test]
fn hwp_enter_after_square_ole_respects_height_boundary() {
    assert_enter_after_square_ole_keeps_wrap_zone("samples/한셀OLE.hwp");
}

#[test]
fn hwpx_enter_after_square_ole_respects_height_boundary() {
    assert_enter_after_square_ole_keeps_wrap_zone("samples/한셀OLE.hwpx");
}
