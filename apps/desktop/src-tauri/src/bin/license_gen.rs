//! 内部工具：生成测试 license 激活码
//!
//! 用法（开发 / 自用）：
//! ```bash
//! cargo run --bin license-gen -- monthly
//! cargo run --bin license-gen -- quarterly
//! cargo run --bin license-gen -- yearly
//! cargo run --bin license-gen -- lifetime
//! ```
//!
//! **生产环境禁止使用本工具生成 license ！** 真实 license 由服务端签发。

use license::{ActivationCodeProvider, DeviceFingerprint, LicenseTier};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let tier_arg = args.get(1).map(|s| s.as_str()).unwrap_or("yearly");

    let tier = match tier_arg.to_lowercase().as_str() {
        "monthly" | "m" => LicenseTier::Monthly,
        "quarterly" | "q" => LicenseTier::Quarterly,
        "yearly" | "y" | "annual" => LicenseTier::Yearly,
        "lifetime" | "lt" | "l" => LicenseTier::Lifetime,
        other => {
            eprintln!("Unknown tier: {other}");
            eprintln!("Usage: license-gen <monthly|quarterly|yearly|lifetime>");
            std::process::exit(1);
        }
    };

    let device = DeviceFingerprint::current();
    let provider = ActivationCodeProvider::default_demo();
    let code = provider.generate_demo_code(tier, &device);
    let user_code = match code.to_user_code() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("encode failed: {e}");
            std::process::exit(1);
        }
    };

    println!("=== Codex gx 激活码 ===");
    println!("Tier:        {}", tier.display_name());
    println!("Device:      {} / {}", device.os, device.hostname);
    println!(
        "DiskSerial:  {}",
        device.disk_serial.as_deref().unwrap_or("<unknown>")
    );
    println!();
    println!(">>> 把下面整行粘到 Codex gx 的「输入激活码」框里 <<<");
    println!();
    println!("{}", user_code);
    println!();
    println!("⚠️  内部工具 — 仅用于开发 / 自测。生产 license 由服务端签发。");
}
