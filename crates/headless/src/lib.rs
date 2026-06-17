//! v1.7：Headless / JSON 模式
//!
//! 设计参考：docs/开发文档.md §5.23
//!
//! ## 输出格式
//! - `text`         — 纯文本
//! - `json`         — JSON Lines（每行一个 event）
//! - `stream-json`  — 流式 JSON event-by-event
//!
//! ## 6 种 event
//! - `thread.started`
//! - `turn.started`
//! - `item.completed`
//! - `item.delta`       (stream-json only)
//! - `turn.completed`
//! - `thread.completed`
//!
//! ## 退出码
//! - 0 成功
//! - 1 任务失败
//! - 2 网络错误
//! - 3 max-turns 超

use serde::{Deserialize, Serialize};

/// 输出格式
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum OutputFormat {
    Text,
    Json,
    StreamJson,
}

impl Default for OutputFormat {
    fn default() -> Self {
        Self::Text
    }
}

impl OutputFormat {
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "json" => Self::Json,
            "stream-json" | "stream_json" | "streamjson" => Self::StreamJson,
            _ => Self::Text,
        }
    }
}

/// Event（流式/JSON 输出）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// thread 启动
    ThreadStarted {
        thread_id: String,
        ts: i64,
    },
    /// 一次 turn 启动
    TurnStarted {
        turn_id: String,
        ts: i64,
    },
    /// 完成一个 item
    ItemCompleted {
        item: Item,
    },
    /// 流式 delta（仅 stream-json）
    ItemDelta {
        item_id: String,
        kind: String,
        text: String,
    },
    /// turn 完成
    TurnCompleted {
        turn_id: String,
        usage: Usage,
        ts: i64,
    },
    /// thread 完成
    ThreadCompleted {
        thread_id: String,
        status: String, // success | failed | max_turns
        ts: i64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Item {
    Reasoning {
        text: String,
    },
    CommandExecution {
        command: String,
        exit_code: i32,
        output: String,
    },
    FileChange {
        path: String,
        diff: String,
    },
    AgentMessage {
        text: String,
    },
    ToolCall {
        name: String,
        args: serde_json::Value,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cost_usd: f32,
}

/// Writer trait
pub trait EventWriter {
    fn write(&mut self, ev: &Event) -> anyhow::Result<()>;
    fn flush(&mut self) -> anyhow::Result<()>;
}

/// 文本 writer
pub struct TextWriter<W: std::io::Write> {
    w: W,
}

impl<W: std::io::Write> TextWriter<W> {
    pub fn new(w: W) -> Self {
        Self { w }
    }
}

impl<W: std::io::Write> EventWriter for TextWriter<W> {
    fn write(&mut self, ev: &Event) -> anyhow::Result<()> {
        match ev {
            Event::ItemCompleted { item } => match item {
                Item::AgentMessage { text } => {
                    writeln!(self.w, "{}", text)?;
                }
                Item::Reasoning { text } => {
                    writeln!(self.w, "[reasoning] {}", text)?;
                }
                Item::CommandExecution { command, exit_code, output } => {
                    writeln!(self.w, "$ {}", command)?;
                    writeln!(self.w, "[exit={}] {}", exit_code, output)?;
                }
                Item::FileChange { path, diff } => {
                    writeln!(self.w, "--- {} ---", path)?;
                    writeln!(self.w, "{}", diff)?;
                }
                Item::ToolCall { name, args } => {
                    writeln!(self.w, "[tool] {}({})", name, serde_json::to_string(args)?)?;
                }
            },
            Event::ItemDelta { text, .. } => {
                write!(self.w, "{}", text)?;
            }
            Event::ThreadCompleted { status, .. } => {
                writeln!(self.w, "[status: {}]", status)?;
            }
            _ => {}
        }
        Ok(())
    }
    fn flush(&mut self) -> anyhow::Result<()> {
        self.w.flush()?;
        Ok(())
    }
}

/// JSON Lines writer
pub struct JsonWriter<W: std::io::Write> {
    w: W,
}

impl<W: std::io::Write> JsonWriter<W> {
    pub fn new(w: W) -> Self {
        Self { w }
    }
}

impl<W: std::io::Write> EventWriter for JsonWriter<W> {
    fn write(&mut self, ev: &Event) -> anyhow::Result<()> {
        let s = serde_json::to_string(ev)?;
        writeln!(self.w, "{}", s)?;
        Ok(())
    }
    fn flush(&mut self) -> anyhow::Result<()> {
        self.w.flush()?;
        Ok(())
    }
}

/// stream-json writer（每 event 一行，delta 也行）
pub struct StreamJsonWriter<W: std::io::Write> {
    w: W,
}

impl<W: std::io::Write> StreamJsonWriter<W> {
    pub fn new(w: W) -> Self {
        Self { w }
    }
}

impl<W: std::io::Write> EventWriter for StreamJsonWriter<W> {
    fn write(&mut self, ev: &Event) -> anyhow::Result<()> {
        let s = serde_json::to_string(ev)?;
        writeln!(self.w, "{}", s)?;
        Ok(())
    }
    fn flush(&mut self) -> anyhow::Result<()> {
        self.w.flush()?;
        Ok(())
    }
}

/// Factory
pub fn writer_for(format: OutputFormat) -> Box<dyn EventWriter> {
    match format {
        OutputFormat::Text => Box::new(TextWriter::new(std::io::stdout())),
        OutputFormat::Json | OutputFormat::StreamJson => {
            Box::new(JsonWriter::new(std::io::stdout()))
        }
    }
}

/// 退出码
pub fn exit_code_for(status: &str) -> i32 {
    match status {
        "success" => 0,
        "failed" => 1,
        "network" => 2,
        "max_turns" => 3,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_parse() {
        assert_eq!(OutputFormat::parse("text"), OutputFormat::Text);
        assert_eq!(OutputFormat::parse("json"), OutputFormat::Json);
        assert_eq!(
            OutputFormat::parse("stream-json"),
            OutputFormat::StreamJson
        );
    }

    #[test]
    fn test_text_writer() {
        let mut buf = Vec::new();
        {
            let mut w = TextWriter::new(&mut buf);
            w.write(&Event::ItemCompleted {
                item: Item::AgentMessage {
                    text: "hello".into(),
                },
            })
            .unwrap();
        }
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("hello"));
    }

    #[test]
    fn test_json_writer() {
        let mut buf = Vec::new();
        {
            let mut w = JsonWriter::new(&mut buf);
            w.write(&Event::ThreadStarted {
                thread_id: "t1".into(),
                ts: 0,
            })
            .unwrap();
        }
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("\"thread_started\""));
        assert!(s.contains("\"thread_id\":\"t1\""));
    }

    #[test]
    fn test_exit_codes() {
        assert_eq!(exit_code_for("success"), 0);
        assert_eq!(exit_code_for("failed"), 1);
        assert_eq!(exit_code_for("network"), 2);
        assert_eq!(exit_code_for("max_turns"), 3);
    }
}
