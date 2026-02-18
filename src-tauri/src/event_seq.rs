use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{json, Value};

static EVENT_SEQ: AtomicU64 = AtomicU64::new(1);

pub fn next_event_seq() -> u64 {
    EVENT_SEQ.fetch_add(1, Ordering::Relaxed)
}

pub fn add_seq_to_payload(payload: Value, seq: u64) -> Value {
    match payload {
        Value::Object(mut map) => {
            map.insert("seq".to_string(), json!(seq));
            Value::Object(map)
        }
        other => json!({
            "seq": seq,
            "data": other
        }),
    }
}

pub fn payload_with_next_seq(payload: Value) -> Value {
    add_seq_to_payload(payload, next_event_seq())
}
