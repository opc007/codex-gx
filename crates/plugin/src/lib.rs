//! v1.5：插件热加载
//!
//! 设计（简化路径）：
//! - 插件 = 一个 JSON manifest + 可选 script / pre-send / post-recv / slash 钩子
//! - 脚本执行：内嵌 mini-DSL（链式 string operations）
//! - 热加载：扫描 `~/.agentshell/plugins/*.json` 即可生效（app 启动时 + 手动 reload）
//! - 安装：把 JSON 字符串写入该目录
//!
//! 后续可扩展为：
//! - WASM 插件（wasmtime / wasmer）
//! - libloading 原生 dylib
//! - 远程注册中心

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HookKind {
    PreSend,
    PostRecv,
    Slash,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hook {
    pub kind: HookKind,
    /// `slash` 钩子的命令名（如 `/todo` → `todo`）
    #[serde(default)]
    pub command: Option<String>,
    /// 描述（用于 UI 显示 /slash 帮助）
    #[serde(default)]
    pub description: Option<String>,
    /// 脚本（DLS 字符串）
    #[serde(default)]
    pub script: Option<String>,
    /// 链式步骤（与 script 二选一；step 列表更结构化）
    #[serde(default)]
    pub steps: Vec<PluginStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum PluginStep {
    /// 去除首尾空白
    Trim,
    /// 全部小写 / 大写
    Lower,
    Upper,
    /// 在尾部追加字符串（arg 为要追加的文本）
    Append {
        arg: String,
    },
    /// 在头部插入字符串
    Prepend {
        arg: String,
    },
    /// 替换（arg = "find::with"）
    Replace {
        arg: String,
    },
    /// 截断到 N 字符
    Truncate {
        arg: String,
    },
    /// 用前缀包裹
    Wrap {
        arg: String,
    },
    /// 输出模板：把 $INPUT 替换为当前 text
    Template {
        arg: String,
    },
    /// 行级处理：把每行包到 markdown checkbox `- [ ] ...`
    ToChecklist,
    /// 行级处理：把每行包到 markdown bullet
    ToBullets,
    /// 拼接 N 次
    Repeat {
        arg: String,
    },
    /// 反转字符串
    Reverse,
    /// 自定义 key→value 注入（不修改 text，仅作为元数据）
    Meta {
        arg: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub hooks: Vec<Hook>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginRegistry {
    pub plugins: HashMap<String, PluginManifest>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self::default()
    }
}

pub fn plugin_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".agentshell").join("plugins")
}

pub fn load_registry() -> PluginRegistry {
    let dir = plugin_dir();
    let mut reg = PluginRegistry::new();
    if !dir.exists() {
        return reg;
    }
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return reg,
    };
    for e in entries.flatten() {
        let path = e.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let data = match std::fs::read_to_string(&path) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let m: PluginManifest = match serde_json::from_str(&data) {
            Ok(m) => m,
            Err(err) => {
                eprintln!("[plugin] 解析 {} 失败: {err}", path.display());
                continue;
            }
        };
        reg.plugins.insert(m.name.clone(), m);
    }
    reg
}

pub fn install(json: &str) -> Result<String, String> {
    let m: PluginManifest = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let dir = plugin_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}.json", m.name));
    let pretty = serde_json::to_string_pretty(&m).map_err(|e| e.to_string())?;
    std::fs::write(&path, pretty).map_err(|e| e.to_string())?;
    Ok(m.name)
}

pub fn remove(name: &str) -> Result<(), String> {
    let path = plugin_dir().join(format!("{name}.json"));
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn list_hooks(reg: &PluginRegistry) -> Vec<(String, Hook)> {
    let mut out = Vec::new();
    for (plugin_name, m) in &reg.plugins {
        for h in &m.hooks {
            out.push((plugin_name.clone(), h.clone()));
        }
    }
    out
}

/// 执行插件步骤链
pub fn run_steps(steps: &[PluginStep], input: &str) -> String {
    let mut text = input.to_string();
    for step in steps {
        text = apply_step(step, &text);
    }
    text
}

fn apply_step(step: &PluginStep, text: &str) -> String {
    match step {
        PluginStep::Trim => text.trim().to_string(),
        PluginStep::Lower => text.to_lowercase(),
        PluginStep::Upper => text.to_uppercase(),
        PluginStep::Append { arg } => format!("{text}{arg}"),
        PluginStep::Prepend { arg } => format!("{arg}{text}"),
        PluginStep::Replace { arg } => {
            // arg 形如 "find::with" — 用 :: 作为分隔（避免 escape 麻烦）
            if let Some(idx) = arg.find("::") {
                let find = &arg[..idx];
                let with = &arg[idx + 2..];
                text.replace(find, with)
            } else {
                text.to_string()
            }
        }
        PluginStep::Truncate { arg } => {
            if let Ok(n) = arg.parse::<usize>() {
                if text.chars().count() > n {
                    let t: String = text.chars().take(n).collect();
                    format!("{t}…")
                } else {
                    text.to_string()
                }
            } else {
                text.to_string()
            }
        }
        PluginStep::Wrap { arg } => format!("{arg}{text}{arg}"),
        PluginStep::Template { arg } => arg.replace("$INPUT", text),
        PluginStep::ToChecklist => text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| format!("- [ ] {l}"))
            .collect::<Vec<_>>()
            .join("\n"),
        PluginStep::ToBullets => text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| format!("- {l}"))
            .collect::<Vec<_>>()
            .join("\n"),
        PluginStep::Repeat { arg } => {
            if let Ok(n) = arg.parse::<usize>() {
                text.repeat(n.max(1))
            } else {
                text.to_string()
            }
        }
        PluginStep::Reverse => text.chars().rev().collect(),
        PluginStep::Meta { .. } => text.to_string(), // 不修改 text
    }
}

/// 调 hook（按 kind + command）
pub fn invoke(reg: &PluginRegistry, kind: HookKind, command: Option<&str>, input: &str) -> String {
    let mut text = input.to_string();
    for (pname, h) in list_hooks(reg) {
        if h.kind != kind {
            continue;
        }
        if kind == HookKind::Slash && h.command.as_deref() != command {
            continue;
        }
        let _ = pname;
        if let Some(script) = &h.script {
            text = script.replace("$INPUT", &text);
        }
        if !h.steps.is_empty() {
            text = run_steps(&h.steps, &text);
        }
    }
    text
}

/// 默认 5 个示例插件（v1.5 开箱即用）
pub fn default_manifests() -> Vec<PluginManifest> {
    vec![
        PluginManifest {
            name: "uppercase".into(),
            version: "1.0".into(),
            description: "把所有 user 消息转大写".into(),
            author: Some("Codex gx".into()),
            tags: vec!["text".into(), "transform".into()],
            hooks: vec![Hook {
                kind: HookKind::PreSend,
                command: None,
                description: Some("Pre-send uppercase".into()),
                script: None,
                steps: vec![PluginStep::Upper],
            }],
        },
        PluginManifest {
            name: "checklist".into(),
            version: "1.0".into(),
            description: "把 assistant 回答转成 markdown checklist".into(),
            author: Some("Codex gx".into()),
            tags: vec!["text".into()],
            hooks: vec![Hook {
                kind: HookKind::PostRecv,
                command: None,
                description: Some("Wrap response as checklist".into()),
                script: None,
                steps: vec![PluginStep::ToChecklist],
            }],
        },
        PluginManifest {
            name: "summarize-200".into(),
            version: "1.0".into(),
            description: "把回答截断到 200 字符".into(),
            author: Some("Codex gx".into()),
            tags: vec!["text".into(), "length".into()],
            hooks: vec![Hook {
                kind: HookKind::PostRecv,
                command: None,
                description: Some("Truncate to 200 chars".into()),
                script: None,
                steps: vec![PluginStep::Truncate { arg: "200".into() }],
            }],
        },
        PluginManifest {
            name: "wrap-think".into(),
            version: "1.0".into(),
            description: "把 user 消息包到 <think> 标签中".into(),
            author: Some("Codex gx".into()),
            tags: vec!["text".into(), "agent".into()],
            hooks: vec![Hook {
                kind: HookKind::PreSend,
                command: None,
                description: Some("Wrap input in <think>".into()),
                script: None,
                steps: vec![
                    PluginStep::Wrap {
                        arg: "<think>\n".into(),
                    },
                    PluginStep::Append {
                        arg: "\n</think>".into(),
                    },
                ],
            }],
        },
        PluginManifest {
            name: "shout".into(),
            version: "1.0".into(),
            description: "把回答加 5 个感叹号".into(),
            author: Some("Codex gx".into()),
            tags: vec!["fun".into()],
            hooks: vec![Hook {
                kind: HookKind::PostRecv,
                command: None,
                description: Some("Add 5 exclamation marks".into()),
                script: None,
                steps: vec![PluginStep::Append {
                    arg: "!!!!!".into(),
                }],
            }],
        },
    ]
}

/// 安装默认插件（v1.5 启动时调用一次）
pub fn install_defaults() -> Result<Vec<String>, String> {
    let mut installed = Vec::new();
    for m in default_manifests() {
        let json = serde_json::to_string_pretty(&m).map_err(|e| e.to_string())?;
        install(&json)?;
        installed.push(m.name);
    }
    Ok(installed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_trim() {
        let s = vec![PluginStep::Trim];
        assert_eq!(run_steps(&s, "  hello  "), "hello");
    }

    #[test]
    fn step_upper() {
        let s = vec![PluginStep::Upper];
        assert_eq!(run_steps(&s, "abc"), "ABC");
    }

    #[test]
    fn step_chain() {
        let s = vec![PluginStep::Trim, PluginStep::Upper];
        assert_eq!(run_steps(&s, "  hi  "), "HI");
    }

    #[test]
    fn step_replace() {
        let s = vec![PluginStep::Replace {
            arg: "foo::bar".into(),
        }];
        assert_eq!(run_steps(&s, "foofoo"), "barbar");
    }

    #[test]
    fn step_truncate() {
        let s = vec![PluginStep::Truncate { arg: "5".into() }];
        assert_eq!(run_steps(&s, "abcdefghij").chars().count(), 6); // 5 + …
    }

    #[test]
    fn step_template() {
        let s = vec![PluginStep::Template {
            arg: "请用一句话总结：$INPUT".into(),
        }];
        assert_eq!(run_steps(&s, "这是一段话"), "请用一句话总结：这是一段话");
    }

    #[test]
    fn step_to_checklist() {
        let s = vec![PluginStep::ToChecklist];
        let r = run_steps(&s, "a\nb\n\nc");
        assert!(r.contains("- [ ] a"));
        assert!(r.contains("- [ ] b"));
        assert!(r.contains("- [ ] c"));
        assert!(!r.contains("- [ ] \n"));
    }

    #[test]
    fn step_to_bullets() {
        let s = vec![PluginStep::ToBullets];
        let r = run_steps(&s, "x\ny");
        assert!(r.contains("- x"));
        assert!(r.contains("- y"));
    }

    #[test]
    fn step_repeat() {
        let s = vec![PluginStep::Repeat { arg: "3".into() }];
        assert_eq!(run_steps(&s, "ab"), "ababab");
    }

    #[test]
    fn step_reverse() {
        let s = vec![PluginStep::Reverse];
        assert_eq!(run_steps(&s, "abc"), "cba");
    }

    #[test]
    fn step_wrap() {
        let s = vec![PluginStep::Wrap { arg: "**".into() }];
        assert_eq!(run_steps(&s, "bold"), "**bold**");
    }

    #[test]
    fn step_append_prepend() {
        let s = vec![
            PluginStep::Prepend { arg: ">>".into() },
            PluginStep::Append { arg: "<<".into() },
        ];
        assert_eq!(run_steps(&s, "x"), ">>x<<");
    }

    #[test]
    fn step_meta_noop() {
        let s = vec![PluginStep::Meta {
            arg: "key=val".into(),
        }];
        assert_eq!(run_steps(&s, "x"), "x");
    }

    #[test]
    fn invoke_pre_send() {
        let mut reg = PluginRegistry::new();
        let m = PluginManifest {
            name: "u".into(),
            version: "1".into(),
            description: "".into(),
            author: None,
            tags: vec![],
            hooks: vec![Hook {
                kind: HookKind::PreSend,
                command: None,
                description: None,
                script: None,
                steps: vec![PluginStep::Upper],
            }],
        };
        reg.plugins.insert("u".into(), m);
        let r = invoke(&reg, HookKind::PreSend, None, "hi");
        assert_eq!(r, "HI");
    }

    #[test]
    fn invoke_post_recv() {
        let mut reg = PluginRegistry::new();
        let m = PluginManifest {
            name: "s".into(),
            version: "1".into(),
            description: "".into(),
            author: None,
            tags: vec![],
            hooks: vec![Hook {
                kind: HookKind::PostRecv,
                command: None,
                description: None,
                script: None,
                steps: vec![PluginStep::Reverse],
            }],
        };
        reg.plugins.insert("s".into(), m);
        let r = invoke(&reg, HookKind::PostRecv, None, "abc");
        assert_eq!(r, "cba");
    }

    #[test]
    fn invoke_slash_filter() {
        let mut reg = PluginRegistry::new();
        let m = PluginManifest {
            name: "todo".into(),
            version: "1".into(),
            description: "".into(),
            author: None,
            tags: vec![],
            hooks: vec![Hook {
                kind: HookKind::Slash,
                command: Some("todo".into()),
                description: None,
                script: Some("📝 $INPUT".into()),
                steps: vec![],
            }],
        };
        reg.plugins.insert("todo".into(), m);
        let r = invoke(&reg, HookKind::Slash, Some("todo"), "买菜");
        assert_eq!(r, "📝 买菜");
        let r2 = invoke(&reg, HookKind::Slash, Some("other"), "买菜");
        assert_eq!(r2, "买菜"); // no match
    }

    #[test]
    fn install_then_remove() {
        let m = PluginManifest {
            name: "test_install".into(),
            version: "1".into(),
            description: "".into(),
            author: None,
            tags: vec![],
            hooks: vec![],
        };
        let json = serde_json::to_string(&m).unwrap();
        let name = install(&json).unwrap();
        assert_eq!(name, "test_install");
        let reg = load_registry();
        assert!(reg.plugins.contains_key("test_install"));
        remove("test_install").unwrap();
        let reg2 = load_registry();
        assert!(!reg2.plugins.contains_key("test_install"));
    }

    #[test]
    fn default_manifests_count() {
        let m = default_manifests();
        assert!(m.len() >= 5);
        let names: Vec<&str> = m.iter().map(|x| x.name.as_str()).collect();
        assert!(names.contains(&"uppercase"));
        assert!(names.contains(&"checklist"));
    }
}
