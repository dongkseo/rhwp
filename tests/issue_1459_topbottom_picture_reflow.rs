//! Issue #1459: 같은 문단의 TAC 그림과 자리차지 그림 혼합 배치.

use rhwp::document_core::DocumentCore;
use rhwp::model::control::Control;
use rhwp::model::shape::TextWrap;
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};

#[derive(Debug, Clone, Copy)]
struct ImageRender {
    control_index: usize,
    y: f64,
    height: f64,
    opacity: f64,
}

fn collect_images(node: &RenderNode, out: &mut Vec<ImageRender>) {
    if let RenderNodeType::Image(img) = &node.node_type {
        if let Some(control_index) = img.control_index {
            out.push(ImageRender {
                control_index,
                y: node.bbox.y,
                height: node.bbox.height,
                opacity: img.opacity,
            });
        }
    }
    for child in &node.children {
        collect_images(child, out);
    }
}

fn load_fixture(path: &str) -> DocumentCore {
    let repo_root = env!("CARGO_MANIFEST_DIR");
    let path = std::path::Path::new(repo_root).join(path);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    DocumentCore::from_bytes(&bytes).unwrap_or_else(|e| panic!("load {}: {e}", path.display()))
}

#[test]
fn topbottom_second_picture_flows_before_tac_picture() {
    for path in [
        "samples/투명도0-50-2nd그림글차처럼off.hwp",
        "samples/투명도0-50-2nd그림글차처럼off.hwpx",
    ] {
        let core = load_fixture(path);
        let para = &core.document().sections[0].paragraphs[0];
        let mut tac_picture = None;
        let mut topbottom_picture = None;
        for (ci, ctrl) in para.controls.iter().enumerate() {
            if let Control::Picture(pic) = ctrl {
                if pic.common.treat_as_char {
                    tac_picture = Some((ci, pic.image_attr.clamped_transparency()));
                } else if matches!(pic.common.text_wrap, TextWrap::TopAndBottom) {
                    topbottom_picture = Some((ci, pic.image_attr.clamped_transparency()));
                }
            }
        }
        let (tac_ci, tac_transparency) =
            tac_picture.unwrap_or_else(|| panic!("{path}: TAC 그림 누락"));
        let (topbottom_ci, topbottom_transparency) =
            topbottom_picture.unwrap_or_else(|| panic!("{path}: 자리차지 그림 누락"));
        assert_eq!(
            tac_transparency, 0,
            "{path}: 첫 TAC 그림은 투명도 0이어야 함"
        );
        assert_eq!(
            topbottom_transparency, 50,
            "{path}: 두 번째 자리차지 그림은 투명도 50이어야 함"
        );

        let tree = core
            .build_page_render_tree(0)
            .unwrap_or_else(|e| panic!("render tree {path}: {e}"));
        let mut images = Vec::new();
        collect_images(&tree.root, &mut images);

        let topbottom = images
            .iter()
            .find(|img| img.control_index == topbottom_ci)
            .unwrap_or_else(|| panic!("{path}: 자리차지 그림 ImageNode 누락: {images:?}"));
        let tac = images
            .iter()
            .find(|img| img.control_index == tac_ci)
            .unwrap_or_else(|| panic!("{path}: TAC 그림 ImageNode 누락: {images:?}"));

        assert!(
            topbottom.y < tac.y,
            "{path}: 한컴처럼 투명도 50 자리차지 그림이 먼저 흐르고 TAC 그림이 아래에 있어야 함: topbottom={topbottom:?}, tac={tac:?}, all={images:?}"
        );
        let vertical_gap = tac.y - (topbottom.y + topbottom.height);
        assert!(
            vertical_gap.abs() <= 2.0,
            "{path}: TAC 그림은 자리차지 그림 bbox 바로 다음 줄에 이어져야 함: gap={vertical_gap:.2}, topbottom={topbottom:?}, tac={tac:?}, all={images:?}"
        );
        assert!(
            topbottom.opacity < tac.opacity,
            "{path}: 자리차지 그림은 투명도 50%, TAC 그림은 투명도 0% 렌더여야 함: topbottom={topbottom:?}, tac={tac:?}"
        );
    }
}
