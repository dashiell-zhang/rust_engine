use std::{env, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=appsettings.toml");
    let output = PathBuf::from(env::var_os("OUT_DIR").ok_or("缺少 OUT_DIR")?);
    let profile_directory = output.ancestors().nth(3).ok_or("无法定位工具输出目录")?;
    fs::copy(
        "appsettings.toml",
        profile_directory.join("repository_tool.appsettings.toml"),
    )?;
    Ok(())
}
