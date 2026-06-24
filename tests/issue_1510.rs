//! Issue #1510: visible text 가 있는 한 문단에 co-anchored para-relative
//! TopAndBottom floating 표가 여러 개 있을 때, vertical_offset 정렬/누적으로
//! 페이지가 늘어나거나 표 순서가 뒤집히는 회귀를 막는다.

use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};
use std::fs;
use std::path::Path;

const SAMPLE: &str = "samples/issue1510_coanchored_float_tables.hwp";
const TARGET_PI: usize = 0;
const TARGET_TABLES: [usize; 3] = [2, 3, 4];

fn load_doc() -> rhwp::wasm_api::HwpDocument {
    let repo_root = env!("CARGO_MANIFEST_DIR");
    let hwp_path = Path::new(repo_root).join(SAMPLE);
    let bytes = fs::read(&hwp_path).unwrap_or_else(|e| panic!("read {}: {}", SAMPLE, e));
    rhwp::wasm_api::HwpDocument::from_bytes(&bytes)
        .unwrap_or_else(|e| panic!("parse {}: {}", SAMPLE, e))
}

fn collect_table_order(root: &RenderNode, out: &mut Vec<usize>) {
    if let RenderNodeType::Table(table) = &root.node_type {
        if table.para_index == Some(TARGET_PI) {
            if let Some(ci) = table.control_index {
                if TARGET_TABLES.contains(&ci) {
                    out.push(ci);
                }
            }
        }
    }
    for child in &root.children {
        collect_table_order(child, out);
    }
}

#[test]
fn issue_1510_coanchored_visible_para_float_tables_stay_on_one_page() {
    let doc = load_doc();

    assert_eq!(
        doc.page_count(),
        1,
        "{} should match the Hancom 2024 HWP PDF baseline as a one-page document",
        SAMPLE,
    );
}

#[test]
fn issue_1510_visible_para_float_tables_keep_document_order() {
    let doc = load_doc();
    let tree = doc
        .build_page_render_tree(0)
        .expect("build_page_render_tree(0)");
    let mut order = Vec::new();
    collect_table_order(&tree.root, &mut order);

    assert_eq!(
        order, TARGET_TABLES,
        "co-anchored visible-host float tables should retain document/control order",
    );
}
