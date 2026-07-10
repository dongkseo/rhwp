import test from 'node:test';
import assert from 'node:assert/strict';

import { EditorTransport, requestTimeoutFor } from '../transport.js';
import { createEditor } from '../index.js';

test('EditorTransport는 exact origin의 v1 port로 binary를 caller detach 없이 전송한다', async () => {
  let received;
  const contentWindow = {
    postMessage(message, targetOrigin, ports) {
      assert.equal(targetOrigin, 'https://studio.example');
      assert.equal(message.type, 'rhwp-connect');
      assert.deepEqual(message.capabilities, ['transferable-array-buffer']);
      const server = ports[0];
      server.onmessage = ({ data }) => {
        received = data;
        server.postMessage({
          type: 'rhwp-response', version: 1, sessionId: data.sessionId,
          id: data.id, result: { pageCount: 1 },
        });
      };
      server.start();
      server.postMessage({
        type: 'rhwp-connected', version: 1, sessionId: message.sessionId,
        capabilities: ['transferable-array-buffer'],
      });
    },
  };
  const fakeWindow = { addEventListener() {}, removeEventListener() {} };
  const transport = new EditorTransport(
    { contentWindow },
    'https://studio.example/app',
    { window: fakeWindow, requestTimeoutMs: 100, handshakeTimeoutMs: 100 },
  );

  await transport.connect();
  const callerBytes = new Uint8Array([1, 2, 3]);
  assert.deepEqual(
    await transport.request('loadFile', { data: callerBytes, fileName: 'a.hwp' }),
    { pageCount: 1 },
  );
  assert.deepEqual([...callerBytes], [1, 2, 3]);
  assert.deepEqual([...received.params.data], [1, 2, 3]);
  transport.destroy();
});

test('EditorTransport는 50 MiB v1 loadFile binary를 number array 없이 전송한다', async () => {
  const size = 50 * 1024 * 1024;
  const callerBuffer = new ArrayBuffer(size);
  const callerBytes = new Uint8Array(callerBuffer);
  callerBytes[0] = 0x11;
  callerBytes[Math.floor(size / 2)] = 0x7f;
  callerBytes[size - 1] = 0xee;

  let received;
  let server;
  const contentWindow = {
    postMessage(message, _targetOrigin, ports) {
      server = ports[0];
      server.onmessage = ({ data }) => {
        received = data.params.data;
        server.postMessage({
          type: 'rhwp-response', version: 1, sessionId: data.sessionId,
          id: data.id, result: { pageCount: 1 },
        });
      };
      server.start();
      server.postMessage({
        type: 'rhwp-connected', version: 1, sessionId: message.sessionId,
        capabilities: ['transferable-array-buffer'],
      });
    },
  };
  const transport = new EditorTransport(
    { contentWindow },
    'https://studio.example/app',
    { window: { addEventListener() {}, removeEventListener() {} }, requestTimeoutMs: 1_000 },
  );

  try {
    await transport.connect();
    await transport.request('loadFile', { data: callerBytes, fileName: 'large.hwp' });

    assert.ok(ArrayBuffer.isView(received));
    assert.equal(Array.isArray(received), false);
    assert.equal(received.byteLength, size);
    assert.equal(received[0], 0x11);
    assert.equal(received[Math.floor(size / 2)], 0x7f);
    assert.equal(received[size - 1], 0xee);
    assert.equal(callerBytes.buffer, callerBuffer);
    assert.equal(callerBuffer.byteLength, size);
    assert.equal(callerBytes[0], 0x11);
    assert.equal(callerBytes[Math.floor(size / 2)], 0x7f);
    assert.equal(callerBytes[size - 1], 0xee);
  } finally {
    transport.destroy();
    server?.close();
  }
});

test('EditorTransport는 legacy fallback에서도 source/origin이 맞는 응답만 받는다', async () => {
  let listener;
  const fakeWindow = {
    addEventListener(_type, callback) { listener = callback; },
    removeEventListener() { listener = undefined; },
  };
  const contentWindow = {
    postMessage(message) {
      if (message.type === 'rhwp-connect') return;
      queueMicrotask(() => {
        listener({ source: {}, origin: 'https://studio.example', data: {
          type: 'rhwp-response', id: message.id, result: 99,
        } });
        listener({ source: contentWindow, origin: 'https://studio.example', data: {
          type: 'rhwp-response', id: message.id, result: 3,
        } });
      });
    },
  };
  const transport = new EditorTransport(
    { contentWindow },
    'https://studio.example/app',
    { window: fakeWindow, requestTimeoutMs: 100, handshakeTimeoutMs: 5 },
  );

  await transport.connect();
  assert.equal(await transport.request('pageCount'), 3);
  transport.destroy();
});

test('EditorTransport.destroy는 pending request를 거부한다', async () => {
  const contentWindow = {
    postMessage(message, _targetOrigin, ports) {
      const server = ports[0];
      server.start();
      server.postMessage({
        type: 'rhwp-connected', version: 1, sessionId: message.sessionId,
        capabilities: ['transferable-array-buffer'],
      });
    },
  };
  const fakeWindow = { addEventListener() {}, removeEventListener() {} };
  const transport = new EditorTransport(
    { contentWindow },
    'https://studio.example/app',
    { window: fakeWindow, requestTimeoutMs: 100, handshakeTimeoutMs: 100 },
  );

  await transport.connect();
  const pending = transport.request('pageCount');
  transport.destroy();
  await assert.rejects(pending, /Editor destroyed/);
});

test('EditorTransport는 일반 요청 10초와 load/export 60초 기본 timeout을 구분한다', () => {
  assert.equal(requestTimeoutFor('pageCount'), 10_000);
  assert.equal(requestTimeoutFor('ready'), 10_000);
  assert.equal(requestTimeoutFor('loadFile'), 60_000);
  assert.equal(requestTimeoutFor('exportHwp'), 60_000);
  assert.equal(requestTimeoutFor('exportHwpx'), 60_000);
});

test('EditorTransport는 구조화된 version 협상 오류를 legacy fallback 없이 전달한다', async () => {
  let listenerCount = 0;
  const fakeWindow = {
    addEventListener() { listenerCount += 1; },
    removeEventListener() { listenerCount -= 1; },
  };
  const contentWindow = {
    postMessage(message, _targetOrigin, ports) {
      const server = ports[0];
      server.start();
      server.postMessage({
        type: 'rhwp-connect-error', version: 1, sessionId: message.sessionId,
        error: {
          code: 'UNSUPPORTED_VERSION',
          message: 'Unsupported embed protocol version: 2',
          supportedVersions: [1],
        },
      });
    },
  };
  const transport = new EditorTransport(
    { contentWindow },
    'https://studio.example/app',
    { window: fakeWindow, handshakeTimeoutMs: 100 },
  );

  await assert.rejects(
    transport.connect(),
    (error) => error.code === 'UNSUPPORTED_VERSION' && error.supportedVersions?.[0] === 1,
  );
  assert.equal(listenerCount, 0);
  transport.destroy();
});

test('EditorTransport.destroy는 진행 중인 handshake도 한 번 거부한다', async () => {
  const contentWindow = { postMessage() {} };
  const fakeWindow = { addEventListener() {}, removeEventListener() {} };
  const transport = new EditorTransport(
    { contentWindow },
    'https://studio.example/app',
    { window: fakeWindow, handshakeTimeoutMs: 100 },
  );

  const connecting = transport.connect();
  transport.destroy();
  await assert.rejects(connecting, /Editor destroyed/);
});

test('EditorTransport는 malformed v1 response를 완료 응답으로 처리하지 않는다', async () => {
  let server;
  const contentWindow = {
    postMessage(message, _targetOrigin, ports) {
      server = ports[0];
      server.onmessage = ({ data }) => {
        server.postMessage({
          type: 'rhwp-response', version: 1, sessionId: data.sessionId,
          id: data.id, result: 1, error: { code: 'RPC_ERROR', message: 'both' },
        });
        queueMicrotask(() => server.postMessage({
          type: 'rhwp-response', version: 1, sessionId: data.sessionId,
          id: data.id, result: 2,
        }));
      };
      server.start();
      server.postMessage({
        type: 'rhwp-connected', version: 1, sessionId: message.sessionId,
        capabilities: ['transferable-array-buffer'],
      });
    },
  };
  const fakeWindow = { addEventListener() {}, removeEventListener() {} };
  const transport = new EditorTransport(
    { contentWindow }, 'https://studio.example/app', { window: fakeWindow },
  );

  await transport.connect();
  assert.equal(await transport.request('pageCount'), 2);
  transport.destroy();
});

test('createEditor 연결 실패는 생성한 iframe과 transport를 정리한다', async () => {
  const originalDocument = globalThis.document;
  const originalWindow = globalThis.window;
  let removed = false;
  const contentWindow = {
    postMessage(message, _targetOrigin, ports) {
      const server = ports[0];
      server.start();
      server.postMessage({
        type: 'rhwp-connect-error', version: 1, sessionId: message.sessionId,
        error: { code: 'UNSUPPORTED_VERSION', message: 'version mismatch' },
      });
    },
  };
  const iframe = {
    contentWindow,
    style: {},
    addEventListener(_type, listener) { queueMicrotask(listener); },
    remove() { removed = true; },
  };
  const container = { appendChild() {} };
  globalThis.window = { addEventListener() {}, removeEventListener() {} };
  globalThis.document = { createElement: () => iframe };

  try {
    await assert.rejects(
      createEditor(container, { studioUrl: 'https://studio.example/app' }),
      /version mismatch/,
    );
    assert.equal(removed, true);
  } finally {
    globalThis.document = originalDocument;
    globalThis.window = originalWindow;
  }
});

test('createEditor는 HTTP(S)가 아닌 studioUrl 실패 시 iframe을 남기지 않는다', async () => {
  const originalDocument = globalThis.document;
  const originalWindow = globalThis.window;
  let removed = false;
  const iframe = {
    contentWindow: { postMessage() { throw new Error('connect attempted'); } },
    style: {},
    addEventListener(_type, listener) { queueMicrotask(listener); },
    remove() { removed = true; },
  };
  globalThis.window = { addEventListener() {}, removeEventListener() {} };
  globalThis.document = { createElement: () => iframe };

  try {
    await assert.rejects(
      createEditor({ appendChild() {} }, { studioUrl: 'file:///tmp/rhwp-studio/' }),
      /HTTP\(S\)/,
    );
    assert.equal(removed, true);
  } finally {
    globalThis.document = originalDocument;
    globalThis.window = originalWindow;
  }
});

test('EditorTransport는 동기 postMessage 실패 시 pending과 timer를 정리한다', async () => {
  const contentWindow = {
    postMessage(message, _targetOrigin, ports) {
      const server = ports[0];
      server.start();
      server.postMessage({
        type: 'rhwp-connected', version: 1, sessionId: message.sessionId,
        capabilities: ['transferable-array-buffer'],
      });
    },
  };
  const transport = new EditorTransport(
    { contentWindow },
    'https://studio.example/app',
    { window: { addEventListener() {}, removeEventListener() {} }, requestTimeoutMs: 10_000 },
  );

  await transport.connect();
  try {
    await assert.rejects(
      transport.request('pageCount', { uncloneable() {} }),
      (error) => error?.name === 'DataCloneError',
    );
    assert.equal(transport._pending.size, 0);
  } finally {
    transport.destroy();
  }
});
