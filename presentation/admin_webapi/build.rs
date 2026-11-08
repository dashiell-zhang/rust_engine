use std::{env, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = "../../InitData/AuthorizeInitData.json";
    println!("cargo:rerun-if-changed={source}");
    let output = PathBuf::from(env::var_os("OUT_DIR").ok_or("缺少 OUT_DIR")?);
    let profile_directory = output.ancestors().nth(3).ok_or("无法定位管理端输出目录")?;
    let directory = profile_directory.join("InitData");
    fs::create_dir_all(&directory)?;
    fs::copy(source, directory.join("AuthorizeInitData.json"))?;
    Ok(())
}
