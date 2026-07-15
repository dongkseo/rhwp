/**
 * 최근 열람 문서 저장소.
 *
 * 파일 메뉴 "최근 문서" 목록의 영속 저장을 담당한다. 브라우저·열기 경로에 무관하게
 * 모든 열기에서 등록·재열기가 가능하도록 **문서 바이트**를 IndexedDB에 저장한다.
 * File System Access 핸들이 있으면 함께 저장하여 재열기 시 라이브 파일을 우선 사용하고,
 * 없거나 접근 실패 시 저장된 바이트로 연다.
 *
 * 자동 백업(`rhwpStudioAutosave`)·비교 이력(`rhwpStudioDocHistory`)과 섞지 않기 위해
 * 별도 IndexedDB(`rhwpStudioRecent`)를 사용한다. IndexedDB를 쓸 수 없는 테스트/제한
 * 환경에서는 메모리 저장소로 폴백한다.
 */

import type { FileSystemFileHandleLike } from '@/command/file-system-access';

const DB_NAME = 'rhwpStudioRecent';
const DB_VER = 1;
const STORE = 'recent';
const MAX_RECENT = 8;

export interface RecentDoc {
  /** 고유 ID (crypto.randomUUID) */
  id: string;
  /** 파일명 (경로 아님 — 브라우저 제약) */
  fileName: string;
  /** 원본 형식 ('hwp' | 'hwpx' | 'hml' 등) */
  sourceFormat: string;
  /** 마지막으로 연 시각 (epoch ms) */
  openedAt: number;
  /** 재열기용 문서 바이트 (열기 시점 스냅샷) */
  bytes: Uint8Array;
  /** 재열기 시 라이브 파일 우선 접근용 핸들 (있을 때만) */
  handle?: FileSystemFileHandleLike;
}

/** addRecentDoc 입력 (id/openedAt는 내부 생성) */
export interface RecentDocInput {
  fileName: string;
  sourceFormat: string;
  bytes: Uint8Array;
  handle?: FileSystemFileHandleLike | null;
}

/** IndexedDB 저장 행 — 바이트는 ArrayBuffer로 보관 */
type RecentRow = Omit<RecentDoc, 'bytes'> & { bytes: ArrayBuffer };

const memory = new Map<string, RecentDoc>();

function idbAvailable(): boolean {
  return typeof indexedDB !== 'undefined';
}

function createRecentId(): string {
  return globalThis.crypto?.randomUUID?.() ?? `recent_${Date.now()}_${Math.random().toString(36).slice(2, 9)}`;
}

function bytesToArrayBuffer(bytes: Uint8Array): ArrayBuffer {
  const copy = new Uint8Array(bytes.byteLength);
  copy.set(bytes);
  return copy.buffer as ArrayBuffer;
}

function rowToDoc(row: RecentRow): RecentDoc {
  return { ...row, bytes: new Uint8Array(row.bytes ?? new ArrayBuffer(0)) };
}

function docToRow(doc: RecentDoc): RecentRow {
  return { ...doc, bytes: bytesToArrayBuffer(doc.bytes) };
}

function openDb(): Promise<IDBDatabase | null> {
  if (!idbAvailable()) return Promise.resolve(null);
  return new Promise((resolve) => {
    const req = indexedDB.open(DB_NAME, DB_VER);
    req.onerror = () => resolve(null);
    req.onsuccess = () => resolve(req.result);
    req.onupgradeneeded = () => {
      const db = req.result;
      if (!db.objectStoreNames.contains(STORE)) {
        db.createObjectStore(STORE, { keyPath: 'id' });
      }
    };
  });
}

async function withDb<T>(fn: (db: IDBDatabase) => Promise<T>, fallback: () => Promise<T>): Promise<T> {
  const db = await openDb();
  if (!db) return fallback();
  try {
    return await fn(db);
  } finally {
    db.close();
  }
}

function getAllRows(db: IDBDatabase): Promise<RecentRow[]> {
  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE, 'readonly');
    const req = tx.objectStore(STORE).getAll();
    req.onsuccess = () => resolve((req.result as RecentRow[]) ?? []);
    req.onerror = () => reject(req.error);
  });
}

function putRow(db: IDBDatabase, row: RecentRow): Promise<void> {
  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE, 'readwrite');
    tx.objectStore(STORE).put(row);
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
  });
}

function deleteRow(db: IDBDatabase, id: string): Promise<void> {
  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE, 'readwrite');
    tx.objectStore(STORE).delete(id);
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
  });
}

/** 핸들 포함 저장을 시도하되, 핸들이 clone 불가(DataCloneError)면 핸들 없이 저장한다. */
async function putRowResilient(db: IDBDatabase, row: RecentRow): Promise<void> {
  try {
    await putRow(db, row);
  } catch (err) {
    if (row.handle) {
      const { handle, ...rest } = row;
      void handle;
      await putRow(db, rest as RecentRow);
      return;
    }
    throw err;
  }
}

/** 동일 파일 판정: 두 핸들이 모두 있으면 isSameEntry, 아니면 파일명 비교. */
async function isSameFile(a: RecentDocInput, existing: RecentDoc): Promise<boolean> {
  const ha = a.handle;
  const hb = existing.handle;
  if (ha && hb && typeof ha.isSameEntry === 'function') {
    try {
      return await ha.isSameEntry(hb);
    } catch {
      // fall through to name compare
    }
  }
  return a.fileName === existing.fileName;
}

/** 최신순(openedAt 내림차순)으로 정렬해 상한까지 자른다. */
function sortAndTrim(rows: RecentDoc[]): RecentDoc[] {
  return rows.sort((a, b) => b.openedAt - a.openedAt).slice(0, MAX_RECENT);
}

/**
 * 최근 문서를 추가한다. 동일 파일이 이미 있으면 제거 후 맨 앞에 다시 넣고,
 * 최대 {@link MAX_RECENT}개를 유지한다. 모든 열기 경로에서 호출된다(핸들 유무 무관).
 */
export async function addRecentDoc(input: RecentDocInput): Promise<void> {
  const entry: RecentDoc = {
    id: createRecentId(),
    fileName: input.fileName,
    sourceFormat: input.sourceFormat,
    openedAt: Date.now(),
    bytes: new Uint8Array(input.bytes),
    ...(input.handle ? { handle: input.handle } : {}),
  };

  await withDb(
    async (db) => {
      const rows = (await getAllRows(db)).map(rowToDoc);
      for (const row of rows) {
        if (await isSameFile(input, row)) await deleteRow(db, row.id);
      }
      await putRowResilient(db, docToRow(entry));
      const after = sortAndTrim((await getAllRows(db)).map(rowToDoc));
      const keep = new Set(after.map((r) => r.id));
      for (const row of await getAllRows(db)) {
        if (!keep.has(row.id)) await deleteRow(db, row.id);
      }
    },
    async () => {
      for (const [id, row] of memory) {
        if (await isSameFile(input, row)) memory.delete(id);
      }
      memory.set(entry.id, entry);
      const keep = new Set(sortAndTrim([...memory.values()]).map((r) => r.id));
      for (const id of [...memory.keys()]) {
        if (!keep.has(id)) memory.delete(id);
      }
    },
  );
}

/** 최근 문서 목록(최신순). */
export async function listRecentDocs(): Promise<RecentDoc[]> {
  return withDb(
    async (db) => sortAndTrim((await getAllRows(db)).map(rowToDoc)),
    async () => sortAndTrim([...memory.values()].map((d) => ({ ...d, bytes: new Uint8Array(d.bytes) }))),
  );
}

/** 특정 최근 문서를 제거한다. */
export async function removeRecentDoc(id: string): Promise<void> {
  memory.delete(id);
  await withDb(
    async (db) => deleteRow(db, id),
    async () => {},
  );
}

/** 최근 문서 목록 전체 삭제. */
export async function clearRecentDocs(): Promise<void> {
  memory.clear();
  await withDb(
    async (db) =>
      new Promise<void>((resolve, reject) => {
        const tx = db.transaction(STORE, 'readwrite');
        tx.objectStore(STORE).clear();
        tx.oncomplete = () => resolve();
        tx.onerror = () => reject(tx.error);
      }),
    async () => {},
  );
}
