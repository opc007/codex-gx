//! v0.6 / v1.2：Desktop Computer Use (CUA)
//!
//! 跨平台 `DesktopCua` trait + 各平台 impl。
//! - macOS：通过 `osascript` 调 System Events（AXUIElement）+ `screencapture`
//! - Windows：通过 PowerShell 调 UI Automation API（System.Windows.Automation）
//!   + System.Drawing 截图
//! - Linux：stub（v1.3+）
//!
//! Windows 平台同时支持：
//! - 列出窗口
//! - 聚焦窗口
//! - 枚举 UI 树
//! - 点击 / 输入 / 组合键
//! - 屏幕截图（base64）

use agent_core::tool::ToolOutput;
use agent_core::{Error, Result, Tool};
use async_trait::async_trait;
use base64::Engine;
use serde_json::json;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

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
        let script = format!(
            r#"
            tell application "System Events"
                set theProcess to first process whose name is "{app}"
                if (count of windows of theProcess) is 0 then
                    return "FAIL: {app} 没有任何窗口"
                end if
                if "{title}" is "" then
                    set frontmost of theProcess to true
                    return "OK: focused {app}"
                else if (count of (every window of theProcess whose name contains "{title}")) is 0 then
                    return "FAIL: {app} 没有标题含 '{title}' 的窗口"
                else
                    set theWindow to first window of theProcess whose name contains "{title}"
                    set frontmost of theProcess to true
                    perform action "AXRaise" of theWindow
                    return "OK: focused {app} / {title}"
                end if
            end tell
            "#,
            app = app_name,
            title = title_filter,
        );
        run_osascript(&script)
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
        let script = format!(
            r#"
            do shell script "/usr/bin/python3 -c 'import Quartz; e = Quartz.CGEventCreateMouseEvent(None, Quartz.kCGEventLeftMouseDown, ({}, {}), Quartz.kCGMouseButtonLeft); Quartz.CGEventPost(Quartz.kCGHIDEventTap, e); e2 = Quartz.CGEventCreateMouseEvent(None, Quartz.kCGEventLeftMouseUp, ({}, {}), Quartz.kCGMouseButtonLeft); Quartz.CGEventPost(Quartz.kCGHIDEventTap, e2)'"
            "#,
            x, y, x, y
        );
        run_osascript(&script)?;
        Ok(format!("🖱️ 点击 ({}, {})", x, y))
    }

    fn type_text(&self, text: &str) -> Result<String> {
        let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
        let script = format!(
            r#"tell application "System Events" to keystroke "{}""#,
            escaped
        );
        run_osascript(&script)?;
        Ok(format!("⌨️ 输入 {} 字符", text.chars().count()))
    }

    fn key_combo(&self, keys: &str) -> Result<String> {
        let parts: Vec<&str> = keys.split('+').map(|s| s.trim()).collect();
        if parts.is_empty() {
            return Err(Error::ToolExecution("keys 不能为空".to_string()));
        }
        let key = parts[0];
        let modifiers: Vec<&str> = parts[1..]
            .iter()
            .map(|s| match s.to_lowercase().as_str() {
                "cmd" | "command" => "command down",
                "ctrl" | "control" => "control down",
                "alt" | "option" => "option down",
                "shift" => "shift down",
                _ => "command down",
            })
            .collect();
        let using = if modifiers.is_empty() {
            String::new()
        } else {
            format!(" using {{ {} }}", modifiers.join(", "))
        };
        let script = format!(
            r#"tell application "System Events" to keystroke "{}"{}"#,
            key, using
        );
        run_osascript(&script)?;
        Ok(format!("⌨️ 组合键 {}", keys))
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
        let (w, h) = parse_png_dimensions(&bytes).unwrap_or((0, 0));
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
        return Err(Error::ToolExecution(format!(
            "osascript 错误: {}",
            stderr.trim()
        )));
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

    fn type_text(&self, text: &str) -> Result<String> {
        // PowerShell 字符串里需要转义双引号
        let escaped = text.replace('"', "`\"").replace('`', "``").replace('$', "`$");
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
        let mut text = format!("🖥️ 平台 {} 可见窗口 ({} 个)：\n\n", c.platform(), wins.len());
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
        "聚焦到指定应用的窗口（可按标题子串过滤）。"
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
        "在屏幕坐标 (x, y) 上点击鼠标左键。"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "x": {"type": "integer"},
                "y": {"type": "integer"}
            },
            "required": ["x", "y"]
        })
    }
    async fn execute(&self, input: ToolInput) -> Result<ToolOutput> {
        let x = input["x"].as_i64().unwrap_or(0) as i32;
        let y = input["y"].as_i64().unwrap_or(0) as i32;
        let r = cua().click_at(x, y)?;
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
        "截取全屏并返回 PNG base64 + 临时文件路径。模型可分析图像内容。"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        json!({"type": "object", "properties": {}})
    }
    async fn execute(&self, _input: ToolInput) -> Result<ToolOutput> {
        let c = cua();
        let shot = c.screenshot()?;
        // 工具输出里只放 path + 尺寸 + base64 前 200 字符（节省 token）
        let preview: String = shot.png_base64.chars().take(200).collect();
        let text = format!(
            "📸 截图完成\n平台: {}\n尺寸: {}x{}\n文件: {}\nbase64 长度: {} 字符\nbase64 前 200 字符: {}\n...(模型可读取多模态图片)",
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
    reg.register(DesktopTypeTextTool);
    reg.register(DesktopKeyComboTool);
    reg.register(DesktopScreenshotTool);
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