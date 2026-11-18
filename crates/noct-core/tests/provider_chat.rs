use noct_core::ai::bindings::openrouter_default;
use noct_core::ai::openrouter::{
    ChatFailure, ChatMessage, Role, StreamEvent, chat_body, read_event, status_failure,
    validated_model_id,
};
use noct_core::ai::sse::{SseError, SseEvent, SseParser};
use noct_core::limits::provider_chat::{MAXIMUM_EVENT_BYTES, MAXIMUM_MODEL_CHARACTERS};
use serde_json::Value;

/// Reads a whole body in one piece and ends the stream.
fn read_all(body: &[u8]) -> Result<Vec<SseEvent>, SseError> {
    let mut parser = SseParser::new();
    let events = parser.push(body)?;
    parser.finish()?;
    Ok(events)
}

#[test]
fn events_are_read_across_chunk_boundaries() {
    let body = b"data: {\"a\":1}\n\ndata: [DONE]\n\n";
    let mut parser = SseParser::new();
    let mut events = Vec::new();
    for byte in body {
        events.extend(parser.push(std::slice::from_ref(byte)).unwrap());
    }
    parser.finish().unwrap();
    assert_eq!(
        events,
        vec![
            SseEvent {
                data: "{\"a\":1}".to_string()
            },
            SseEvent {
                data: "[DONE]".to_string()
            },
        ]
    );
}

#[test]
fn crlf_line_ends_are_read_and_comment_lines_are_skipped() {
    let events = read_all(b": keep-alive\r\ndata: one\r\n\r\n").unwrap();
    assert_eq!(
        events,
        vec![SseEvent {
            data: "one".to_string()
        }]
    );
}

#[test]
fn several_data_lines_join_into_one_event_with_a_newline() {
    let events = read_all(b"data: first\ndata: second\n\n").unwrap();
    assert_eq!(
        events,
        vec![SseEvent {
            data: "first\nsecond".to_string()
        }]
    );
}

#[test]
fn an_event_field_and_an_id_are_accepted_and_ignored() {
    let events = read_all(b"event: message\nid: 7\ndata: x\n\n").unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].data, "x");
}

#[test]
fn an_unknown_field_is_malformed_and_nothing_is_skipped() {
    assert_eq!(read_all(b"foo: x\n\n"), Err(SseError::Malformed));
}

#[test]
fn a_carriage_return_that_is_not_a_crlf_end_is_malformed() {
    assert_eq!(read_all(b"data: a\rb\n\n"), Err(SseError::Malformed));
}

#[test]
fn bytes_that_are_not_utf8_are_refused() {
    assert_eq!(read_all(b"data: \xff\xfe\n\n"), Err(SseError::NotUtf8));
}

#[test]
fn a_line_over_the_cap_is_refused_without_waiting_for_its_end() {
    let mut parser = SseParser::new();
    let line = vec![b'a'; MAXIMUM_EVENT_BYTES + 1];
    assert_eq!(parser.push(&line), Err(SseError::TooLarge));
}

#[test]
fn the_data_of_one_event_over_the_cap_is_refused_across_lines() {
    let mut body = Vec::new();
    for _ in 0..(MAXIMUM_EVENT_BYTES / 1000 + 2) {
        body.extend_from_slice(b"data: ");
        body.extend(std::iter::repeat_n(b'x', 1000));
        body.push(b'\n');
    }
    assert_eq!(read_all(&body), Err(SseError::TooLarge));
}

#[test]
fn a_stream_that_ends_inside_a_line_is_truncated() {
    assert_eq!(
        read_all(b"data: {\"a\":1}\n\ndata: [DO"),
        Err(SseError::Truncated)
    );
}

#[test]
fn an_event_without_its_blank_line_is_truncated() {
    assert_eq!(read_all(b"data: {\"a\":1}\n"), Err(SseError::Truncated));
}

#[test]
fn a_stream_that_ends_between_events_is_complete_for_the_parser() {
    assert_eq!(read_all(b"data: a\n\n").unwrap().len(), 1);
}

#[test]
fn the_request_names_the_model_and_asks_for_a_stream() {
    let body = chat_body(
        "openai/gpt-4o:free",
        &[
            ChatMessage {
                role: Role::System,
                content: "s".to_string(),
            },
            ChatMessage {
                role: Role::User,
                content: "u".to_string(),
            },
        ],
    );
    let value: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(value["model"], "openai/gpt-4o:free");
    assert_eq!(value["stream"], true);
    assert_eq!(value["messages"][0]["role"], "system");
    assert_eq!(value["messages"][1]["role"], "user");
    assert_eq!(
        value.as_object().unwrap().len(),
        3,
        "only model, messages and stream"
    );
}

#[test]
fn the_body_escapes_quotes_and_newlines_in_the_content() {
    let content = "pergunta \"com\" aspas\ne quebra\\";
    let body = chat_body(
        "a/b",
        &[ChatMessage {
            role: Role::User,
            content: content.to_string(),
        }],
    );
    let value: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(value["messages"][0]["content"], content);
}

#[test]
fn a_delta_is_read_and_done_ends_the_answer() {
    let delta = read_event(r#"{"choices":[{"delta":{"content":"oi"}}]}"#).unwrap();
    assert_eq!(delta, Some(StreamEvent::Delta("oi".to_string())));
    assert_eq!(read_event("[DONE]").unwrap(), Some(StreamEvent::Done));
}

#[test]
fn a_role_only_or_usage_chunk_carries_no_text() {
    assert_eq!(
        read_event(r#"{"choices":[{"delta":{"role":"assistant"}}]}"#).unwrap(),
        None
    );
    assert_eq!(
        read_event(r#"{"choices":[{"delta":{"content":null}}]}"#).unwrap(),
        None
    );
    assert_eq!(
        read_event(r#"{"choices":[{"delta":{"content":""}}]}"#).unwrap(),
        None
    );
    assert_eq!(
        read_event(r#"{"choices":[],"usage":{"total_tokens":3}}"#).unwrap(),
        None
    );
}

#[test]
fn an_error_object_in_the_stream_is_a_failure_and_its_text_is_not_kept() {
    let failure =
        read_event(r#"{"error":{"message":"echoed sk-or-v1-secret-value"}}"#).unwrap_err();
    assert_eq!(failure, ChatFailure::ProviderError);
    assert_eq!(failure.code(), "provider-error");
    assert!(
        !format!("{failure:?}").contains("sk-or"),
        "the failure holds no provider text"
    );
}

#[test]
fn a_chunk_that_is_not_json_or_has_no_choices_is_malformed() {
    assert_eq!(read_event("not json"), Err(ChatFailure::Malformed));
    assert_eq!(read_event(r#"{"other":1}"#), Err(ChatFailure::Malformed));
}

#[test]
fn a_content_of_the_wrong_type_is_malformed() {
    assert_eq!(
        read_event(r#"{"choices":[{"delta":{"content":5}}]}"#),
        Err(ChatFailure::Malformed)
    );
}

#[test]
fn status_codes_map_to_fixed_failures() {
    assert_eq!(status_failure(200), None);
    assert_eq!(status_failure(299), None);
    assert_eq!(status_failure(401), Some(ChatFailure::Unauthorized));
    assert_eq!(status_failure(403), Some(ChatFailure::Unauthorized));
    assert_eq!(status_failure(429), Some(ChatFailure::RateLimited));
    assert_eq!(status_failure(503), Some(ChatFailure::Unavailable));
    assert_eq!(status_failure(418), Some(ChatFailure::Rejected));
}

#[test]
fn the_failure_codes_are_stable_and_hold_no_provider_text() {
    let codes = [
        ChatFailure::Malformed,
        ChatFailure::Truncated,
        ChatFailure::TooLarge,
        ChatFailure::ProviderError,
        ChatFailure::Unauthorized,
        ChatFailure::RateLimited,
        ChatFailure::Unavailable,
        ChatFailure::Rejected,
        ChatFailure::Transport,
        ChatFailure::Timeout,
        ChatFailure::Cancelled,
    ];
    let names: Vec<&str> = codes.iter().map(|code| code.code()).collect();
    assert_eq!(
        names,
        [
            "stream-malformed",
            "stream-truncated",
            "response-too-large",
            "provider-error",
            "provider-unauthorized",
            "provider-rate-limited",
            "provider-unavailable",
            "provider-rejected",
            "transport",
            "timeout",
            "cancelled",
        ]
    );
}

#[test]
fn sse_errors_map_to_chat_failures() {
    assert_eq!(ChatFailure::from(SseError::NotUtf8), ChatFailure::Malformed);
    assert_eq!(
        ChatFailure::from(SseError::Malformed),
        ChatFailure::Malformed
    );
    assert_eq!(ChatFailure::from(SseError::TooLarge), ChatFailure::TooLarge);
    assert_eq!(
        ChatFailure::from(SseError::Truncated),
        ChatFailure::Truncated
    );
}

#[test]
fn model_ids_of_the_vendor_and_name_shape_are_accepted_with_or_without_a_variant() {
    assert_eq!(
        validated_model_id("openai/gpt-4o").unwrap(),
        "openai/gpt-4o"
    );
    assert_eq!(
        validated_model_id("meta-llama/llama-3.1-8b-instruct:free").unwrap(),
        "meta-llama/llama-3.1-8b-instruct:free"
    );
}

#[test]
fn model_ids_outside_the_shape_or_the_characters_are_refused() {
    for id in [
        "", "gpt-4o", "a/b/c", "/name", "vendor/", "a b/c", "a/b\"", "a/\u{e9}", "a/b\n",
    ] {
        assert!(validated_model_id(id).is_err(), "{id:?}");
    }
}

#[test]
fn a_model_id_at_the_maximum_length_is_accepted_and_one_more_is_refused() {
    let name = "x".repeat(MAXIMUM_MODEL_CHARACTERS - 2);
    let at_limit = format!("a/{name}");
    assert_eq!(at_limit.len(), MAXIMUM_MODEL_CHARACTERS);
    assert!(validated_model_id(&at_limit).is_ok());
    let over = format!("a/{name}x");
    assert!(validated_model_id(&over).is_err());
}

#[test]
fn the_openrouter_default_fixes_the_provider_and_keeps_the_identifier() {
    let model = openrouter_default("openai/gpt-4o:free").unwrap();
    assert_eq!(model.provider_id, "openrouter");
    assert_eq!(model.model_id, "openai/gpt-4o:free");
}

#[test]
fn an_openrouter_default_with_a_bad_identifier_is_refused() {
    for id in ["", "gpt-4o", "a/b c", "a/b\n"] {
        assert!(openrouter_default(id).is_err(), "{id:?}");
    }
    let too_long = format!("a/{}", "x".repeat(MAXIMUM_MODEL_CHARACTERS));
    assert!(openrouter_default(&too_long).is_err());
}
