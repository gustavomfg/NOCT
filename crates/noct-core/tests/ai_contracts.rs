//! Model descriptors and workspace bindings, checked against `shared/ai`. The expected
//! texts were produced by the TypeScript Zod schemas followed by `JSON.stringify`.

use noct_core::ai::ProviderSource;
use noct_core::ai::bindings::WorkspaceModelBindings;
use noct_core::ai::model::{ModelDescriptor, ModelReference};
use serde_json::{Value, json};

fn descriptor_from(value: Value) -> Result<ModelDescriptor, String> {
    let parsed: ModelDescriptor =
        serde_json::from_value(value).map_err(|error| error.to_string())?;
    parsed.validated().map_err(|error| error.to_string())
}

fn sample() -> Value {
    json!({
        "displayName": "  Modelo Alfa  ",
        "providerId": " provider-1 ",
        "modelId": "model-1",
        "source": "local",
        "capabilities": ["chat", "streaming"],
        "contextWindow": 128000,
        "availability": "available",
        "pricing": {
            "currency": "USD",
            "inputPerMillionTokens": 2,
            "outputPerMillionTokens": 0.000001,
            "effectiveAt": " 2026-01-01 "
        },
        "family": "alpha"
    })
}

#[test]
fn a_descriptor_is_trimmed_and_written_exactly_as_zod_and_json_stringify_write_it() {
    let descriptor = descriptor_from(sample()).expect("valid descriptor");

    assert_eq!(
        serde_json::to_string(&descriptor).unwrap(),
        r#"{"providerId":"provider-1","modelId":"model-1","displayName":"Modelo Alfa","family":"alpha","source":"local","capabilities":["chat","streaming"],"contextWindow":128000,"pricing":{"currency":"USD","inputPerMillionTokens":2,"outputPerMillionTokens":0.000001,"effectiveAt":"2026-01-01"},"availability":"available"}"#,
    );
    assert_eq!(descriptor.source, ProviderSource::Local);
}

#[test]
fn unknown_keys_and_explicit_nulls_are_rejected_as_strict_optional_fields() {
    let mut unknown = sample();
    unknown["extra"] = json!(true);
    assert!(descriptor_from(unknown).is_err());

    let mut nullable = sample();
    nullable["family"] = Value::Null;
    assert!(descriptor_from(nullable).is_err());

    let mut null_pricing = sample();
    null_pricing["pricing"] = Value::Null;
    assert!(descriptor_from(null_pricing).is_err());
}

#[test]
fn text_is_bounded_after_trimming_and_counted_in_utf16_units() {
    let mut blank = sample();
    blank["displayName"] = json!("   ");
    assert!(descriptor_from(blank).is_err());

    let mut at_limit = sample();
    at_limit["displayName"] = json!("😀".repeat(250));
    assert!(
        descriptor_from(at_limit).is_ok(),
        "250 emoji are 500 UTF-16 units"
    );

    let mut over_limit = sample();
    over_limit["displayName"] = json!("😀".repeat(251));
    assert!(descriptor_from(over_limit).is_err());
}

#[test]
fn counts_must_be_positive_and_capabilities_must_be_unique() {
    let mut zero_window = sample();
    zero_window["contextWindow"] = json!(0);
    assert!(descriptor_from(zero_window).is_err());

    let mut fractional = sample();
    fractional["contextWindow"] = json!(1.5);
    assert!(descriptor_from(fractional).is_err());

    let mut duplicated = sample();
    duplicated["capabilities"] = json!(["chat", "chat"]);
    let error = descriptor_from(duplicated).unwrap_err();
    assert!(error.contains("Capacidades duplicadas"), "{error}");
}

#[test]
fn pricing_needs_one_price_and_non_negative_values() {
    let mut no_price = sample();
    no_price["pricing"] = json!({ "currency": "USD", "effectiveAt": "2026-01-01" });
    let error = descriptor_from(no_price).unwrap_err();
    assert!(error.contains("ao menos um preço"), "{error}");

    let mut negative = sample();
    negative["pricing"]["inputPerMillionTokens"] = json!(-1);
    assert!(descriptor_from(negative).is_err());

    let mut other_currency = sample();
    other_currency["pricing"]["currency"] = json!("BRL");
    assert!(descriptor_from(other_currency).is_err());
}

#[test]
fn unknown_capabilities_and_availability_values_are_rejected() {
    let mut capability = sample();
    capability["capabilities"] = json!(["telepathy"]);
    assert!(descriptor_from(capability).is_err());

    let mut availability = sample();
    availability["availability"] = json!("sleeping");
    assert!(descriptor_from(availability).is_err());
}

#[test]
fn workspace_bindings_follow_their_schema_order_and_trim_references() {
    let parsed: WorkspaceModelBindings = serde_json::from_value(json!({
        "workspaceId": " /w ",
        "embeddingBinding": { "providerId": "p", "modelId": "m" },
        "defaultBinding": { "providerId": "p2", "modelId": "m2" },
        "remoteEmbeddingAllowed": false
    }))
    .unwrap();
    let bindings = parsed.validated().unwrap();

    assert_eq!(
        serde_json::to_string(&bindings).unwrap(),
        r#"{"workspaceId":"/w","defaultBinding":{"providerId":"p2","modelId":"m2"},"embeddingBinding":{"providerId":"p","modelId":"m"},"remoteEmbeddingAllowed":false}"#,
    );
}

#[test]
fn a_binding_reference_must_name_a_provider_and_a_model() {
    let parsed: Result<WorkspaceModelBindings, _> = serde_json::from_value(json!({
        "workspaceId": "/w",
        "defaultBinding": { "providerId": "p" }
    }));
    assert!(parsed.is_err());

    let reference = ModelReference {
        provider_id: "  ".to_string(),
        model_id: "m".to_string(),
    };
    assert!(reference.validated().is_err());
}

#[test]
fn identifiers_follow_zod_uuid_rules() {
    use noct_core::validation::uuid_text;

    for valid in [
        "6f9619ff-8b86-4d11-b42d-00c04fc964ff",
        "6F9619FF-8B86-4D11-B42D-00C04FC964FF",
        "00000000-0000-0000-0000-000000000000",
        "ffffffff-ffff-ffff-ffff-ffffffffffff",
    ] {
        assert!(uuid_text("id", valid).is_ok(), "{valid}");
    }
    for invalid in [
        "6f9619ff-8b86-0d11-b42d-00c04fc964ff",
        "6f9619ff-8b86-9d11-b42d-00c04fc964ff",
        "6f9619ff-8b86-4d11-c42d-00c04fc964ff",
        "FFFFFFFF-FFFF-FFFF-FFFF-FFFFFFFFFFFF",
        "6f9619ff8b864d11b42d00c04fc964ff",
        "{6f9619ff-8b86-4d11-b42d-00c04fc964ff}",
        "",
    ] {
        assert!(uuid_text("id", invalid).is_err(), "{invalid}");
    }
}

#[test]
fn provider_configuration_input_is_trimmed_and_bounded() {
    use noct_core::ai::provider_configuration::{
        ProviderConfigurationInput, ProviderConfigurationType,
    };

    let parsed: ProviderConfigurationInput = serde_json::from_value(json!({
        "providerType": "chat-completions",
        "displayName": "  Local  ",
        "source": "local",
        "baseUrl": " http://127.0.0.1:11434 ",
        "enabled": true,
        "requiresAuthentication": false,
        "timeoutMs": 30000
    }))
    .unwrap();
    let validated = parsed.clone().validated().unwrap();
    assert_eq!(validated.display_name, "Local");
    assert_eq!(validated.base_url, "http://127.0.0.1:11434");
    assert_eq!(
        validated.provider_type,
        ProviderConfigurationType::ChatCompletions
    );

    let mut too_fast = parsed.clone();
    too_fast.timeout_ms = 999;
    assert!(too_fast.validated().is_err());
    let mut too_slow = parsed.clone();
    too_slow.timeout_ms = 120_001;
    assert!(too_slow.validated().is_err());
    let mut no_name = parsed;
    no_name.display_name = " ".to_string();
    assert!(no_name.validated().is_err());

    let unknown_type: Result<ProviderConfigurationInput, _> = serde_json::from_value(json!({
        "providerType": "openai-compatible",
        "displayName": "x", "source": "local", "baseUrl": "x",
        "enabled": true, "requiresAuthentication": false, "timeoutMs": 1000
    }));
    assert!(unknown_type.is_err());
}
