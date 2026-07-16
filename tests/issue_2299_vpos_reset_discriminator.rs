//! `recalculate_section_vpos` 의 vpos 리셋 판별 — 양방향 회귀 테스트.
//!
//! 이 함수는 편집 후 문단들의 vpos 를 다시 쌓는다. 그러면서 "여기서 단/쪽이 새로 시작한다"는
//! 신호를 저장된 vpos 로 추정한다. 그 추정이 양쪽으로 틀릴 수 있고, **두 실패는 서로 반대
//! 방향으로 당긴다**:
//!
//! - 너무 좁게 잡으면 (#2299) 다단 문서를 편집할 때 단-밴드가 소멸해 쪽수가 뛴다.
//! - 너무 넓게 잡으면 평범한 문단들이 단 경계로 오인되어 못 박히고, 아래로 밀려야 할 때
//!   안 밀린다 (줄간격을 키워도 쪽이 안 늘어난다).
//!
//! `<` 는 전자, `<=` 는 후자로 실패한다. vpos 동일성만으로는 두 상황이 구분되지 않는다 —
//! 어느 쪽이든 직전 문단과 vpos 가 같다. 판별에는 vpos 밖의 근거가 필요하다.
//!
//! 두 방향을 함께 잠근다. 한쪽만 있으면 반대쪽으로 넘어지는 수정이 통과한다.

use rhwp::document_core::DocumentCore;
use std::fs;
use std::path::Path;

fn load(rel: &str) -> DocumentCore {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    let bytes = fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    DocumentCore::from_bytes(&bytes).unwrap_or_else(|e| panic!("parse {rel}: {e}"))
}

/// 방향 1 — 좁게 잡으면 깨진다: 다단 문서 앞쪽 편집 시 단-밴드 소멸.
///
/// `samples/basic/shortcut.hwp` 는 2단 zone 7쪽 문서다. 앞쪽 문단을 편집하면 우측 단이
/// 사라지고 단일 단으로 펴져 9쪽이 된다 (#2299).
#[test]
fn editing_head_of_multicolumn_preserves_page_count() {
    let mut core = load("samples/basic/shortcut.hwp");
    let before = core.page_count();
    assert_eq!(before, 7, "shortcut.hwp 원본 7쪽 (기준값)");

    core.insert_text_native(0, 0, 0, "가").expect("0번 문단 편집");

    assert_eq!(
        core.page_count(),
        before,
        "다단 문서 앞쪽을 편집하자 {}쪽 → {}쪽 — 우측 단-밴드가 소멸해 1단으로 펴졌다 (#2299)",
        before,
        core.page_count()
    );
}

/// 방향 1 보강 — 쪽수만 맞고 단이 사라지는 경우를 막는다.
///
/// 쪽수가 우연히 같아도 우측 단이 비었으면 다단이 깨진 것이다.
#[test]
fn editing_head_of_multicolumn_keeps_right_column_populated() {
    let mut core = load("samples/basic/shortcut.hwp");
    core.insert_text_native(0, 0, 0, "가").expect("0번 문단 편집");

    let tree = core.build_page_render_tree(0).expect("0쪽 렌더 트리");
    let mut cols = Vec::new();
    collect_columns(&tree.root, &mut cols);

    assert!(
        cols.contains(&1),
        "편집 후 0쪽에 우측 단(col=1)이 없다 — 단-밴드 소멸. 실측 col={cols:?}"
    );
}

/// 방향 2 — 넓게 잡으면 깨진다: 줄간격을 키워도 쪽이 안 늘어난다.
///
/// 평범한 단일 단 문단들이 "단 경계"로 오인되어 vpos 가 못 박히면, 아래로 밀려야 할 문단이
/// 제자리에 머물러 페이지 경계를 넘지 못한다.
///
/// (`src/document_core/commands/text_editing.rs` 의 같은 이름 유닛 테스트와 짝이다. 그쪽은
/// 내부 경로를, 이쪽은 공개 경로를 잠근다.)
#[test]
fn increasing_line_spacing_grows_page_count() {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().expect("blank2010");

    let text = "Test paragraph for spacing. ".repeat(20);
    for _ in 0..29 {
        let last = core.document().sections[0].paragraphs.len() - 1;
        core.insert_text_native(0, last, 0, &text).expect("텍스트");
        core.split_paragraph_native(0, last, text.len()).expect("분할");
    }
    let last = core.document().sections[0].paragraphs.len() - 1;
    core.insert_text_native(0, last, 0, &text).expect("마지막 문단 텍스트");

    let before = core.page_count();

    for para_idx in 5..30 {
        core.apply_para_format_native(0, para_idx, r#"{"lineSpacing":360}"#)
            .expect("줄간격 360%");
    }

    assert!(
        core.page_count() > before,
        "줄간격을 160% → 360% 로 올렸는데 쪽수가 그대로다 ({}쪽) — 문단이 단 경계로 오인되어 \
         vpos 가 못 박혔다",
        before
    );
}

/// 렌더 트리에서 Column 노드의 col 인덱스를 모은다.
fn collect_columns(node: &rhwp::renderer::render_tree::RenderNode, out: &mut Vec<u16>) {
    if let rhwp::renderer::render_tree::RenderNodeType::Column(col) = &node.node_type {
        out.push(*col);
    }
    for child in &node.children {
        collect_columns(child, out);
    }
}
