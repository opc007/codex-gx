//! v1.9.9: Context Compression Tauri 命令
//!
//! - 估算 token
//! - 启发式摘要
//! - 压缩消息列表

use agent_core::message::{ContentBlock, Message, MessageRole};
use context::{compress as ctx_compress, estimate_tokens, summarize, CompressionConfig, KeyFacts, Summary};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct CompressStatus {
    pub version: String,
    pub default_config: CompressionConfig,
}

#[tauri::command]
pub fn compress_status() -> CompressStatus {
    CompressStatus {
        version: "v1.9.9".into(),
        default_config: CompressionConfig::default(),
    }
}

#[derive(Deserialize)]
pub struct EstimateArgs {
    pub messages: Vec<MessageInput>,
}

#[derive(Deserialize)]
pub struct MessageInput {
    pub role: String,
    pub text: String,
}

impl MessageInput {
    fn to_message(&self) -> Message {
        let sid = uuid::Uuid::new_v4();
        match self.role.as_str() {
            "user" | "user" => Message::user(sid, &self.text),
            "assistant" => Message::assistant(sid, &self.text),
            "system" => Message::system(sid, &self.text),
            _ => Message::user(sid, &self.text),
        }
    }
}

#[tauri::command]
pub fn compress_estimate(args: EstimateArgs) -> u32 {
    let msgs: Vec<Message> = args.messages.iter().map(|m| m.to_message()).collect();
    estimate_tokens(&msgs)
}

#[derive(Deserialize)]
pub struct SummaryArgs {
    pub messages: Vec<MessageInput>,
    #[serde(default)]
    pub config: Option<CompressionConfig>,
}

#[tauri::command]
pub fn compress_summarize(args: SummaryArgs) -> Summary {
    let msgs: Vec<Message> = args.messages.iter().map(|m| m.to_message()).collect();
    let cfg = args.config.unwrap_or_default();
    summarize(&msgs, &cfg)
}

#[derive(Deserialize)]
pub struct CompressArgs {
    pub messages: Vec<MessageInput>,
    #[serde(default)]
    pub config: Option<CompressionConfig>,
}

#[derive(Serialize)]
pub struct CompressResult {
    pub summary: Summary,
    pub compressed_count: usize,
    pub original_count: usize,
    pub compressed_messages: Vec<CompressedMsg>,
}

#[derive(Serialize)]
pub struct CompressedMsg {
    pub role: String,
    pub text: String,
}

#[tauri::command]
pub fn compress_run(args: CompressArgs) -> CompressResult {
    let msgs: Vec<Message> = args.messages.iter().map(|m| m.to_message()).collect();
    let cfg = args.config.unwrap_or_default();
    let (compacted, summary) = ctx_compress(&msgs, &cfg);
    let compressed_messages = compacted
        .iter()
        .map(|m| CompressedMsg {
            role: match m.role {
                MessageRole::User => "user".into(),
                MessageRole::Assistant => "assistant".into(),
                MessageRole::System => "system".into(),
                MessageRole::Tool => "tool".into(),
            },
            text: m
                .content
                .iter()
                .filter_map(|c| if let ContentBlock::Text { text } = c { Some(text.clone()) } else { None })
                .collect::<Vec<_>>()
                .join("\n"),
        })
        .collect();
    CompressResult {
        original_count: msgs.len(),
        compressed_count: compacted.len(),
        compressed_messages,
        summary,
    }
}

#[tauri::command]
pub fn compress_facts(args: EstimateArgs) -> KeyFacts {
    let msgs: Vec<Message> = args.messages.iter().map(|m| m.to_message()).collect();
    KeyFacts::from_messages(&msgs)
}

#[tauri::command]
pub fn compress_should_trigger(args: EstimateArgs) -> bool {
    let msgs: Vec<Message> = args.messages.iter().map(|m| m.to_message()).collect();
    let tokens = estimate_tokens(&msgs);
    CompressionConfig::default().should_trigger(tokens)
}