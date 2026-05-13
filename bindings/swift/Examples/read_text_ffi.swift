#!/usr/bin/env swift
/// rhwp_read_text FFI 직접 호출 예제.
///
/// 사용법 (레포 루트에서):
///   cargo build --manifest-path bindings/Native/Cargo.toml --release
///   swift bindings/swift/Examples/read_text_ffi.swift
///
/// rhwp_read_text(path, page) → JSON 문자열 반환.
/// page = -1 이면 전체 페이지, 0 이상이면 해당 페이지만.
///
/// 반환 JSON 형식:
///   {"ok":true,"pageCount":27,"pages":[{"index":0,"text":"..."},{"index":1,"text":"..."},...]}

import Foundation

typealias ReadTextFn = @convention(c) (UnsafePointer<CChar>, Int32) -> UnsafeMutablePointer<CChar>?
typealias StringFreeFn = @convention(c) (UnsafeMutablePointer<CChar>) -> Void

// 1. 네이티브 라이브러리 로드
let scriptDir = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
let repoRoot = scriptDir
    .deletingLastPathComponent()  // Examples/
    .deletingLastPathComponent()  // swift/
    .deletingLastPathComponent()  // bindings/
let libPath = repoRoot
    .appendingPathComponent("bindings/Native/target/release/librhwp_native_ffi.dylib")
    .path

guard let handle = dlopen(libPath, RTLD_NOW) else {
    let err = String(cString: dlerror())
    print("ERROR: dylib 로드 실패 — \(err)")
    print("       cargo build --manifest-path bindings/Native/Cargo.toml --release 를 먼저 실행하세요.")
    exit(1)
}

guard let readSym = dlsym(handle, "rhwp_read_text"),
      let freeSym = dlsym(handle, "rhwp_string_free") else {
    print("ERROR: FFI 심볼을 찾을 수 없습니다.")
    exit(1)
}

let readText = unsafeBitCast(readSym, to: ReadTextFn.self)
let freeStr = unsafeBitCast(freeSym, to: StringFreeFn.self)

// 2. HWP 파일 경로 (인자 또는 기본값)
let samplePath: String
if CommandLine.arguments.count > 1 {
    samplePath = CommandLine.arguments[1]
} else {
    samplePath = repoRoot.appendingPathComponent("samples/KTX.hwp").path
}

// 3. 페이지 번호 (-1 = 전체, 0~ = 개별 페이지)
let page: Int32
if CommandLine.arguments.count > 2, let p = Int32(CommandLine.arguments[2]) {
    page = p
} else {
    page = -1  // 전체 페이지
}

print("파일: \(samplePath)")
print("페이지: \(page == -1 ? "전체" : String(page))")
print()

// 4. FFI 호출
guard let resultPtr = samplePath.withCString({ readText($0, page) }) else {
    print("ERROR: rhwp_read_text 반환값 null — 파일 경로를 확인하세요.")
    exit(1)
}

let jsonStr = String(cString: resultPtr)

// 5. JSON 파싱 + 출력
if let data = jsonStr.data(using: .utf8),
   let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
   let ok = json["ok"] as? Bool, ok,
   let pageCount = json["pageCount"] as? Int,
   let pages = json["pages"] as? [[String: Any]] {

    print("=== 텍스트 읽기 성공 (pageCount=\(pageCount), 반환 페이지=\(pages.count)) ===")
    print()
    for pageObj in pages.prefix(5) {
        let idx = pageObj["index"] as? Int ?? -1
        let text = pageObj["text"] as? String ?? ""
        let lines = text.components(separatedBy: "\n").filter { !$0.isEmpty }
        print("--- 페이지 \(idx + 1) (\(lines.count)줄) ---")
        for line in lines.prefix(5) {
            print("  \(line)")
        }
        if lines.count > 5 { print("  ... (\(lines.count - 5)줄 생략)") }
        print()
    }
    if pages.count > 5 { print("... (\(pages.count - 5) 페이지 생략)") }
} else {
    print("JSON 파싱 실패 또는 에러 응답:")
    print(String(jsonStr.prefix(500)))
}

// 6. 메모리 해제
freeStr(resultPtr)
