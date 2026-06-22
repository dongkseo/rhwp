// 다운로드 관찰자 (Chrome)
// - .hwp/.hwpx 다운로드 감지 → 뷰어로 열기
// - 사용자 설정(autoOpen)에 따라 동작
//
// #198 (chrome-fd-001): HWP 가 아닌 일반 파일 다운로드에는 suggest() 를 호출하지 않아
//                       Chrome 의 마지막 저장 위치 기억 동작을 보존한다.
// #207: 판정 로직은 rhwp-shared/sw/download-interceptor-common.js 와 공유.
// #1131: 로컬 file:// HWP 는 이미 디스크에 있으므로 자체 뷰어로 열 때 중복 다운로드를
//        억제한다 (cancel + erase, best-effort). 원격(http) 파일은 기존 동작 유지.
// #1471: onDeterminingFilename 리스너 등록 자체가 다른 확장의 download({filename})
//        경로 결정을 무효화하므로 filename 결정 단계에서 완전히 빠진다.

import { openViewer } from './viewer-launcher.js';
import { shouldInterceptDownload } from './download-interceptor-common.js';

const handled = new Set();

/** 다운로드 항목이 로컬 file:// 인지 판별. */
function isLocalFileDownload(item) {
  return typeof item?.url === 'string' && item.url.startsWith('file:');
}

function isTerminalDelta(delta) {
  return delta?.state?.current === 'complete' || delta?.state?.current === 'interrupted' || Boolean(delta?.error);
}

function shouldRecheckDownload(delta) {
  return Boolean(delta?.filename?.current || delta?.finalUrl?.current || delta?.state?.current === 'complete');
}

/**
 * 다운로드 관찰자를 설정한다.
 *
 * - 로컬 file:// HWP: 뷰어를 열고 다운로드는 cancel + erase 로 억제 (#1131)
 * - 원격 HWP/HWPX 다운로드: 뷰어 트리거
 * - 일반 파일: filename 결정 단계에 참여하지 않음 (#1471)
 */
export function setupDownloadInterceptor() {
  chrome.downloads.onCreated.addListener((item) => {
    processDownloadItem(item);
  });

  chrome.downloads.onChanged.addListener(async (delta) => {
    if (!handled.has(delta.id) && shouldRecheckDownload(delta)) {
      try {
        const [item] = await chrome.downloads.search({ id: delta.id });
        processDownloadItem(item);
      } catch (err) {
        console.error('[rhwp] 다운로드 항목 재조회 오류:', err);
      }
    }

    if (handled.has(delta.id) && isTerminalDelta(delta)) {
      setTimeout(() => handled.delete(delta.id), 30000);
    }
  });
}

function processDownloadItem(item) {
  if (!item || handled.has(item.id)) return;
  if (!shouldInterceptDownload(item)) return;

  handled.add(item.id);
  handleHwpDownload(item);

  if (isLocalFileDownload(item)) {
    suppressLocalDownload(item);
  }
}

/**
 * 로컬 file:// 다운로드를 취소하고 다운로드 목록에서 제거한다 (#1131, best-effort).
 *
 * 로컬 복사는 거의 즉시 완료되어 cancel 이 늦을 수 있으나, erase 로 항목을 정리한다.
 * 실패해도 뷰어 동작에는 영향이 없으므로 예외는 무시한다.
 */
async function suppressLocalDownload(item) {
  try {
    await chrome.downloads.cancel(item.id);
  } catch {
    // 이미 완료/취소됨 — 무시하고 erase 로 진행
  }
  try {
    await chrome.downloads.erase({ id: item.id });
  } catch {
    // 항목 제거 실패는 치명적이지 않음 — 무시
  }
}

async function handleHwpDownload(item) {
  try {
    const settings = await chrome.storage.sync.get({ autoOpen: true });
    if (!settings.autoOpen) return;

    // 대용량 파일 경고 (50MB 초과)
    if (item.fileSize > 50 * 1024 * 1024) {
      console.warn(`[rhwp] 대용량 파일: ${item.filename} (${(item.fileSize / 1024 / 1024).toFixed(1)}MB)`);
    }

    openViewer({
      url: item.url,
      filename: item.filename,
    });
  } catch (err) {
    console.error('[rhwp] 다운로드 인터셉터 오류:', err);
  }
}
