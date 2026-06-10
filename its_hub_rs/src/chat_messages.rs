use crate::types::{ChatMessage, Content};

/// Unified wrapper for handling both string prompts and conversation history.
///
/// Mirrors Python's `ChatMessages` from `its_hub/types.py`.
#[derive(Debug, Clone)]
pub enum ChatMessages {
    Text(String),
    Messages(Vec<ChatMessage>),
}

impl ChatMessages {
    /// Create ChatMessages from a string prompt.
    pub fn from_string(s: impl Into<String>) -> Self {
        ChatMessages::Text(s.into())
    }

    /// Create ChatMessages from a list of ChatMessage.
    pub fn from_messages(messages: Vec<ChatMessage>) -> Self {
        ChatMessages::Messages(messages)
    }

    /// Convert to prompt string representation.
    ///
    /// If string: returns the string.
    /// If messages: joins all content with newlines, prefixed by role.
    /// Mirrors Python's `to_prompt()` method.
    pub fn to_prompt(&self) -> String {
        match self {
            ChatMessages::Text(s) => s.clone(),
            ChatMessages::Messages(messages) => {
                let mut lines = Vec::new();
                for msg in messages {
                    let text_content = msg.extract_text_content();

                    if msg.role == "tool" {
                        let tool_call_id = msg.tool_call_id.as_deref().unwrap_or("");
                        lines.push(format!("tool[{tool_call_id}]: {text_content}"));
                    } else if msg.role == "assistant" && msg.tool_calls.is_some() {
                        let tool_calls = msg.tool_calls.as_ref().unwrap();
                        let tool_call_strs: Vec<String> = tool_calls
                            .iter()
                            .filter_map(|tc| tc.function.as_ref().map(|f| format!("{}()", f.name)))
                            .collect();
                        let tool_calls_text = tool_call_strs.join(", ");
                        if !text_content.is_empty() {
                            lines.push(format!(
                                "assistant: {text_content} [calls: {tool_calls_text}]"
                            ));
                        } else {
                            lines.push(format!("assistant: [calls: {tool_calls_text}]"));
                        }
                    } else {
                        lines.push(format!("{}: {text_content}", msg.role));
                    }
                }
                lines.join("\n")
            }
        }
    }

    /// Convert to list of ChatMessage objects.
    ///
    /// If string: returns a single user message wrapping the string.
    /// If messages: returns the messages directly.
    pub fn to_chat_messages(&self) -> Vec<ChatMessage> {
        match self {
            ChatMessages::Text(s) => vec![ChatMessage {
                role: "user".to_string(),
                content: Some(Content::Text(s.clone())),
                tool_calls: None,
                tool_call_id: None,
            }],
            ChatMessages::Messages(msgs) => msgs.clone(),
        }
    }

    /// Create a batch of identical chat message lists for parallel generation.
    pub fn to_batch(&self, size: usize) -> Vec<Vec<ChatMessage>> {
        let chat_messages = self.to_chat_messages();
        vec![chat_messages; size]
    }

    /// Check if the original input was a string.
    pub fn is_string(&self) -> bool {
        matches!(self, ChatMessages::Text(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ToolCall, ToolFunction};

    fn user_msg(text: &str) -> ChatMessage {
        ChatMessage {
            role: "user".to_string(),
            content: Some(Content::Text(text.to_string())),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    fn assistant_msg(text: &str) -> ChatMessage {
        ChatMessage {
            role: "assistant".to_string(),
            content: Some(Content::Text(text.to_string())),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    #[test]
    fn test_from_string() {
        let cm = ChatMessages::from_string("What is 2+2?");
        assert!(cm.is_string());
    }

    #[test]
    fn test_from_messages() {
        let msgs = vec![user_msg("hello"), assistant_msg("hi")];
        let cm = ChatMessages::from_messages(msgs);
        assert!(!cm.is_string());
    }

    #[test]
    fn test_to_prompt_from_string() {
        let cm = ChatMessages::from_string("What is 2+2?");
        assert_eq!(cm.to_prompt(), "What is 2+2?");
    }

    #[test]
    fn test_to_prompt_from_messages() {
        let msgs = vec![user_msg("hello"), assistant_msg("hi there")];
        let cm = ChatMessages::from_messages(msgs);
        assert_eq!(cm.to_prompt(), "user: hello\nassistant: hi there");
    }

    #[test]
    fn test_to_prompt_tool_message() {
        let msgs = vec![ChatMessage {
            role: "tool".to_string(),
            content: Some(Content::Text("result data".to_string())),
            tool_calls: None,
            tool_call_id: Some("call_123".to_string()),
        }];
        let cm = ChatMessages::from_messages(msgs);
        assert_eq!(cm.to_prompt(), "tool[call_123]: result data");
    }

    #[test]
    fn test_to_prompt_assistant_with_tool_calls() {
        let msgs = vec![ChatMessage {
            role: "assistant".to_string(),
            content: Some(Content::Text("Looking up weather".to_string())),
            tool_calls: Some(vec![ToolCall {
                id: "call_1".to_string(),
                call_type: "function".to_string(),
                function: Some(ToolFunction {
                    name: "get_weather".to_string(),
                    arguments: serde_json::json!({}),
                }),
            }]),
            tool_call_id: None,
        }];
        let cm = ChatMessages::from_messages(msgs);
        assert_eq!(
            cm.to_prompt(),
            "assistant: Looking up weather [calls: get_weather()]"
        );
    }

    #[test]
    fn test_to_prompt_assistant_tool_calls_no_content() {
        let msgs = vec![ChatMessage {
            role: "assistant".to_string(),
            content: None,
            tool_calls: Some(vec![ToolCall {
                id: "call_1".to_string(),
                call_type: "function".to_string(),
                function: Some(ToolFunction {
                    name: "calc".to_string(),
                    arguments: serde_json::json!({}),
                }),
            }]),
            tool_call_id: None,
        }];
        let cm = ChatMessages::from_messages(msgs);
        assert_eq!(cm.to_prompt(), "assistant: [calls: calc()]");
    }

    #[test]
    fn test_to_chat_messages_from_string() {
        let cm = ChatMessages::from_string("What is 2+2?");
        let msgs = cm.to_chat_messages();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].role, "user");
        assert_eq!(msgs[0].extract_text_content(), "What is 2+2?");
    }

    #[test]
    fn test_to_chat_messages_from_messages() {
        let original = vec![user_msg("hello"), assistant_msg("hi")];
        let cm = ChatMessages::from_messages(original.clone());
        let result = cm.to_chat_messages();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].role, "user");
        assert_eq!(result[1].role, "assistant");
    }

    #[test]
    fn test_to_batch() {
        let cm = ChatMessages::from_string("test prompt");
        let batch = cm.to_batch(3);
        assert_eq!(batch.len(), 3);
        for item in &batch {
            assert_eq!(item.len(), 1);
            assert_eq!(item[0].role, "user");
            assert_eq!(item[0].extract_text_content(), "test prompt");
        }
    }

    #[test]
    fn test_to_batch_from_messages() {
        let msgs = vec![user_msg("hello"), assistant_msg("hi")];
        let cm = ChatMessages::from_messages(msgs);
        let batch = cm.to_batch(2);
        assert_eq!(batch.len(), 2);
        for item in &batch {
            assert_eq!(item.len(), 2);
        }
    }

    #[test]
    fn test_is_string() {
        let text = ChatMessages::from_string("hello");
        assert!(text.is_string());

        let msgs = ChatMessages::from_messages(vec![user_msg("hello")]);
        assert!(!msgs.is_string());
    }
}
