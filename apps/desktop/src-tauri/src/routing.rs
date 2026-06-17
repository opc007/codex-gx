//! v1.3：LLM Provider 路由策略
//!
//! 旧 v0.7 路由是硬编码的关键词匹配。
//! v1.3 引入「策略」概念：
//! - 一组有序规则
//! - 每条规则 = 触发条件 + 目标 (provider, model)
//! - 触发条件支持：
//!   - 任务类型（task_type）：code / reason / summary / translate / chat / vision / long / quick
//!   - 关键词匹配（中文 / 英文 / 任意）
//!   - 文件类型（用户消息里包含文件名后缀）
//! - 兜底链（fallback chain）：当主 model 不可用时，依次尝试
//! - 用户可编辑（前端 UI），存到 `~/.agentshell/routing.json`
//!
//! 决策 API：
//!   - `RoutingEngine::decide(message, task_hint) -> Decision`
//!   - Decision { primary, fallbacks, reason }

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TaskType {
    Code,
    Reason,
    Summary,
    Translate,
    Chat,
    Vision,
    Long,
    Quick,
    /// 兜底
    Generic,
}

impl TaskType {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskType::Code => "code",
            TaskType::Reason => "reason",
            TaskType::Summary => "summary",
            TaskType::Translate => "translate",
            TaskType::Chat => "chat",
            TaskType::Vision => "vision",
            TaskType::Long => "long",
            TaskType::Quick => "quick",
            TaskType::Generic => "generic",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingRule {
    pub id: String,
    pub name: String,
    /// 优先级（数字越小越先匹配）
    pub priority: u32,
    /// 触发条件
    pub match_condition: MatchCondition,
    /// 主目标（provider, model）
    pub primary: RouteTarget,
    /// 兜底链
    #[serde(default)]
    pub fallbacks: Vec<RouteTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchCondition {
    /// 任务类型（可选，列表）
    #[serde(default)]
    pub task_types: Vec<TaskType>,
    /// 关键词（任意匹配，OR 语义）
    #[serde(default)]
    pub keywords: Vec<String>,
    /// 文件后缀（包含 . 后缀；OR 语义）
    #[serde(default)]
    pub file_exts: Vec<String>,
    /// 消息最小长度（消息长度 >= 阈值才触发）
    #[serde(default)]
    pub min_length: Option<usize>,
    /// 消息最大长度
    #[serde(default)]
    pub max_length: Option<usize>,
}

impl MatchCondition {
    pub fn matches(&self, msg: &str, task_hint: Option<&TaskType>) -> bool {
        // 语义：task_types / keywords / file_exts 任意一个命中即视为通过（OR）
        // 长度限制总是 AND（如果设了）

        // 任务类型
        let mut hit = false;
        if !self.task_types.is_empty() {
            if let Some(t) = task_hint {
                if self.task_types.contains(t) {
                    hit = true;
                }
            }
            // 如果 task_types 设了但没 hint 且其他条件也没命中 → 不通过
        }
        // 关键词
        if !hit && !self.keywords.is_empty() {
            let lower = msg.to_lowercase();
            if self
                .keywords
                .iter()
                .any(|k| lower.contains(&k.to_lowercase()))
            {
                hit = true;
            }
        }
        // 文件后缀
        if !hit && !self.file_exts.is_empty() {
            let lower = msg.to_lowercase();
            if self
                .file_exts
                .iter()
                .any(|e| lower.contains(&format!(".{}", e.to_lowercase())))
            {
                hit = true;
            }
        }
        // 如果三类条件都设了但都没命中 → 不通过
        let any_condition_set =
            !self.task_types.is_empty() || !self.keywords.is_empty() || !self.file_exts.is_empty();
        if any_condition_set && !hit {
            return false;
        }
        // 长度（AND）
        let len = msg.chars().count();
        if let Some(min) = self.min_length {
            if len < min {
                return false;
            }
        }
        if let Some(max) = self.max_length {
            if len > max {
                return false;
            }
        }
        true
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteTarget {
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingStrategy {
    pub version: u32,
    /// 默认主目标（兜底最后）
    pub default: RouteTarget,
    #[serde(default)]
    pub default_fallbacks: Vec<RouteTarget>,
    /// 规则列表
    pub rules: Vec<RoutingRule>,
}

impl RoutingStrategy {
    pub fn builtin() -> Self {
        RoutingStrategy {
            version: 1,
            default: RouteTarget {
                provider: "MiniMax".to_string(),
                model: "MiniMax-M3".to_string(),
            },
            default_fallbacks: vec![
                RouteTarget {
                    provider: "deepseek".to_string(),
                    model: "deepseek-chat".to_string(),
                },
                RouteTarget {
                    provider: "anthropic".to_string(),
                    model: "claude-sonnet-4-5".to_string(),
                },
            ],
            rules: vec![
                RoutingRule {
                    id: "code-deepseek".to_string(),
                    name: "代码 → DeepSeek".to_string(),
                    priority: 10,
                    match_condition: MatchCondition {
                        task_types: vec![TaskType::Code],
                        keywords: vec![
                            "code".to_string(),
                            "function".to_string(),
                            "fn ".to_string(),
                            "impl ".to_string(),
                            "bug".to_string(),
                            "debug".to_string(),
                            "error".to_string(),
                            "rust".to_string(),
                            "python".to_string(),
                            "javascript".to_string(),
                            "typescript".to_string(),
                            "compile".to_string(),
                            "refactor".to_string(),
                            "重构".to_string(),
                            "编译".to_string(),
                            "报错".to_string(),
                            "代码".to_string(),
                            "函数".to_string(),
                        ],
                        file_exts: vec![
                            "rs".to_string(),
                            "py".to_string(),
                            "js".to_string(),
                            "ts".to_string(),
                            "tsx".to_string(),
                            "jsx".to_string(),
                            "go".to_string(),
                            "java".to_string(),
                            "c".to_string(),
                            "cpp".to_string(),
                        ],
                        min_length: None,
                        max_length: None,
                    },
                    primary: RouteTarget {
                        provider: "deepseek".to_string(),
                        model: "deepseek-chat".to_string(),
                    },
                    fallbacks: vec![RouteTarget {
                        provider: "anthropic".to_string(),
                        model: "claude-sonnet-4-5".to_string(),
                    }],
                },
                RoutingRule {
                    id: "reason-claude".to_string(),
                    name: "推理 → Claude".to_string(),
                    priority: 20,
                    match_condition: MatchCondition {
                        task_types: vec![TaskType::Reason, TaskType::Long],
                        keywords: vec![
                            "plan".to_string(),
                            "分析".to_string(),
                            "规划".to_string(),
                            "策略".to_string(),
                            "compare".to_string(),
                            "tradeoff".to_string(),
                            "复杂".to_string(),
                            "深度".to_string(),
                            "reasoning".to_string(),
                        ],
                        file_exts: vec![],
                        min_length: Some(200),
                        max_length: None,
                    },
                    primary: RouteTarget {
                        provider: "anthropic".to_string(),
                        model: "claude-sonnet-4-5".to_string(),
                    },
                    fallbacks: vec![RouteTarget {
                        provider: "MiniMax".to_string(),
                        model: "MiniMax-M3".to_string(),
                    }],
                },
                RoutingRule {
                    id: "summary-quick".to_string(),
                    name: "摘要/短答 → 快速模型".to_string(),
                    priority: 30,
                    match_condition: MatchCondition {
                        task_types: vec![TaskType::Summary, TaskType::Quick],
                        keywords: vec![
                            "总结".to_string(),
                            "摘要".to_string(),
                            "summary".to_string(),
                            "summarize".to_string(),
                            "简短".to_string(),
                            "一句话".to_string(),
                        ],
                        file_exts: vec![],
                        min_length: None,
                        max_length: Some(80),
                    },
                    primary: RouteTarget {
                        provider: "deepseek".to_string(),
                        model: "deepseek-chat".to_string(),
                    },
                    fallbacks: vec![],
                },
                RoutingRule {
                    id: "vision-gpt".to_string(),
                    name: "视觉 → GPT-4o".to_string(),
                    priority: 5,
                    match_condition: MatchCondition {
                        task_types: vec![TaskType::Vision],
                        keywords: vec![],
                        file_exts: vec![
                            "png".to_string(),
                            "jpg".to_string(),
                            "jpeg".to_string(),
                            "gif".to_string(),
                            "webp".to_string(),
                        ],
                        min_length: None,
                        max_length: None,
                    },
                    primary: RouteTarget {
                        provider: "openai".to_string(),
                        model: "gpt-4o".to_string(),
                    },
                    fallbacks: vec![RouteTarget {
                        provider: "anthropic".to_string(),
                        model: "claude-sonnet-4-5".to_string(),
                    }],
                },
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub primary: RouteTarget,
    pub fallbacks: Vec<RouteTarget>,
    pub reason: String,
    pub rule_id: Option<String>,
}

pub struct RoutingEngine {
    strategy: RoutingStrategy,
    path: PathBuf,
}

impl RoutingEngine {
    pub fn load_or_default() -> Self {
        let path = routing_file_path();
        let strategy = match std::fs::read_to_string(&path) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_else(|_| RoutingStrategy::builtin()),
            Err(_) => RoutingStrategy::builtin(),
        };
        RoutingEngine { strategy, path }
    }

    pub fn strategy(&self) -> &RoutingStrategy {
        &self.strategy
    }

    pub fn set_strategy(&mut self, s: RoutingStrategy) {
        self.strategy = s;
    }

    pub fn save(&self) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let json = serde_json::to_string_pretty(&self.strategy)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(&self.path, json)
    }

    pub fn decide(&self, message: &str, task_hint: Option<&TaskType>) -> Decision {
        // 按 priority 升序
        let mut rules: Vec<&RoutingRule> = self.strategy.rules.iter().collect();
        rules.sort_by_key(|r| r.priority);

        for rule in rules {
            if rule.match_condition.matches(message, task_hint) {
                return Decision {
                    primary: rule.primary.clone(),
                    fallbacks: rule.fallbacks.clone(),
                    reason: format!("命中规则「{}」(priority={})", rule.name, rule.priority),
                    rule_id: Some(rule.id.clone()),
                };
            }
        }
        Decision {
            primary: self.strategy.default.clone(),
            fallbacks: self.strategy.default_fallbacks.clone(),
            reason: "无匹配规则，使用默认".to_string(),
            rule_id: None,
        }
    }
}

fn routing_file_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home)
        .join(".agentshell")
        .join("routing.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_strategy_loaded() {
        let s = RoutingStrategy::builtin();
        assert!(!s.rules.is_empty());
        assert_eq!(s.default.model, "MiniMax-M3");
    }

    #[test]
    fn match_condition_keywords() {
        let m = MatchCondition {
            task_types: vec![],
            keywords: vec!["hello".to_string(), "你好".to_string()],
            file_exts: vec![],
            min_length: None,
            max_length: None,
        };
        assert!(m.matches("Hello world", None));
        assert!(m.matches("你好，世界", None));
        assert!(!m.matches("rust code", None));
    }

    #[test]
    fn match_condition_file_exts() {
        let m = MatchCondition {
            task_types: vec![],
            keywords: vec![],
            file_exts: vec!["rs".to_string(), "py".to_string()],
            min_length: None,
            max_length: None,
        };
        assert!(m.matches("请修复 src/main.rs 的 bug", None));
        assert!(m.matches("check foo.py", None));
        assert!(!m.matches("hello world", None));
    }

    #[test]
    fn match_condition_length() {
        let m = MatchCondition {
            task_types: vec![],
            keywords: vec![],
            file_exts: vec![],
            min_length: Some(10),
            max_length: Some(20),
        };
        assert!(m.matches("0123456789", None));
        assert!(m.matches("0123456789abcdefghij", None));
        assert!(!m.matches("short", None));
        assert!(!m.matches("0123456789012345678901234567890", None));
    }

    #[test]
    fn match_condition_task_types() {
        let m = MatchCondition {
            task_types: vec![TaskType::Code],
            keywords: vec![],
            file_exts: vec![],
            min_length: None,
            max_length: None,
        };
        assert!(m.matches("anything", Some(&TaskType::Code)));
        assert!(!m.matches("anything", Some(&TaskType::Chat)));
        assert!(!m.matches("anything", None));
    }

    #[test]
    fn engine_decide_code() {
        let e = RoutingEngine::load_or_default();
        let d = e.decide("请帮我写一个 Rust 函数来解析 JSON", Some(&TaskType::Code));
        assert_eq!(d.primary.provider, "deepseek");
        assert!(d.reason.contains("代码"));
    }

    #[test]
    fn engine_decide_default() {
        let e = RoutingEngine::load_or_default();
        let d = e.decide("Hi there", None);
        assert_eq!(d.primary.model, "MiniMax-M3");
        assert!(d.reason.contains("默认"));
    }

    #[test]
    fn engine_decide_priority_order() {
        let s = RoutingStrategy {
            version: 1,
            default: RouteTarget {
                provider: "x".to_string(),
                model: "y".to_string(),
            },
            default_fallbacks: vec![],
            rules: vec![
                RoutingRule {
                    id: "low-priority".to_string(),
                    name: "low".to_string(),
                    priority: 100,
                    match_condition: MatchCondition {
                        task_types: vec![],
                        keywords: vec!["hello".to_string()],
                        file_exts: vec![],
                        min_length: None,
                        max_length: None,
                    },
                    primary: RouteTarget {
                        provider: "p1".to_string(),
                        model: "m1".to_string(),
                    },
                    fallbacks: vec![],
                },
                RoutingRule {
                    id: "high-priority".to_string(),
                    name: "high".to_string(),
                    priority: 1,
                    match_condition: MatchCondition {
                        task_types: vec![],
                        keywords: vec!["hello".to_string()],
                        file_exts: vec![],
                        min_length: None,
                        max_length: None,
                    },
                    primary: RouteTarget {
                        provider: "p2".to_string(),
                        model: "m2".to_string(),
                    },
                    fallbacks: vec![],
                },
            ],
        };
        let mut e = RoutingEngine::load_or_default();
        e.set_strategy(s);
        let d = e.decide("hello world", None);
        assert_eq!(d.primary.model, "m2");
        assert_eq!(d.rule_id, Some("high-priority".to_string()));
    }
}