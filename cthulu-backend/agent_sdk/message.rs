use claude_agent_sdk_rust::types::content::ContentBlock;
use claude_agent_sdk_rust::types::messages::{AssistantMessage, Message, ResultMessage};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct SseEvent {
    pub event_type: String,
    pub data: serde_json::Value,
}

/// Convert a single SDK `Message` into zero or more `SseEvent`s.
pub fn sdk_message_to_sse_events(msg: &Message) -> Vec<SseEvent> {
    match msg {
        Message::Assistant(assistant) => assistant_to_events(assistant),
        Message::Result(result) => vec![result_to_event(result)],
        Message::StreamEvent(stream_event) => stream_event_to_events(stream_event),
        Message::System(_) | Message::User(_) => vec![],
    }
}

pub fn extract_session_id(msg: &Message) -> Option<String> {
    match msg {
        Message::Result(r) => Some(r.session_id.clone()),
        Message::Assistant(a) => a.session_id.clone(),
        Message::System(s) => s
            .data
            .get("session_id")
            .and_then(|v| v.as_str())
            .map(String::from),
        Message::StreamEvent(se) => Some(se.session_id.clone()),
        _ => None,
    }
}

fn assistant_to_events(assistant: &AssistantMessage) -> Vec<SseEvent> {
    let mut events = Vec::new();

    for block in &assistant.message.content {
        match block {
            ContentBlock::Text(text_block) => {
                events.push(SseEvent {
                    event_type: "text".to_string(),
                    data: serde_json::json!({ "text": text_block.text }),
                });
            }
            ContentBlock::ToolUse(tool_use) => {
                events.push(SseEvent {
                    event_type: "tool_use".to_string(),
                    data: serde_json::json!({
                        "id": tool_use.id,
                        "tool": tool_use.name,
                        "name": tool_use.name,
                        "input": tool_use.input,
                    }),
                });
            }
            ContentBlock::ToolResult(tool_result) => {
                events.push(SseEvent {
                    event_type: "tool_result".to_string(),
                    data: serde_json::json!({
                        "tool_use_id": tool_result.tool_use_id,
                        "content": tool_result.content,
                        "is_error": tool_result.is_error.unwrap_or(false),
                    }),
                });
            }
            ContentBlock::Thinking(_) => {}
        }
    }

    if let Some(error) = &assistant.error {
        events.push(SseEvent {
            event_type: "error".to_string(),
            data: serde_json::json!({ "error": error }),
        });
    }

    events
}

fn result_to_event(result: &ResultMessage) -> SseEvent {
    SseEvent {
        event_type: "result".to_string(),
        data: serde_json::json!({
            "text": result.result.as_deref().unwrap_or(""),
            "cost": result.total_cost_usd.unwrap_or(0.0),
            "turns": result.num_turns,
            "duration_ms": result.duration_ms,
            "is_error": result.is_error,
            "subtype": result.subtype,
            "session_id": result.session_id,
            "structured_output": result.structured_output,
        }),
    }
}

fn stream_event_to_events(
    stream_event: &claude_agent_sdk_rust::types::messages::StreamEvent,
) -> Vec<SseEvent> {
    let event = &stream_event.event;

    let Some(event_type) = event.get("type").and_then(|t| t.as_str()) else {
        return vec![];
    };

    if event_type == "content_block_delta" {
        if let Some(text) = event
            .get("delta")
            .and_then(|d| d.get("text"))
            .and_then(|t| t.as_str())
        {
            return vec![SseEvent {
                event_type: "text".to_string(),
                data: serde_json::json!({ "text": text }),
            }];
        }
    }

    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;
    use claude_agent_sdk_rust::types::content::{TextBlock, ToolUseBlock};
    use claude_agent_sdk_rust::types::messages::AssistantMessageInner;

    #[test]
    fn test_text_message_to_sse() {
        let msg = Message::Assistant(AssistantMessage {
            message: AssistantMessageInner {
                content: vec![ContentBlock::Text(TextBlock {
                    text: "Hello world".to_string(),
                })],
                id: None,
                model: "claude-opus-4-20250514".to_string(),
                role: Some("assistant".to_string()),
                stop_reason: None,
                stop_sequence: None,
                message_type: None,
                usage: None,
            },
            parent_tool_use_id: None,
            session_id: Some("test-session".to_string()),
            error: None,
        });

        let events = sdk_message_to_sse_events(&msg);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "text");
        assert_eq!(events[0].data["text"], "Hello world");
    }

    #[test]
    fn test_tool_use_to_sse() {
        let msg = Message::Assistant(AssistantMessage {
            message: AssistantMessageInner {
                content: vec![ContentBlock::ToolUse(ToolUseBlock {
                    id: "tu_123".to_string(),
                    name: "Bash".to_string(),
                    input: serde_json::json!({"command": "ls -la"}),
                })],
                id: None,
                model: "claude-opus-4-20250514".to_string(),
                role: Some("assistant".to_string()),
                stop_reason: None,
                stop_sequence: None,
                message_type: None,
                usage: None,
            },
            parent_tool_use_id: None,
            session_id: None,
            error: None,
        });

        let events = sdk_message_to_sse_events(&msg);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "tool_use");
        assert_eq!(events[0].data["tool"], "Bash");
        assert_eq!(events[0].data["id"], "tu_123");
    }

    #[test]
    fn test_result_to_sse() {
        let msg = Message::Result(ResultMessage {
            subtype: "success".to_string(),
            duration_ms: 5000,
            duration_api_ms: 4500,
            is_error: false,
            num_turns: 3,
            session_id: "test-session".to_string(),
            total_cost_usd: Some(0.042),
            usage: None,
            result: Some("Task completed.".to_string()),
            structured_output: None,
        });

        let events = sdk_message_to_sse_events(&msg);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "result");
        assert_eq!(events[0].data["cost"], 0.042);
        assert_eq!(events[0].data["turns"], 3);
    }
}
