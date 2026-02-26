use std::fs;
use std::path::PathBuf;

use serde_json::Value;
use translator_voice_input_tool_lib::ipc::types::AsrInitializeParams;

#[test]
fn asr_initialize_params_serializes_optional_language() {
    let params = AsrInitializeParams {
        model_id: Some("openai/whisper-small".to_string()),
        device_pref: Some("auto".to_string()),
        language: Some("en".to_string()),
    };

    let value = serde_json::to_value(params).expect("params should serialize");
    assert_eq!(value.get("language").and_then(Value::as_str), Some("en"));
}

#[test]
fn asr_initialize_params_omits_language_when_not_set() {
    let params = AsrInitializeParams {
        model_id: Some("nvidia/parakeet-tdt-0.6b-v3".to_string()),
        device_pref: Some("cpu".to_string()),
        language: None,
    };

    let value = serde_json::to_value(params).expect("params should serialize");
    assert!(value.get("language").is_none());
}

#[test]
fn sidecar_contract_declares_asr_initialize_language_param() {
    let contract_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri should have project parent")
        .join("shared/contracts/sidecar.rpc.v1.json");
    let raw = fs::read_to_string(contract_path).expect("contract should be readable");
    let contract: Value = serde_json::from_str(&raw).expect("contract should parse");
    let methods = contract
        .get("methods")
        .and_then(Value::as_array)
        .expect("contract methods should be array");

    let asr_initialize = methods
        .iter()
        .find(|method| method.get("name").and_then(Value::as_str) == Some("asr.initialize"))
        .expect("asr.initialize method should be present");

    let language = asr_initialize
        .get("params_schema")
        .and_then(|schema| schema.get("properties"))
        .and_then(|props| props.get("language"))
        .expect("asr.initialize params should include language");
    let language_type = language
        .get("type")
        .and_then(Value::as_array)
        .expect("language type should be an array");
    let language_type_values: Vec<&str> = language_type.iter().filter_map(Value::as_str).collect();
    assert!(language_type_values.contains(&"string"));
    assert!(language_type_values.contains(&"null"));
}
