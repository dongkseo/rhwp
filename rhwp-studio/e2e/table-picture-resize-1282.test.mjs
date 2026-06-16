/**
 * E2E 테스트 (Issue #1282): 회전된 표 셀 내부 picture 리사이즈.
 *
 * Rust by-path API 직접 호출만으로는 rhwp-studio 의 실제 드래그 상태(cellPath 보존,
 * 회전 bbox 기준 리사이즈, Undo 기록)까지 검증하지 못한다. 이 테스트는
 * samples/ta-pic-001-r.hwp 의 표 셀 내부 회전 그림을 선택하고, 실제 InputHandler
 * 마우스 드래그 경로를 호출해 셀 높이와 표시 bbox가 안정적으로 갱신되는지 확인한다.
 */
import { runTest, loadHwpFile, assert } from './helpers.mjs';

runTest('회전 표 셀 picture 리사이즈 드래그 안정성 (#1282)', async ({ page }) => {
  await loadHwpFile(page, 'ta-pic-001-r.hwp');

  const result = await page.evaluate(async () => {
    const wasm = window.__wasm;
    const ih = window.__inputHandler;
    const cursor = ih.cursor;
    const nextFrame = () => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    const PX2HWP = 7200 / 96;

    const findTarget = () => {
      for (let pageIndex = 0; pageIndex < wasm.pageCount; pageIndex += 1) {
        const layout = wasm.getPageControlLayout(pageIndex);
        for (const ctrl of layout.controls || []) {
          const path = ctrl.cellPath;
          const first = Array.isArray(path) ? path[0] : null;
          if (
            ctrl.type === 'image'
            && ctrl.paraIdx === 0
            && ctrl.controlIdx === 0
            && first
            && (first.controlIndex ?? first.controlIdx) === 2
            && (first.cellIndex ?? first.cellIdx) === 2
            && (first.cellParaIndex ?? first.cellParaIdx) === 0
          ) {
            return { pageIndex, ctrl };
          }
        }
      }
      return null;
    };

    const target = findTarget();
    if (!target) return { error: 'ta-pic-001-r.hwp 대상 회전 셀 picture를 찾지 못함' };

    const cellPath = target.ctrl.cellPath;
    const getProps = () => wasm.getCellPicturePropertiesByPath(0, 0, cellPath, 0);
    const getCellProps = () => wasm.getCellProperties(0, 0, 2, 2);
    const getBbox = () => {
      const found = findTarget();
      return found?.ctrl ?? null;
    };
    const centerOf = (bbox) => bbox ? { x: bbox.x + bbox.w / 2, y: bbox.y + bbox.h / 2 } : null;
    const requiredCellHeight = (cell, pic) =>
      (pic.vertOffset ?? 0) + pic.height + cell.paddingTop + cell.paddingBottom;

    const select = () => {
      cursor.enterPictureObjectSelectionDirect(
        0,
        0,
        0,
        'image',
        target.ctrl.cellIdx,
        target.ctrl.cellParaIdx,
        undefined,
        target.ctrl.outerTableControlIdx,
        cellPath,
      );
      ih.renderPictureObjectSelection();
    };

    const me = (type, x, y) => {
      const ev = new MouseEvent(type, { button: 0, clientX: x, clientY: y, bubbles: true });
      Object.defineProperty(ev, 'target', { value: ih.container, configurable: true });
      return ev;
    };

    const ensureHandleVisible = async (contentY) => {
      ih.container.scrollTop = Math.max(0, contentY - ih.container.clientHeight / 2);
      await nextFrame();
      select();
      await nextFrame();
    };

    const dragResize = async () => {
      select();
      let handle = (ih.pictureObjectRenderer.handles || []).find((h) => h.dir === 'se');
      if (!handle) return { error: 'se 리사이즈 핸들을 찾지 못함' };
      await ensureHandleVisible(handle.cy);
      handle = (ih.pictureObjectRenderer.handles || []).find((h) => h.dir === 'se');
      if (!handle) return { error: '스크롤 후 se 리사이즈 핸들을 찾지 못함' };

      const sc = ih.container.querySelector('#scroll-content');
      const rect = sc.getBoundingClientRect();
      const x = rect.left + handle.cx;
      const y = rect.top + handle.cy;
      ih.onClickBound(me('mousedown', x, y));
      const stateCellPath = ih.pictureResizeState?.ref?.cellPath ?? null;
      ih.onMouseMoveBound(me('mousemove', x + 52, y + 44));
      await nextFrame();
      const midBbox = getBbox();
      ih.onMouseUpBound(me('mouseup', x + 52, y + 44));
      await nextFrame();
      return { stateCellPath, midBbox };
    };

    const beforeProps = getProps();
    const beforeCell = getCellProps();
    const beforeBbox = getBbox();
    const beforeCenter = centerOf(beforeBbox);

    const drag = await dragResize();
    if (drag.error) return { error: drag.error };

    const afterProps = getProps();
    const afterCell = getCellProps();
    const afterBbox = getBbox();
    const afterCenter = centerOf(afterBbox);
    const midCenter = centerOf(drag.midBbox);

    ih.handleUndo();
    await nextFrame();
    const undoProps = getProps();

    return {
      stateCellPath: drag.stateCellPath,
      beforeProps,
      afterProps,
      undoProps,
      beforeCell,
      afterCell,
      requiredAfter: requiredCellHeight(afterCell, afterProps),
      beforeBbox,
      afterBbox,
      beforeCenter,
      midCenter,
      afterCenter,
      centerJumpAfter: beforeCenter && afterCenter
        ? Math.hypot(afterCenter.x - beforeCenter.x, afterCenter.y - beforeCenter.y)
        : null,
      centerJumpMid: beforeCenter && midCenter
        ? Math.hypot(midCenter.x - beforeCenter.x, midCenter.y - beforeCenter.y)
        : null,
      pxToHwp: PX2HWP,
    };
  });

  assert(!result.error, `검증 실패: ${result.error}`);
  console.log('결과:', JSON.stringify(result, null, 2));

  assert(Array.isArray(result.stateCellPath) && result.stateCellPath.length === 1,
    `드래그 상태 cellPath 보존 실패: ${JSON.stringify(result.stateCellPath)}`);
  assert(result.afterProps.height > result.beforeProps.height,
    `picture height 증가 실패: ${result.beforeProps.height} → ${result.afterProps.height}`);
  assert(result.afterCell.height > result.beforeCell.height,
    `owner cell height 증가 실패: ${result.beforeCell.height} → ${result.afterCell.height}`);
  assert(result.afterCell.height >= result.requiredAfter,
    `owner cell height 부족: cell=${result.afterCell.height}, required=${result.requiredAfter}`);
  assert(result.afterBbox && result.afterBbox.h > result.beforeBbox.h,
    `표시 bbox 높이 증가 실패: ${result.beforeBbox?.h} → ${result.afterBbox?.h}`);
  assert(result.centerJumpMid != null && result.centerJumpMid < 180,
    `라이브 드래그 중 bbox 중심 과도 이동: ${result.centerJumpMid}`);
  assert(result.centerJumpAfter != null && result.centerJumpAfter < 180,
    `확정 후 bbox 중심 과도 이동: ${result.centerJumpAfter}`);
  assert(result.undoProps.height === result.beforeProps.height && result.undoProps.width === result.beforeProps.width,
    `Undo picture size 복구 실패: before=${result.beforeProps.width}x${result.beforeProps.height}, undo=${result.undoProps.width}x${result.undoProps.height}`);

  console.log('✅ #1282 회전 표 셀 picture: 드래그 리사이즈 cellPath/셀높이/bbox 안정성 통과');
});
