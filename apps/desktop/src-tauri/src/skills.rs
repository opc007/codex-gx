//! v0.8：Skill 系统 — 用户自定义 slash 命令
//!
//! 从 ~/.agentshell/skills.json 加载：
//! ```json
//! {
//!   "skills": [
//!     { "name": "commit", "description": "Git 提交", "shell": "git add -A && git commit -m \"$ARG\"" },
//!     { "name": "deploy", "description": "部署", "shell": "./deploy.sh $ARG" }
//!   ]
//! }
//! ```
//! 调用时 `/commit <msg>` 会执行 `git add -A && git commit -m "<msg>"`

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub shell: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillsFile {
    #[serde(default)]
    pub skills: Vec<Skill>,
}

/// 加载 skill 文件
pub fn load_skills() -> SkillsFile {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let path = PathBuf::from(home)
        .join(".agentshell")
        .join("skills.json");
    if !path.exists() {
        return SkillsFile::default();
    }
    match std::fs::read_to_string(&path) {
        Ok(data) => serde_json::from_str(&data).unwrap_or_else(|e| {
            eprintln!("[skills] 解析失败: {}", e);
            SkillsFile::default()
        }),
        Err(e) => {
            eprintln!("[skills] 读取失败: {}", e);
            SkillsFile::default()
        }
    }
}

/// 写入 skill 文件
pub fn save_skills(file: &SkillsFile) -> std::io::Result<()> {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let dir = PathBuf::from(home).join(".agentshell");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("skills.json");
    let json = serde_json::to_string_pretty(file).map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::Other, e)
    })?;
    std::fs::write(path, json)
}

/// 在 skills 中按名字查找
pub fn find_skill<'a>(skills: &'a SkillsFile, name: &str) -> Option<&'a Skill> {
    skills.skills.iter().find(|s| s.name == name)
}

/// 执行 skill（替换 $ARG 占位符）
pub fn execute_skill(skill: &Skill, arg: &str) -> Result<String, String> {
    let cmd = skill.shell.replace("$ARG", arg);
    // 用 sh -c 执行（支持 && | 等）
    let output = if cfg!(target_os = "windows") {
        Command::new("cmd").args(["/C", &cmd]).output()
    } else {
        Command::new("sh").args(["-c", &cmd]).output()
    };
    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout).to_string();
            let stderr = String::from_utf8_lossy(&o.stderr).to_string();
            let code = o.status.code().unwrap_or(-1);
            let mut result = format!("🔧 skill `{}` exit={}\n", skill.name, code);
            if !stdout.is_empty() {
                result.push_str(&format!("--- stdout ---\n{}\n", stdout));
            }
            if !stderr.is_empty() {
                result.push_str(&format!("--- stderr ---\n{}\n", stderr));
            }
            if code != 0 {
                return Err(result);
            }
            Ok(result)
        }
        Err(e) => Err(format!("执行失败: {}", e)),
    }
}

/// 把 skill 转成 slash command 列表（前端用）
pub fn to_command_map(skills: &SkillsFile) -> HashMap<String, SkillInfo> {
    skills
        .skills
        .iter()
        .map(|s| {
            (
                format!("/{}", s.name),
                SkillInfo {
                    name: s.name.clone(),
                    description: s.description.clone(),
                },
            )
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_skill() {
        let mut f = SkillsFile::default();
        f.skills.push(Skill {
            name: "commit".into(),
            description: "git commit".into(),
            shell: "git add -A && git commit -m \"$ARG\"".into(),
        });
        assert!(find_skill(&f, "commit").is_some());
        assert!(find_skill(&f, "nonexistent").is_none());
    }

    #[test]
    fn test_execute_skill_echo() {
        let s = Skill {
            name: "echo".into(),
            description: "echo arg".into(),
            shell: "echo $ARG".into(),
        };
        let r = execute_skill(&s, "hello");
        assert!(r.is_ok());
        assert!(r.unwrap().contains("hello"));
    }
}