// 防止 Windows 链接 release 失败
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    agentshell_lib::run();
}
