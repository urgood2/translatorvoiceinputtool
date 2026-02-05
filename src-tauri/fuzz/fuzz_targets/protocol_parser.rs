//! Fuzz test for JSON-RPC protocol parsing.
//!
//! This fuzz target tests that the protocol parser never panics on malformed input,
//! ensuring memory safety and robustness against untrusted sidecar messages.

#![no_main]

use libfuzzer_sys::fuzz_target;
use translator_voice_input_tool_lib::ipc::types::{
    IncomingMessage, Notification, RequestId, Response,
};

fuzz_target!(|data: &[u8]| {
    // Only test valid UTF-8 strings since JSON-RPC uses text
    if let Ok(s) = std::str::from_utf8(data) {
        // Test parsing as IncomingMessage (the main entry point)
        let _ = serde_json::from_str::<IncomingMessage>(s);

        // Test parsing individual types
        let _ = serde_json::from_str::<Response>(s);
        let _ = serde_json::from_str::<Notification>(s);
        let _ = serde_json::from_str::<RequestId>(s);

        // Test with line length limits (matching MAX_LINE_LENGTH = 1 MiB)
        if s.len() <= 1024 * 1024 {
            let _ = serde_json::from_str::<IncomingMessage>(s);
        }

        // Test common JSON-RPC patterns
        // Empty object
        if s == "{}" {
            let _ = serde_json::from_str::<Response>(s);
        }

        // Nested structures
        if s.contains('[') || s.contains('{') {
            let _ = serde_json::from_str::<serde_json::Value>(s);
        }
    }

    // Also test raw bytes to ensure no buffer overflows in serde_json
    // serde_json should safely handle invalid UTF-8
    let _ = serde_json::from_slice::<IncomingMessage>(data);
    let _ = serde_json::from_slice::<Response>(data);
    let _ = serde_json::from_slice::<Notification>(data);
});
