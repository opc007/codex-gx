//! v1.7：Headless CLI（CI/CD 用）
//!
//! 用法：
//! ```bash
//! # 默认 text 模式
//! agentshell-headless "fix the failing test in src/auth.test.ts"
//!
//! # JSON Lines（CI 解析）
//! agentshell-headless --format json "add TypeScript strict to tsconfig.json"
//!
//! # Stream JSON
//! agentshell-headless --format stream-json --max-turns 50 "migrate all .jsx to .tsx"
//!
//! # 不审批（CI 默认）
//! agentshell-headless --approval auto "review this PR"
//! ```

use headless::{Event, EventWriter, Item, Usage, OutputFormat, writer_for, exit_code_for};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 || args[1] == "--help" || args[1] == "-h" {
        print_help();
        return;
    }

    // parse args
    let mut prompt: Option<String> = None;
    let mut format = OutputFormat::Text;
    let mut max_turns: u32 = 30;
    let mut approval_auto = false;

    let mut i = 1;
    while i < args.len() {
        let a = &args[i];
        match a.as_str() {
            "--format" | "-f" => {
                if let Some(v) = args.get(i + 1) {
                    format = OutputFormat::parse(v);
                    i += 2;
                } else {
                    eprintln!("--format 需要参数");
                    std::process::exit(1);
                }
            }
            "--max-turns" => {
                if let Some(v) = args.get(i + 1) {
                    max_turns = v.parse().unwrap_or(30);
                    i += 2;
                } else {
                    eprintln!("--max-turns 需要数字");
                    std::process::exit(1);
                }
            }
            "--approval" => {
                if let Some(v) = args.get(i + 1) {
                    approval_auto = v == "auto";
                    i += 2;
                } else {
                    eprintln!("--approval 需要参数");
                    std::process::exit(1);
                }
            }
            other => {
                if other.starts_with("--") {
                    eprintln!("unknown flag: {other}");
                    std::process::exit(1);
                }
                if prompt.is_none() {
                    prompt = Some(other.to_string());
                }
                i += 1;
            }
        }
    }

    let prompt = match prompt {
        Some(p) => p,
        None => {
            eprintln!("❌ 缺少 prompt");
            print_help();
            std::process::exit(1);
        }
    };

    // 演示版：直接走演示事件流（不连真实 M3）
    // 真实版会调 provider.chat
    let mut w = writer_for(format);
    let thread_id = format!("thread-{}", uuid::Uuid::new_v4());

    let _ = w.write(&Event::ThreadStarted {
        thread_id: thread_id.clone(),
        ts: now(),
    });
    let _ = w.flush();

    let turn_id = format!("turn-{}", uuid::Uuid::new_v4());
    let _ = w.write(&Event::TurnStarted {
        turn_id: turn_id.clone(),
        ts: now(),
    });

    // 演示：发出 1 个 agent message
    let _ = w.write(&Event::ItemCompleted {
        item: Item::AgentMessage {
            text: format!(
                "📦 [headless demo]\n收到 prompt：{}\nformat={:?}\nmax_turns={}\napproval={}\n\n⚠️  这是演示事件流 — 真实版本会调 provider.chat 流式输出。\n",
                prompt, format, max_turns, if approval_auto { "auto" } else { "on-request" }
            ),
        },
    });
    let _ = w.write(&Event::TurnCompleted {
        turn_id,
        usage: Usage {
            input_tokens: 100,
            output_tokens: 50,
            cost_usd: 0.001,
        },
        ts: now(),
    });
    let status = if max_turns == 0 { "max_turns" } else { "success" };
    let _ = w.write(&Event::ThreadCompleted {
        thread_id,
        status: status.to_string(),
        ts: now(),
    });
    let _ = w.flush();

    std::process::exit(exit_code_for(status));
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

fn print_help() {
    eprintln!("agentshell-headless — Codex gx v1.7 headless mode");
    eprintln!();
    eprintln!("USAGE:");
    eprintln!("    agentshell-headless <prompt> [options]");
    eprintln!();
    eprintln!("OPTIONS:");
    eprintln!("    -f, --format <text|json|stream-json>   输出格式（默认 text）");
    eprintln!("        --max-turns <N>                    最大 turn 数（默认 30）");
    eprintln!("        --approval <auto|on-request>      审批模式（CI 用 auto）");
    eprintln!();
    eprintln!("EXAMPLES:");
    eprintln!("    agentshell-headless \"fix the bug\"");
    eprintln!("    agentshell-headless --format json \"run tests\" > out.jsonl");
    eprintln!("    agentshell-headless --format stream-json --max-turns 5 \"review\"");
}
