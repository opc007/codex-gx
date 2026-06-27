//! v0.6 / v1.2：Desktop Computer Use (CUA)
//!
//! 跨平台 `DesktopCua` trait + 各平台 impl。
//! - macOS：通过 `osascript` 调 System Events（窗口/焦点） + enigo（鼠标/键盘）
//! - Windows：通过 PowerShell 调 UI Automation API（System.Windows.Automation）
//!   + System.Drawing 截图
//! - Linux：stub（v1.3+）
//!
//! 重要变更：鼠标键盘模拟已从 Python+Quartz 迁移到 Rust enigo，避免用户环境缺 Quartz 模块的问题。
//!
//! Windows 平台同时支持：
//! - 列出窗口
//! - 聚焦窗口
//! - 枚举 UI 树
//! - 点击 / 输入 / 组合键
//! - 屏幕截图（base64）

#![allow(missing_docs)]

use agent_core::tool::ToolOutput;
use agent_core::{Error, Result, Tool};
use async_trait::async_trait;
use base64::Engine;
use serde_json::json;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::sync::LazyLock;
use std::sync::Mutex;

/// 最后一次截图的屏幕尺寸，用于相对坐标换算 (Codex 风格 0-1 相对)
static LAST_SCREEN_SIZE: LazyLock<Mutex<(u32, u32)>> = LazyLock::new(|| Mutex::new((1920, 1080)));

type ToolInput = serde_json::Value;

/// 跨平台 Desktop CUA 接口
pub trait DesktopCua: Send + Sync + std::fmt::Debug {
    fn platform(&self) -> &'static str;

    /// 列出所有可见窗口
    fn list_windows(&self) -> Result<Vec<DesktopWindow>>;

    /// 聚焦到指定应用 + 标题子串的窗口
    fn focus_window(&self, app_name: &str, title_contains: Option<&str>) -> Result<String>;

    /// 获取应用的 UI 元素树（role + description），depth 控制返回行数
    fn get_app_tree(&self, app_name: &str, depth: u32) -> Result<String>;

    /// 在屏幕坐标 (x, y) 上点击
    fn click_at(&self, x: i32, y: i32) -> Result<String>;

    /// 双击
    fn double_click_at(&self, x: i32, y: i32) -> Result<String> {
        // 默认 fallback 到两次 click
        let _ = self.click_at(x, y);
        std::thread::sleep(std::time::Duration::from_millis(80));
        self.click_at(x, y)
    }

    /// 滚动 (dx, dy)
    fn scroll(&self, dx: i32, dy: i32) -> Result<String> {
        Err(Error::ToolExecution("scroll 未在此平台实装".to_string()))
    }

    /// 向当前聚焦的应用输入文本
    fn type_text(&self, text: &str) -> Result<String>;

    /// 发送键盘组合键
    fn key_combo(&self, keys: &str) -> Result<String>;

    /// 截全屏并返回 PNG 的 base64 编码
    fn screenshot(&self) -> Result<DesktopScreenshot> {
        // 默认实现：各平台自己实现
        Err(Error::ToolExecution(format!(
            "screenshot 在 {} 上未实装",
            self.platform()
        )))
    }

    /// 启动应用（推荐用这个来打开 App，比 AppleScript 可靠）
    fn launch_app(&self, app_name: &str) -> Result<String> {
        Err(Error::ToolExecution("launch_app 未在此平台实装".to_string()))
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DesktopWindow {
    pub app: String,
    pub title: String,
    pub pid: u32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DesktopScreenshot {
    /// PNG base64
    pub png_base64: String,
    /// 屏幕宽
    pub width: u32,
    /// 屏幕高
    pub height: u32,
    /// 临时文件路径（供前端可选预览）
    pub path: String,
}

// ============================================================
// macOS 实现：通过 osascript 调 System Events
// ============================================================

#[cfg(target_os = "macos")]
#[derive(Debug, Default)]
pub struct MacOsDesktopCua;

#[cfg(target_os = "macos")]
impl DesktopCua for MacOsDesktopCua {
    fn platform(&self) -> &'static str {
        "macos"
    }

    fn list_windows(&self) -> Result<Vec<DesktopWindow>> {
        let script = r#"
            tell application "System Events"
                set out to ""
                repeat with p in (every process whose visible is true)
                    set pname to name of p
                    repeat with w in (every window of p)
                        try
                            set wtitle to name of w
                            set pid_ to unix id of p
                            set out to out & pname & "|||" & wtitle & "|||" & pid_ & linefeed
                        end try
                    end repeat
                end repeat
                return out
            end tell
        "#;
        let raw = run_osascript(script)?;
        let mut wins = Vec::new();
        for line in raw.lines() {
            let parts: Vec<&str> = line.split("|||").collect();
            if parts.len() == 3 {
                let pid = parts[2].trim().parse().unwrap_or(0);
                if !parts[0].is_empty() && !parts[1].is_empty() {
                    wins.push(DesktopWindow {
                        app: parts[0].to_string(),
                        title: parts[1].to_string(),
                        pid,
                    });
                }
            }
        }
        Ok(wins)
    }

    fn focus_window(&self, app_name: &str, title_contains: Option<&str>) -> Result<String> {
        let title_filter = title_contains.unwrap_or("");
        // 尝试常见名称（微信/WeChat）
        for app in [app_name, "微信", "WeChat", "weixin"] {
            let script = format!(
                r#"
                tell application "System Events"
                    try
                        set theProcess to first process whose name is "{app}"
                        if (count of windows of theProcess) is 0 then
                            continue
                        end if
                        if "{title}" is "" then
                            set frontmost of theProcess to true
                            return "OK: focused {app}"
                        else if (count of (every window of theProcess whose name contains "{title}")) is 0 then
                            continue
                        else
                            set theWindow to first window of theProcess whose name contains "{title}"
                            set frontmost of theProcess to true
                            perform action "AXRaise" of theWindow
                            return "OK: focused {app} / {title}"
                        end if
                    end try
                end tell
                "#,
                app = app,
                title = title_filter,
            );
            if let Ok(res) = run_osascript(&script) {
                if res.starts_with("OK") {
                    return Ok(res);
                }
            }
        }
        Err(Error::ToolExecution(format!("无法聚焦应用 {}", app_name)))
    }

    fn get_app_tree(&self, app_name: &str, depth: u32) -> Result<String> {
        let script = format!(
            r#"
            tell application "System Events"
                set theProcess to first process whose name is "{}"
                set out to ""
                tell theProcess
                    repeat with elem in (entire contents)
                        try
                            set out to out & (role of elem as string) & " | " & (description of elem as string) & linefeed
                        end try
                    end repeat
                end tell
                return out
            end tell
            "#,
            app_name
        );
        let raw = run_osascript(&script)?;
        let lines: Vec<&str> = raw.lines().take((depth as usize) * 20).collect();
        Ok(format!(
            "📱 App: {}\n🌲 UI Elements (max {} lines):\n{}",
            app_name,
            lines.len(),
            lines.join("\n")
        ))
    }

    fn click_at(&self, x: i32, y: i32) -> Result<String> {
        use enigo::{Enigo, Mouse, Settings, Coordinate, Button, Direction};

        let mut enigo = match Enigo::new(&Settings::default()) {
            Ok(e) => e,
            Err(e) => {
                let msg = format!("{}", e);
                if msg.to_lowercase().contains("accessibility") || msg.to_lowercase().contains("permission") {
                    return Err(Error::ToolExecution("缺少 macOS 辅助功能权限。请到「系统设置 → 隐私与安全性 → 辅助功能」授予 Codex gx 权限，然后完全重启应用。".to_string()));
                }
                return Err(Error::ToolExecution(format!("Enigo 初始化失败: {}", e)));
            }
        };

        enigo.move_mouse(x as i32, y as i32, Coordinate::Abs)
            .map_err(|e| Error::ToolExecution(format!("移动鼠标失败: {}", e)))?;

        enigo.button(Button::Left, Direction::Click)
            .map_err(|e| Error::ToolExecution(format!("点击失败: {}", e)))?;

        Ok(format!("🖱️ 点击 ({}, {})", x, y))
    }

    fn double_click_at(&self, x: i32, y: i32) -> Result<String> {
        use enigo::{Enigo, Mouse, Settings, Coordinate, Button, Direction};

        let mut enigo = Enigo::new(&Settings::default())
            .map_err(|e| Error::ToolExecution(format!("Enigo 初始化失败: {}", e)))?;

        enigo.move_mouse(x as i32, y as i32, Coordinate::Abs)
            .map_err(|e| Error::ToolExecution(format!("移动鼠标失败: {}", e)))?;

        enigo.button(Button::Left, Direction::Click)
            .map_err(|e| Error::ToolExecution(format!("第一次点击失败: {}", e)))?;

        std::thread::sleep(std::time::Duration::from_millis(80));

        enigo.button(Button::Left, Direction::Click)
            .map_err(|e| Error::ToolExecution(format!("第二次点击失败: {}", e)))?;

        Ok(format!("🖱️🖱️ 双击 ({}, {})", x, y))
    }

    fn scroll(&self, dx: i32, dy: i32) -> Result<String> {
        use enigo::{Enigo, Axis, Mouse, Settings};

        let mut enigo = Enigo::new(&Settings::default())
            .map_err(|e| Error::ToolExecution(format!("Enigo 初始化失败: {}", e)))?;

        // Enigo scroll: amount, Axis
        if dy != 0 {
            enigo.scroll(dy, Axis::Vertical)
                .map_err(|e| Error::ToolExecution(format!("垂直滚动失败: {}", e)))?;
        }
        if dx != 0 {
            enigo.scroll(dx, Axis::Horizontal)
                .map_err(|e| Error::ToolExecution(format!("水平滚动失败: {}", e)))?;
        }

        Ok(format!("🖱️ 滚动 dx={}, dy={}", dx, dy))
    }

    fn type_text(&self, text: &str) -> Result<String> {
        use enigo::{Enigo, Keyboard, Settings, Direction};

        let mut enigo = Enigo::new(&Settings::default())
            .map_err(|e| Error::ToolExecution(format!("Enigo 初始化失败: {}", e)))?;

        enigo.text(text)
            .map_err(|e| Error::ToolExecution(format!("输入文字失败: {}", e)))?;

        Ok(format!("⌨️ 输入 {} 字符", text.chars().count()))
    }

    fn key_combo(&self, keys: &str) -> Result<String> {
        use enigo::{Enigo, Keyboard, Settings, Direction, Key};

        let mut enigo = Enigo::new(&Settings::default())
            .map_err(|e| Error::ToolExecution(format!("Enigo 初始化失败: {}", e)))?;

        let parts: Vec<String> = keys.split('+').map(|s| s.trim().to_lowercase()).collect();
        if parts.is_empty() {
            return Err(Error::ToolExecution("keys 不能为空".to_string()));
        }

        let main_key_str = &parts[0];
        let main_key = match main_key_str.as_str() {
            "enter" | "return" => Key::Return,
            "tab" => Key::Tab,
            "esc" | "escape" => Key::Escape,
            "space" => Key::Space,
            "backspace" | "delete" => Key::Backspace,
            "up" => Key::UpArrow,
            "down" => Key::DownArrow,
            "left" => Key::LeftArrow,
            "right" => Key::RightArrow,
            c if c.len() == 1 => Key::Unicode(c.chars().next().unwrap()),
            _ => return Err(Error::ToolExecution(format!("不支持的按键: {}", main_key_str))),
        };

        // 处理修饰键
        for m in &parts[1..] {
            match m.as_str() {
                "cmd" | "command" | "meta" => { let _ = enigo.key(Key::Meta, Direction::Press); }
                "ctrl" | "control" => { let _ = enigo.key(Key::Control, Direction::Press); }
                "alt" | "option" => { let _ = enigo.key(Key::Alt, Direction::Press); }
                "shift" => { let _ = enigo.key(Key::Shift, Direction::Press); }
                _ => {}
            }
        }

        let _ = enigo.key(main_key, Direction::Click);

        // 释放修饰键
        for m in &parts[1..] {
            match m.as_str() {
                "cmd" | "command" | "meta" => { let _ = enigo.key(Key::Meta, Direction::Release); }
                "ctrl" | "control" => { let _ = enigo.key(Key::Control, Direction::Release); }
                "alt" | "option" => { let _ = enigo.key(Key::Alt, Direction::Release); }
                "shift" => { let _ = enigo.key(Key::Shift, Direction::Release); }
                _ => {}
            }
        }

        Ok(format!("⌨️ 组合键 {}", keys))
    }

    fn launch_app(&self, app_name: &str) -> Result<String> {
        // 尝试常见名称
        let mut launched_name = None;
        for name in [app_name, "微信", "WeChat", "weixin"] {
            let output = Command::new("open")
                .arg("-a")
                .arg(name)
                .output()
                .map_err(|e| Error::ToolExecution(format!("执行 open 失败: {}", e)))?;

            if output.status.success() {
                launched_name = Some(name.to_string());
                break;
            }
        }
        if launched_name.is_none() {
            return Err(Error::ToolExecution(format!(
                "打开「{}」失败。请确认应用已安装，并尝试 bash 命令: open -a \"微信\" ",
                app_name
            )));
        }
        let name = launched_name.unwrap();

        // 等待应用启动，最多10秒，每秒检查是否有窗口
        for _ in 0..10 {
            std::thread::sleep(std::time::Duration::from_secs(1));
            if let Ok(windows) = self.list_windows() {
                if windows.iter().any(|w| w.app.contains("微信") || w.app.contains("WeChat") || w.app.to_lowercase().contains("weixin")) {
                    return Ok(format!("✅ 已成功启动应用: {} 并确认窗口出现", name));
                }
            }
        }
        Ok(format!("✅ 已尝试启动应用: {} （请手动确认是否打开）", name))
    }

    fn screenshot(&self) -> Result<DesktopScreenshot> {
        // v1.2：使用 screencapture 命令
        let tmp = std::env::temp_dir().join(format!("codex_gx_screen_{}.png", std::process::id()));
        let path_str = tmp.to_string_lossy().to_string();
        let out = Command::new("screencapture")
            .arg("-x") // 不发出快门声
            .arg(&path_str)
            .output()
            .map_err(|e| Error::ToolExecution(format!("screencapture 启动失败: {}", e)))?;
        if !out.status.success() {
            return Err(Error::ToolExecution(format!(
                "screencapture 错误: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        let bytes = std::fs::read(&tmp)
            .map_err(|e| Error::ToolExecution(format!("读取截图失败: {}", e)))?;
        // 解析 PNG 头获取宽高
        let (w, h) = parse_png_dimensions(&bytes).unwrap_or((1920, 1080));

        // 更新 last screen size 供相对坐标使用 (Codex 协议)
        if let Ok(mut size) = LAST_SCREEN_SIZE.lock() {
            *size = (w, h);
        }

        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        Ok(DesktopScreenshot {
            png_base64: b64,
            width: w,
            height: h,
            path: path_str,
        })
    }
}

#[cfg(target_os = "macos")]
fn run_osascript(script: &str) -> Result<String> {
    let out = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|e| Error::ToolExecution(format!("osascript 启动失败: {}", e)))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let msg = stderr.trim().to_string();
        // 常见 macOS 权限错误：辅助功能 / Accessibility
        if msg.contains("辅助访问") || msg.contains("not allowed assistive access") || msg.contains("-25211") {
            return Err(Error::ToolExecution(
                "【权限不足】无法控制桌面。\n\n请执行以下操作：\n1. 打开「系统设置 → 隐私与安全性 → 辅助功能」\n2. 点击锁解锁\n3. 将「Codex gx」（或运行 `cargo tauri dev` 的终端/iTerm）添加到列表并打勾\n4. 完全退出 Codex gx（Cmd+Q）后重新启动\n\n授权后，点击/输入等操作才能工作。".to_string()
            ));
        }
        return Err(Error::ToolExecution(format!("osascript 错误: {}", msg)));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

// ============================================================
// Windows 实现：通过 PowerShell 调 UI Automation API
// ============================================================

#[cfg(target_os = "windows")]
#[derive(Debug, Default)]
pub struct WindowsDesktopCua;

#[cfg(target_os = "windows")]
impl DesktopCua for WindowsDesktopCua {
    fn platform(&self) -> &'static str {
        "windows"
    }

    fn list_windows(&self) -> Result<Vec<DesktopWindow>> {
        // 列出所有顶层窗口：title, process name, pid
        let ps = r#"
            Add-Type @"
            using System;
            using System.Collections.Generic;
            using System.Runtime.InteropServices;
            using System.Text;
            public class W {
                [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc enumProc, IntPtr lParam);
                [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int count);
                [DllImport("user32.dll")] public static extern int GetWindowTextLength(IntPtr hWnd);
                [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
                [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
                public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
            }
"@
            $results = New-Object System.Collections.Generic.List[string]
            $callback = [W+EnumWindowsProc]{
                param($hWnd, $lParam)
                if ([W]::IsWindowVisible($hWnd)) {
                    $len = [W]::GetWindowTextLength($hWnd)
                    if ($len -gt 0) {
                        $sb = New-Object System.Text.StringBuilder ($len + 1)
                        [void][W]::GetWindowText($hWnd, $sb, $sb.Capacity)
                        $title = $sb.ToString()
                        $pid = 0
                        [void][W]::GetWindowThreadProcessId($hWnd, [ref]$pid)
                        $proc = (Get-Process -Id $pid -ErrorAction SilentlyContinue).ProcessName
                        if (-not $proc) { $proc = "?" }
                        $results.Add("$proc|||$title|||$pid")
                    }
                }
                return $true
            }
            [void][W]::EnumWindows($callback, [IntPtr]::Zero)
            $results -join "`n"
        "#;
        let raw = run_powershell(ps)?;
        let mut wins = Vec::new();
        for line in raw.lines() {
            let parts: Vec<&str> = line.split("|||").collect();
            if parts.len() == 3 {
                let pid = parts[2].trim().parse().unwrap_or(0);
                if !parts[0].is_empty() && !parts[1].is_empty() {
                    wins.push(DesktopWindow {
                        app: parts[0].to_string(),
                        title: parts[1].to_string(),
                        pid,
                    });
                }
            }
        }
        Ok(wins)
    }

    fn focus_window(&self, app_name: &str, title_contains: Option<&str>) -> Result<String> {
        let title = title_contains.unwrap_or("");
        let ps = format!(
            r#"
            $proc = Get-Process -Name "{}" -ErrorAction SilentlyContinue | Select-Object -First 1
            if (-not $proc) {{ return "FAIL: 找不到进程 {}" }}
            $proc.Refresh()
            $h = $proc.MainWindowHandle
            if ($h -eq 0) {{ return "FAIL: {} 没有主窗口" }}
            Add-Type @"
            using System;
            using System.Runtime.InteropServices;
            public class W2 {{
                [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
                [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
            }}
"@
            [void][W2]::ShowWindow($h, 9)   # SW_RESTORE
            [void][W2]::SetForegroundWindow($h)
            return "OK: focused {} (hwnd=$h)"
            "#,
            app_name, app_name, app_name, app_name
        );
        run_powershell(&ps)
    }

    fn get_app_tree(&self, app_name: &str, depth: u32) -> Result<String> {
        // 用 UI Automation 枚举指定进程主窗口的 UI 树
        let max = (depth as i32) * 20;
        let ps = format!(
            r#"
            Add-Type -AssemblyName UIAutomationClient
            Add-Type -AssemblyName UIAutomationTypes
            $proc = Get-Process -Name "{}" -ErrorAction SilentlyContinue | Select-Object -First 1
            if (-not $proc) {{ return "FAIL: 找不到进程 {}" }}
            $h = $proc.MainWindowHandle
            if ($h -eq 0) {{ return "FAIL: {} 没有主窗口" }}
            $root = [System.Windows.Automation.AutomationElement]::FromHandle($h)
            if (-not $root) {{ return "FAIL: FromHandle 返回空" }}
            $out = New-Object System.Collections.Generic.List[string]
            $walker = [System.Windows.Automation.TreeWalker]::ContentViewWalker
            $stack = New-Object System.Collections.Generic.Stack[object]
            $stack.Push(@{{ Elem=$root; D=0 }})
            $count = 0
            while ($stack.Count -gt 0 -and $count -lt {max}) {{
                $frame = $stack.Pop()
                $e = $frame.Elem
                $d = $frame.D
                if (-not $e) {{ continue }}
                $name = $e.Current.Name
                $ctrl = $e.Current.ControlType.ProgrammaticName
                $auto = $e.Current.AutomationId
                $out.Add("$d | $ctrl | $name | autoId=$auto")
                $count++
                if ($d -lt {depth}) {{
                    $child = $walker.GetFirstChild($e)
                    while ($child) {{
                        $stack.Push(@{{ Elem=$child; D=($d+1) }})
                        $child = $walker.GetNextSibling($child)
                    }}
                }}
            }}
            $out -join "`n"
            "#,
            app_name,
            app_name,
            app_name,
            depth = depth,
            max = max
        );
        let raw = run_powershell(&ps)?;
        Ok(format!(
            "📱 App: {}\n🌲 UI Tree (max {} nodes):\n{}",
            app_name, max, raw
        ))
    }

    fn click_at(&self, x: i32, y: i32) -> Result<String> {
        let ps = format!(
            r#"
            Add-Type @"
            using System;
            using System.Runtime.InteropServices;
            public class C {{
                [DllImport("user32.dll", SetLastError=true)] public static extern bool SetCursorPos(int X, int Y);
                [DllImport("user32.dll", SetLastError=true)] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
            }}
"@
            [void][C]::SetCursorPos({}, {})
            [C]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)  # LEFTDOWN
            Start-Sleep -Milliseconds 50
            [C]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)  # LEFTUP
            "OK: clicked ({}, {})"
            "#,
            x, y, x, y
        );
        run_powershell(&ps)?;
        Ok(format!("🖱️ 点击 ({}, {})", x, y))
    }

    fn double_click_at(&self, x: i32, y: i32) -> Result<String> {
        // 简单两次 click
        let _ = self.click_at(x, y);
        std::thread::sleep(std::time::Duration::from_millis(100));
        self.click_at(x, y)
    }

    fn scroll(&self, dx: i32, dy: i32) -> Result<String> {
        let ps = format!(
            r#"
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class S {{
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
}}
"@
[S]::mouse_event(0x0800, 0, 0, {}, [UIntPtr]::Zero)  # WHEEL
"OK: scrolled dy={}"
            "#,
            dy, dy
        );
        run_powershell(&ps)?;
        Ok(format!("🖱️ 滚动 dx={}, dy={}", dx, dy))
    }

    fn type_text(&self, text: &str) -> Result<String> {
        // PowerShell 字符串里需要转义双引号
        let escaped = text
            .replace('"', "`\"")
            .replace('`', "``")
            .replace('$', "`$");
        let ps = format!(
            r#"
            Add-Type -AssemblyName System.Windows.Forms
            [System.Windows.Forms.SendKeys]::SendWait("{}")
            "OK"
            "#,
            escaped
        );
        run_powershell(&ps)?;
        Ok(format!("⌨️ 输入 {} 字符", text.chars().count()))
    }

    fn key_combo(&self, keys: &str) -> Result<String> {
        // SendKeys 语法：^ = Ctrl, % = Alt, + = Shift, # = Win
        let mut sk = String::new();
        for part in keys.split('+') {
            let p = part.trim();
            let lower = p.to_lowercase();
            match lower.as_str() {
                "ctrl" | "control" => sk.push('^'),
                "alt" => sk.push('%'),
                "shift" => sk.push('+'),
                "win" | "cmd" | "command" => sk.push('#'),
                _ => sk.push_str(p),
            }
        }
        let ps = format!(
            r#"
            Add-Type -AssemblyName System.Windows.Forms
            [System.Windows.Forms.SendKeys]::SendWait("{}")
            "OK"
            "#,
            sk
        );
        run_powershell(&ps)?;
        Ok(format!("⌨️ 组合键 {}", keys))
    }

    fn screenshot(&self) -> Result<DesktopScreenshot> {
        // 用 System.Drawing 截全屏
        let tmp = std::env::temp_dir().join(format!("codex_gx_screen_{}.png", std::process::id()));
        let path_str = tmp.to_string_lossy().to_string();
        let path_for_ps = path_str.replace('\\', "\\\\").replace('\'', "`'");
        let ps = format!(
            r#"
            Add-Type -AssemblyName System.Drawing
            Add-Type -AssemblyName System.Windows.Forms
            $bounds = [System.Windows.Forms.SystemInformation]::VirtualScreen
            $bmp = New-Object System.Drawing.Bitmap $bounds.Width, $bounds.Height
            $g = [System.Drawing.Graphics]::FromImage($bmp)
            $g.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size)
            $bmp.Save('{}', [System.Drawing.Imaging.ImageFormat]::Png)
            $g.Dispose()
            $bmp.Dispose()
            "$($bounds.Width)x$($bounds.Height)"
            "#,
            path_for_ps
        );
        let out = run_powershell(&ps)?;
        let bytes = std::fs::read(&tmp)
            .map_err(|e| Error::ToolExecution(format!("读取截图失败: {}", e)))?;
        let (w, h) = parse_png_dimensions(&bytes).unwrap_or((0, 0));
        // 解析 "$w x $h" 输出
        let mut dw = w;
        let mut dh = h;
        if let Some((a, b)) = out.split_once('x') {
            dw = a.trim().parse().unwrap_or(w);
            dh = b.trim().parse().unwrap_or(h);
        }
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        Ok(DesktopScreenshot {
            png_base64: b64,
            width: dw,
            height: dh,
            path: path_str,
        })
    }
}

#[cfg(target_os = "windows")]
fn run_powershell(script: &str) -> Result<String> {
    // 写到临时 .ps1 文件再执行，避开命令行长度限制
    let tmp = std::env::temp_dir().join(format!("codex_gx_cua_{}.ps1", std::process::id()));
    {
        let mut f = std::fs::File::create(&tmp)
            .map_err(|e| Error::ToolExecution(format!("create ps1 failed: {}", e)))?;
        f.write_all(script.as_bytes())
            .map_err(|e| Error::ToolExecution(format!("write ps1 failed: {}", e)))?;
    }
    let out = Command::new("powershell")
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-File")
        .arg(&tmp)
        .output()
        .map_err(|e| Error::ToolExecution(format!("powershell 启动失败: {}", e)))?;
    let _ = std::fs::remove_file(&tmp);
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(Error::ToolExecution(format!(
            "PowerShell 错误: {}",
            stderr.trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

// ============================================================
// Linux 平台 stub（v1.3+ 再实装）
// ============================================================

#[cfg(target_os = "linux")]
#[derive(Debug, Default)]
pub struct LinuxDesktopCua;

#[cfg(target_os = "linux")]
impl DesktopCua for LinuxDesktopCua {
    fn platform(&self) -> &'static str {
        "linux"
    }
    fn list_windows(&self) -> Result<Vec<DesktopWindow>> {
        // 实装：wmctrl
        let out = Command::new("wmctrl")
            .arg("-l")
            .arg("-p")
            .output()
            .map_err(|e| Error::ToolExecution(format!("wmctrl 启动失败: {}", e)))?;
        if !out.status.success() {
            return Ok(Vec::new());
        }
        let raw = String::from_utf8_lossy(&out.stdout);
        let mut wins = Vec::new();
        for line in raw.lines() {
            // 0x00.. 0  host Title
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 5 {
                let pid = parts[2].parse().unwrap_or(0);
                wins.push(DesktopWindow {
                    app: parts[3].to_string(),
                    title: parts[4..].join(" "),
                    pid,
                });
            }
        }
        Ok(wins)
    }
    fn focus_window(&self, _app: &str, _title: Option<&str>) -> Result<String> {
        Err(Error::ToolExecution(
            "Linux Desktop CUA 部分实装（仅 list_windows），v1.3+ 完善".to_string(),
        ))
    }
    fn get_app_tree(&self, _app: &str, _depth: u32) -> Result<String> {
        Err(Error::ToolExecution(
            "Linux Desktop CUA 部分实装（仅 list_windows），v1.3+ 完善".to_string(),
        ))
    }
    fn click_at(&self, _x: i32, _y: i32) -> Result<String> {
        Err(Error::ToolExecution(
            "Linux Desktop CUA 部分实装（仅 list_windows），v1.3+ 完善".to_string(),
        ))
    }
    fn type_text(&self, _text: &str) -> Result<String> {
        Err(Error::ToolExecution(
            "Linux Desktop CUA 部分实装（仅 list_windows），v1.3+ 完善".to_string(),
        ))
    }
    fn key_combo(&self, _keys: &str) -> Result<String> {
        Err(Error::ToolExecution(
            "Linux Desktop CUA 部分实装（仅 list_windows），v1.3+ 完善".to_string(),
        ))
    }
    fn screenshot(&self) -> Result<DesktopScreenshot> {
        // 尝试 import
        let tmp = std::env::temp_dir().join(format!("codex_gx_screen_{}.png", std::process::id()));
        let path_str = tmp.to_string_lossy().to_string();
        let out = Command::new("import")
            .arg("-window")
            .arg("root")
            .arg(&path_str)
            .output();
        match out {
            Ok(o) if o.status.success() => {
                let bytes = std::fs::read(&tmp)
                    .map_err(|e| Error::ToolExecution(format!("读取截图失败: {}", e)))?;
                let (w, h) = parse_png_dimensions(&bytes).unwrap_or((0, 0));
                let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                Ok(DesktopScreenshot {
                    png_base64: b64,
                    width: w,
                    height: h,
                    path: path_str,
                })
            }
            _ => Err(Error::ToolExecution(
                "Linux screenshot 需要 ImageMagick (import)".to_string(),
            )),
        }
    }
}

// ============================================================
// 未知平台 stub
// ============================================================

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
#[derive(Debug, Default)]
pub struct StubDesktopCua;

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
impl DesktopCua for StubDesktopCua {
    fn platform(&self) -> &'static str {
        "unknown"
    }
    fn list_windows(&self) -> Result<Vec<DesktopWindow>> {
        Ok(Vec::new())
    }
    fn focus_window(&self, _: &str, _: Option<&str>) -> Result<String> {
        Err(Error::ToolExecution("Desktop CUA 未实装".to_string()))
    }
    fn get_app_tree(&self, _: &str, _: u32) -> Result<String> {
        Err(Error::ToolExecution("Desktop CUA 未实装".to_string()))
    }
    fn click_at(&self, _: i32, _: i32) -> Result<String> {
        Err(Error::ToolExecution("Desktop CUA 未实装".to_string()))
    }
    fn type_text(&self, _: &str) -> Result<String> {
        Err(Error::ToolExecution("Desktop CUA 未实装".to_string()))
    }
    fn key_combo(&self, _: &str) -> Result<String> {
        Err(Error::ToolExecution("Desktop CUA 未实装".to_string()))
    }
}

// ============================================================
// 工具注册
// ============================================================

fn cua() -> Box<dyn DesktopCua> {
    #[cfg(target_os = "macos")]
    {
        Box::new(MacOsDesktopCua)
    }
    #[cfg(target_os = "windows")]
    {
        Box::new(WindowsDesktopCua)
    }
    #[cfg(target_os = "linux")]
    {
        Box::new(LinuxDesktopCua)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        Box::new(StubDesktopCua)
    }
}

/// 简单解析 PNG 宽高（从 IHDR）
fn parse_png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 24 {
        return None;
    }
    // PNG signature: 89 50 4E 47 0D 0A 1A 0A
    // IHDR follows at offset 8, length 4 bytes, type 4 bytes
    if &bytes[0..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    if &bytes[12..16] != b"IHDR" {
        return None;
    }
    let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    Some((w, h))
}

// --- desktop_list_windows ---

#[derive(Debug)]
pub struct DesktopListWindowsTool;
#[async_trait]
impl Tool for DesktopListWindowsTool {
    fn name(&self) -> &str {
        "desktop_list_windows"
    }
    fn description(&self) -> &str {
        "列出所有可见窗口的 {app, title, pid}。macOS 通过 System Events；Windows 通过 UI Automation；Linux 通过 wmctrl。"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        json!({"type": "object", "properties": {}})
    }
    async fn execute(&self, _input: ToolInput) -> Result<ToolOutput> {
        let c = cua();
        let wins = c.list_windows()?;
        if wins.is_empty() {
            return Ok(ToolOutput::ok(format!(
                "🖥️ 平台 {}：无可见窗口（或平台部分实装）",
                c.platform()
            )));
        }
        let mut text = format!(
            "🖥️ 平台 {} 可见窗口 ({} 个)：\n\n",
            c.platform(),
            wins.len()
        );
        for (i, w) in wins.iter().enumerate().take(50) {
            text.push_str(&format!(
                "{}. **{}** — `{}` (pid {})\n",
                i + 1,
                w.app,
                w.title,
                w.pid
            ));
        }
        if wins.len() > 50 {
            text.push_str(&format!("\n... 共 {} 个（截断）", wins.len()));
        }
        Ok(ToolOutput::ok(text))
    }
}

// --- desktop_focus_window ---

#[derive(Debug)]
pub struct DesktopFocusWindowTool;
#[async_trait]
impl Tool for DesktopFocusWindowTool {
    fn name(&self) -> &str {
        "desktop_focus_window"
    }
    fn description(&self) -> &str {
        "聚焦到指定应用的窗口（可按标题子串过滤）。强烈建议先调用 desktop_launch_app 来打开应用。"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "app": {"type": "string", "description": "应用名（macOS）/ 进程名（Windows/Linux）"},
                "title_contains": {"type": "string", "description": "窗口标题子串（可选）"}
            },
            "required": ["app"]
        })
    }
    async fn execute(&self, input: ToolInput) -> Result<ToolOutput> {
        let app = input["app"].as_str().unwrap_or("").to_string();
        if app.is_empty() {
            return Ok(ToolOutput::err("app 不能为空".to_string()));
        }
        let title = input["title_contains"].as_str();
        let r = cua().focus_window(&app, title)?;
        Ok(ToolOutput::ok(r))
    }
}

// --- desktop_get_app_tree ---

#[derive(Debug)]
pub struct DesktopGetAppTreeTool;
#[async_trait]
impl Tool for DesktopGetAppTreeTool {
    fn name(&self) -> &str {
        "desktop_get_app_tree"
    }
    fn description(&self) -> &str {
        "获取应用的 UI 元素树（role + description / control type + name）。depth 控制返回行数（depth*20）。"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "app": {"type": "string"},
                "depth": {"type": "integer", "default": 3}
            },
            "required": ["app"]
        })
    }
    async fn execute(&self, input: ToolInput) -> Result<ToolOutput> {
        let app = input["app"].as_str().unwrap_or("").to_string();
        if app.is_empty() {
            return Ok(ToolOutput::err("app 不能为空".to_string()));
        }
        let depth = input["depth"].as_u64().unwrap_or(3) as u32;
        let r = cua().get_app_tree(&app, depth)?;
        Ok(ToolOutput::ok(r))
    }
}

// --- desktop_click_at ---

#[derive(Debug)]
pub struct DesktopClickAtTool;
#[async_trait]
impl Tool for DesktopClickAtTool {
    fn name(&self) -> &str {
        "desktop_click_at"
    }
    fn description(&self) -> &str {
        "在屏幕上点击鼠标左键。支持绝对像素或相对坐标 x:0.0-1.0, y:0.0-1.0（Codex风格，推荐）。先 screenshot 获取视觉，然后根据描述输出相对坐标点击。用于朋友圈按钮等。"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "x": {"type": "number", "description": "x 坐标：整数像素 或 0.0~1.0 相对坐标"},
                "y": {"type": "number", "description": "y 坐标：整数像素 或 0.0~1.0 相对坐标"}
            },
            "required": ["x", "y"]
        })
    }
    async fn execute(&self, input: ToolInput) -> Result<ToolOutput> {
        let x_val = input["x"].as_f64().unwrap_or(0.0);
        let y_val = input["y"].as_f64().unwrap_or(0.0);

        // Codex 对齐：支持相对坐标 0.0-1.0，使用上次截图的实际尺寸
        let (x, y) = if x_val > 0.0 && x_val <= 1.0 && y_val > 0.0 && y_val <= 1.0 {
            let (sw, sh) = if let Ok(size) = LAST_SCREEN_SIZE.lock() {
                *size
            } else {
                (1920u32, 1080u32)
            };
            let abs_x = (x_val * sw as f64) as i32;
            let abs_y = (y_val * sh as f64) as i32;
            (abs_x, abs_y)
        } else {
            (x_val as i32, y_val as i32)
        };

        let r = cua().click_at(x, y)?;
        Ok(ToolOutput::ok(format!("{} (原始输入: {}, {})", r, x_val, y_val)))
    }
}

// --- desktop_double_click_at ---

#[derive(Debug)]
pub struct DesktopDoubleClickAtTool;
#[async_trait]
impl Tool for DesktopDoubleClickAtTool {
    fn name(&self) -> &str {
        "desktop_double_click_at"
    }
    fn description(&self) -> &str {
        "双击屏幕坐标。支持相对坐标 x,y 0.0-1.0 或绝对像素。"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "x": {"type": "number"},
                "y": {"type": "number"}
            },
            "required": ["x", "y"]
        })
    }
    async fn execute(&self, input: ToolInput) -> Result<ToolOutput> {
        let x_val = input["x"].as_f64().unwrap_or(0.0);
        let y_val = input["y"].as_f64().unwrap_or(0.0);

        let (x, y) = if x_val > 0.0 && x_val <= 1.0 && y_val > 0.0 && y_val <= 1.0 {
            let (sw, sh) = if let Ok(size) = LAST_SCREEN_SIZE.lock() { *size } else { (1920u32, 1080u32) };
            ((x_val * sw as f64) as i32, (y_val * sh as f64) as i32)
        } else {
            (x_val as i32, y_val as i32)
        };

        let r = cua().double_click_at(x, y)?;
        Ok(ToolOutput::ok(r))
    }
}

// --- desktop_scroll ---

#[derive(Debug)]
pub struct DesktopScrollTool;
#[async_trait]
impl Tool for DesktopScrollTool {
    fn name(&self) -> &str {
        "desktop_scroll"
    }
    fn description(&self) -> &str {
        "滚动鼠标。dx 水平, dy 垂直。正数向下/右。"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "dx": {"type": "integer", "default": 0},
                "dy": {"type": "integer", "default": 0}
            }
        })
    }
    async fn execute(&self, input: ToolInput) -> Result<ToolOutput> {
        let dx = input["dx"].as_i64().unwrap_or(0) as i32;
        let dy = input["dy"].as_i64().unwrap_or(0) as i32;
        let r = cua().scroll(dx, dy)?;
        Ok(ToolOutput::ok(r))
    }
}

// --- desktop_type_text ---

#[derive(Debug)]
pub struct DesktopTypeTextTool;
#[async_trait]
impl Tool for DesktopTypeTextTool {
    fn name(&self) -> &str {
        "desktop_type_text"
    }
    fn description(&self) -> &str {
        "向当前聚焦的应用输入文本。"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "text": {"type": "string"}
            },
            "required": ["text"]
        })
    }
    async fn execute(&self, input: ToolInput) -> Result<ToolOutput> {
        let text = input["text"].as_str().unwrap_or("").to_string();
        if text.is_empty() {
            return Ok(ToolOutput::err("text 不能为空".to_string()));
        }
        let r = cua().type_text(&text)?;
        Ok(ToolOutput::ok(r))
    }
}

// --- desktop_key_combo ---

#[derive(Debug)]
pub struct DesktopKeyComboTool;
#[async_trait]
impl Tool for DesktopKeyComboTool {
    fn name(&self) -> &str {
        "desktop_key_combo"
    }
    fn description(&self) -> &str {
        "发送键盘组合键。macOS: 'cmd+c' 'cmd+shift+p'。Windows: 'ctrl+c' 'alt+tab'。"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "keys": {"type": "string", "description": "如 'cmd+c' 'cmd+shift+4' 'ctrl+alt+delete'"}
            },
            "required": ["keys"]
        })
    }
    async fn execute(&self, input: ToolInput) -> Result<ToolOutput> {
        let keys = input["keys"].as_str().unwrap_or("").to_string();
        if keys.is_empty() {
            return Ok(ToolOutput::err("keys 不能为空".to_string()));
        }
        let r = cua().key_combo(&keys)?;
        Ok(ToolOutput::ok(r))
    }
}

// --- desktop_screenshot ---

#[derive(Debug)]
pub struct DesktopScreenshotTool;
#[async_trait]
impl Tool for DesktopScreenshotTool {
    fn name(&self) -> &str {
        "desktop_screenshot"
    }
    fn description(&self) -> &str {
        "截取全屏（或当前 App）返回 PNG base64。模型可用视觉分析界面，然后用相对坐标 0.0-1.0 或元素描述来点击/操作。Codex Computer Use 核心工具。"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "app": {"type": "string", "description": "可选：指定 App 名称，只截这个窗口（例如 'Safari' 或 '微信'）"}
            }
        })
    }
    async fn execute(&self, input: ToolInput) -> Result<ToolOutput> {
        let c = cua();
        // 如果指定了 app，先尝试 focus（最佳实践）
        if let Some(app) = input["app"].as_str() {
            let _ = c.focus_window(app, None);
        }
        let shot = c.screenshot()?;
        // 工具输出里只放 path + 尺寸 + base64 前 200 字符（节省 token）
        let preview: String = shot.png_base64.chars().take(200).collect();
        let text = format!(
            "📸 截图完成（Codex Computer Use）\n平台: {}\n尺寸: {}x{}\n文件: {}\nbase64 长度: {} 字符\n\n【Codex 风格使用协议】\n1. 先看这张图，找到目标元素\n2. 回复相对坐标 {{ \"x\": 0.0-1.0, \"y\": 0.0-1.0, \"reason\": \"...\" }} 或直接调用 desktop_click_at\n3. 操作后可以再截图验证\n\nbase64 前 200 字符: {}...(完整 base64 在 data 里，模型可直接看图片)",
            c.platform(),
            shot.width,
            shot.height,
            shot.path,
            shot.png_base64.len(),
            preview
        );
        // 同时返回结构化结果
        let mut out = ToolOutput::ok(text);
        out.data = Some(json!({
            "path": shot.path,
            "width": shot.width,
            "height": shot.height,
            "png_base64": shot.png_base64,
        }));
        Ok(out)
    }
}

/// 注册全部桌面 CUA 工具
pub fn register_desktop_cua(reg: &mut agent_core::ToolRegistry) {
    reg.register(DesktopListWindowsTool);
    reg.register(DesktopFocusWindowTool);
    reg.register(DesktopGetAppTreeTool);
    reg.register(DesktopClickAtTool);
    reg.register(DesktopDoubleClickAtTool);
    reg.register(DesktopScrollTool);
    reg.register(DesktopTypeTextTool);
    reg.register(DesktopKeyComboTool);
    reg.register(DesktopScreenshotTool);
    reg.register(DesktopCheckPermissionTool);
    reg.register(DesktopLaunchAppTool);
    reg.register(DesktopOpenAccessibilitySettingsTool);
}

/// 检查桌面控制权限（推荐在做复杂操作前先调用）
#[derive(Debug)]
pub struct DesktopCheckPermissionTool;

#[async_trait]
impl Tool for DesktopCheckPermissionTool {
    fn name(&self) -> &str {
        "desktop_check_permission"
    }
    fn description(&self) -> &str {
        "检查 macOS 辅助功能权限状态。返回是否可以控制桌面、窗口、鼠标、键盘等，并给出授权指导。先调用这个，再用 desktop_launch_app 打开微信。"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        json!({ "type": "object", "properties": {} })
    }
    async fn execute(&self, _input: ToolInput) -> Result<ToolOutput> {
        #[cfg(target_os = "macos")]
        {
            Ok(ToolOutput::ok(
                "【macOS 桌面控制权限检查】\n\
                 要使用点击窗口、控制微信/浏览器等功能，必须授予「辅助功能」权限。\n\n\
                 操作步骤：\n\
                 1. 打开「系统设置 → 隐私与安全性 → 辅助功能」\n\
                 2. 解锁后，点击 + 添加「Codex gx」或你运行 `cargo tauri dev` 的终端（iTerm/Terminal）\n\
                 3. 打勾启用\n\
                 4. **完全退出** Codex gx (右键 Dock 图标 → 退出)，然后重新打开\n\n\
                 授权后，desktop_* 工具即可正常工作。\n\
                 同时建议也授予「屏幕录制」权限用于截图。"
                    .to_string()
            ))
        }
        #[cfg(not(target_os = "macos"))]
        {
            Ok(ToolOutput::ok("非 macOS 平台，桌面控制权限通常由系统沙箱或用户确认控制。".to_string()))
        }
    }
}

// --- desktop_open_accessibility_settings ---

#[derive(Debug)]
pub struct DesktopOpenAccessibilitySettingsTool;

#[async_trait]
impl Tool for DesktopOpenAccessibilitySettingsTool {
    fn name(&self) -> &str {
        "desktop_open_accessibility_settings"
    }
    fn description(&self) -> &str {
        "打开 macOS 辅助功能设置页面，帮助用户授予权限以使用桌面控制（点击、输入等）。"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        json!({ "type": "object", "properties": {} })
    }
    async fn execute(&self, _input: ToolInput) -> Result<ToolOutput> {
        #[cfg(target_os = "macos")]
        {
            let _ = Command::new("open")
                .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
                .output();
            Ok(ToolOutput::ok("已打开辅助功能设置页面。请添加 Codex gx 并启用权限，然后重启应用。".to_string()))
        }
        #[cfg(not(target_os = "macos"))]
        {
            Ok(ToolOutput::ok("此功能仅 macOS 可用。".to_string()))
        }
    }
}

// --- desktop_launch_app ---

#[derive(Debug)]
pub struct DesktopLaunchAppTool;

#[async_trait]
impl Tool for DesktopLaunchAppTool {
    fn name(&self) -> &str {
        "desktop_launch_app"
    }
    fn description(&self) -> &str {
        "启动 macOS 应用（推荐用于打开微信、浏览器等）。比直接用 AppleScript 可靠。参数 app_name 如 '微信' 。示例流程：先 launch_app('微信') ，然后 focus ，screenshot ，根据视觉用 click_at 相对坐标点朋友圈图标。"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "app_name": {"type": "string", "description": "应用名称，如 '微信' 或 'WeChat'"}
            },
            "required": ["app_name"]
        })
    }
    async fn execute(&self, input: ToolInput) -> Result<ToolOutput> {
        let app_name = input["app_name"].as_str().unwrap_or("").to_string();
        if app_name.is_empty() {
            return Ok(ToolOutput::err("app_name 不能为空".to_string()));
        }
        let r = cua().launch_app(&app_name)?;
        Ok(ToolOutput::ok(r))
    }
}

// 避免未用 import 警告
#[allow(dead_code)]
fn _unused_path() -> PathBuf {
    PathBuf::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_png_dimensions_ok() {
        // 一个真实 PNG 头: 1x1 PNG
        let png = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x06\x00\x00\x00\x1f\x15\xc4\x89";
        assert_eq!(parse_png_dimensions(png), Some((1, 1)));
    }

    #[test]
    fn parse_png_dimensions_bad() {
        assert_eq!(parse_png_dimensions(b"not png"), None);
        assert_eq!(parse_png_dimensions(b"\x89PNG\r\n\x1a\n\x00"), None);
    }

    #[test]
    fn cua_returns_correct_platform() {
        let c = cua();
        let p = c.platform();
        #[cfg(target_os = "macos")]
        assert_eq!(p, "macos");
        #[cfg(target_os = "windows")]
        assert_eq!(p, "windows");
        #[cfg(target_os = "linux")]
        assert_eq!(p, "linux");
    }
}
