use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChatType {
    Direct,
    Group,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalCardStatus {
    Approved,
    Denied,
    Error,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AdapterEvent {
    Ready {
        platform: String,
    },
    Message {
        platform: String,
        message_id: String,
        chat_id: String,
        chat_type: ChatType,
        user_id: String,
        content: String,
        mentioned_bot: bool,
        #[serde(default)]
        thread_id: Option<String>,
        #[serde(default)]
        root_id: Option<String>,
        #[serde(default)]
        card_task_id: Option<String>,
        #[serde(default)]
        image_paths: Vec<String>,
        #[serde(default)]
        quoted_text: Option<String>,
    },
    Error {
        platform: String,
        message: String,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AdapterCommand {
    Reply {
        platform: String,
        message_id: String,
        chat_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        thread_id: Option<String>,
        content: String,
        finished: bool,
    },
    Approval {
        platform: String,
        message_id: String,
        chat_id: String,
        token: String,
        task_id: String,
        title: String,
        content: String,
    },
    ApprovalResult {
        platform: String,
        message_id: String,
        chat_id: String,
        task_id: String,
        status: ApprovalCardStatus,
        content: String,
    },
}

impl AdapterEvent {
    pub fn platform(&self) -> &str {
        match self {
            Self::Ready { platform }
            | Self::Message { platform, .. }
            | Self::Error { platform, .. } => platform,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_flat_message_envelope() {
        let event: AdapterEvent = serde_json::from_str(
            r#"{"type":"message","platform":"wecom","message_id":"m1","chat_id":"c1","chat_type":"group","user_id":"u1","content":"hello","mentioned_bot":true}"#,
        )
        .unwrap();
        assert!(matches!(
            event,
            AdapterEvent::Message {
                mentioned_bot: true,
                ..
            }
        ));
    }

    #[test]
    fn parses_lark_topic_and_reply_root_ids_separately() {
        let event: AdapterEvent = serde_json::from_str(
            r#"{"type":"message","platform":"lark","message_id":"m1","chat_id":"c1","chat_type":"group","user_id":"u1","content":"hello","mentioned_bot":true,"thread_id":"omt_topic_1","root_id":"om_root_1"}"#,
        )
        .unwrap();
        assert!(matches!(
            event,
            AdapterEvent::Message {
                thread_id: Some(thread_id),
                root_id: Some(root_id),
                ..
            } if thread_id == "omt_topic_1" && root_id == "om_root_1"
        ));
    }

    #[test]
    fn parses_optional_local_image_paths() {
        let event: AdapterEvent = serde_json::from_str(
            r#"{"type":"message","platform":"wecom","message_id":"m1","chat_id":"u1","chat_type":"direct","user_id":"u1","content":"分析图片","mentioned_bot":true,"image_paths":["/tmp/private/image.png"]}"#,
        )
        .unwrap();
        assert!(matches!(
            event,
            AdapterEvent::Message { image_paths, .. }
                if image_paths == vec!["/tmp/private/image.png"]
        ));
    }
}
