//! Regression guards for `samples/rowbreak-problem-pages.hwpx`.
//!
//! The first chart-like TAC table on page 2 (`pi=5 ci=0`) must start below the
//! preceding `<민간 SaaS 연계공통기반 운영체계>` title line. Otherwise the chart
//! border and image are painted under that title text.

use rhwp::renderer::render_tree::{BoundingBox, RenderNode, RenderNodeType};
use std::fs;
use std::path::Path;

const SAMPLE: &str = "samples/rowbreak-problem-pages.hwpx";
const PAGE_INDEX: u32 = 1;

fn find_table_bbox(root: &RenderNode, target_pi: usize, target_ci: usize) -> Option<BoundingBox> {
    if let RenderNodeType::Table(t) = &root.node_type {
        if t.para_index == Some(target_pi) && t.control_index == Some(target_ci) {
            return Some(root.bbox);
        }
    }

    root.children
        .iter()
        .find_map(|child| find_table_bbox(child, target_pi, target_ci))
}

fn find_textrun_bbox_containing(root: &RenderNode, needle: &str) -> Option<BoundingBox> {
    if let RenderNodeType::TextRun(run) = &root.node_type {
        if run.text.contains(needle) {
            return Some(root.bbox);
        }
    }

    root.children
        .iter()
        .find_map(|child| find_textrun_bbox_containing(child, needle))
}

fn collect_table_cells<'a>(
    root: &'a RenderNode,
    target_pi: usize,
    target_ci: usize,
) -> Vec<&'a RenderNode> {
    if let RenderNodeType::Table(t) = &root.node_type {
        if t.para_index == Some(target_pi) && t.control_index == Some(target_ci) {
            return root
                .children
                .iter()
                .filter(|child| matches!(child.node_type, RenderNodeType::TableCell(_)))
                .collect();
        }
    }

    root.children
        .iter()
        .find_map(|child| {
            let cells = collect_table_cells(child, target_pi, target_ci);
            (!cells.is_empty()).then_some(cells)
        })
        .unwrap_or_default()
}

fn collect_text(node: &RenderNode, out: &mut String) {
    if let RenderNodeType::TextRun(run) = &node.node_type {
        out.push_str(&run.text);
    }
    for child in &node.children {
        collect_text(child, out);
    }
}

fn text_line_exists(root: &RenderNode, needle: &str) -> bool {
    if matches!(root.node_type, RenderNodeType::TextLine(_)) {
        let mut text = String::new();
        collect_text(root, &mut text);
        if text.contains(needle) {
            return true;
        }
    }

    root.children
        .iter()
        .any(|child| text_line_exists(child, needle))
}

fn text_line_bbox_containing(root: &RenderNode, needle: &str) -> Option<BoundingBox> {
    if matches!(root.node_type, RenderNodeType::TextLine(_)) {
        let mut text = String::new();
        collect_text(root, &mut text);
        if text.contains(needle) {
            return Some(root.bbox);
        }
    }

    root.children
        .iter()
        .find_map(|child| text_line_bbox_containing(child, needle))
}

#[test]
fn rowbreak_page2_chart_starts_below_title_line() {
    let repo_root = env!("CARGO_MANIFEST_DIR");
    let sample_path = Path::new(repo_root).join(SAMPLE);
    let bytes = fs::read(&sample_path).unwrap_or_else(|e| panic!("read {}: {}", SAMPLE, e));
    let doc = rhwp::wasm_api::HwpDocument::from_bytes(&bytes)
        .unwrap_or_else(|e| panic!("parse {}: {:?}", SAMPLE, e));
    let tree = doc
        .build_page_render_tree(PAGE_INDEX)
        .unwrap_or_else(|e| panic!("render page {}: {}", PAGE_INDEX + 1, e));

    let chart =
        find_table_bbox(&tree.root, 5, 0).expect("page 2 chart table pi=5 ci=0 should render");
    let title = find_textrun_bbox_containing(&tree.root, "연계공통기반 운영체계")
        .expect("page 2 chart title text should render");

    let title_bottom = title.y + title.height;
    assert!(
        chart.y >= title_bottom - 0.5,
        "page 2 chart overlaps title text: title=[{:.2}..{:.2}], chart_y={:.2}",
        title.y,
        title_bottom,
        chart.y,
    );
}

#[test]
fn rowbreak_page7_nested_table_paragraph_keeps_host_text() {
    let repo_root = env!("CARGO_MANIFEST_DIR");
    let sample_path = Path::new(repo_root).join(SAMPLE);
    let bytes = fs::read(&sample_path).unwrap_or_else(|e| panic!("read {}: {}", SAMPLE, e));
    let doc = rhwp::wasm_api::HwpDocument::from_bytes(&bytes)
        .unwrap_or_else(|e| panic!("parse {}: {:?}", SAMPLE, e));
    let page7 = doc
        .build_page_render_tree(6)
        .unwrap_or_else(|e| panic!("render page 7: {e}"));
    let page8 = doc
        .build_page_render_tree(7)
        .unwrap_or_else(|e| panic!("render page 8: {e}"));

    let cells = collect_table_cells(&page7.root, 21, 0);
    assert!(
        !cells.is_empty(),
        "page 7 rowbreak table pi=21 ci=0 should render cells"
    );
    assert!(
        cells
            .iter()
            .any(|cell| text_line_exists(cell, "1. 「정보통신망")),
        "row 25 should keep the host paragraph text before its nested reference table"
    );
    assert!(
        !cells
            .iter()
            .any(|cell| text_line_exists(cell, "2. 이용자 정보가 유출된 때")),
        "row 25 page-7 fragment should end before item 2; item 2 belongs to the continued fragment"
    );

    let continued_cells = collect_table_cells(&page8.root, 21, 0);
    assert!(
        continued_cells
            .iter()
            .any(|cell| text_line_exists(cell, "2. 이용자 정보가 유출된 때")),
        "continued row 25 fragment should start with item 2"
    );
    assert!(
        continued_cells
            .iter()
            .any(|cell| text_line_exists(cell, "② 클라우드컴퓨팅서비스")),
        "continued row 25 fragment should keep circled-2 text before row 26"
    );
    assert!(
        continued_cells
            .iter()
            .any(|cell| text_line_exists(cell, "② 정보통신서비스")),
        "row 26 should keep the circled-2 host paragraph text before its nested reference table"
    );

    let row26_tail = text_line_bbox_containing(&page8.root, "도록 권고할 수 있다")
        .expect("row 26 tail text should render before row 27");
    let row27_start = text_line_bbox_containing(&page8.root, "법원의 제출명령이나")
        .expect("row 27 first text should render after row 26");
    assert!(
        row26_tail.y + row26_tail.height <= row27_start.y,
        "row 26 tail overlaps row 27: row26 bottom={:.2}, row27 top={:.2}",
        row26_tail.y + row26_tail.height,
        row27_start.y
    );
}
