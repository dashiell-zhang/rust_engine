mod settings;

use settings::DeploymentSettings;
use std::{
    collections::BTreeMap,
    env,
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
};
use uuid::Uuid;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() {
    if let Err(error) = generate() {
        eprintln!("生成部署配置失败：{error}");
        std::process::exit(1);
    }
}

/// 只生成部署文件，不启动应用、不安装服务，也不连接部署目标。
fn generate() -> Result<()> {
    if env::args_os().len() != 1 {
        return Err("生成器不接受启动参数，请修改 deployment/deploysettings.json".into());
    }
    let directory = find_generator_directory()?;
    // 锁文件保留在原位，以免并发生成器分别锁住不同的文件对象。
    let generation_lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(directory.join(".generation.lock"))?;
    generation_lock
        .try_lock()
        .map_err(|error| format!("无法独占生成目录，可能已有生成任务运行：{error}"))?;

    let content = fs::read_to_string(directory.join("deploysettings.json"))?;
    let settings: DeploymentSettings =
        serde_json::from_str(content.trim_start_matches('\u{feff}'))?;
    settings.validate()?;
    let values = settings.template_values()?;
    let name = settings.project_name.to_ascii_lowercase();
    let files = [
        ("nginx/nginx.conf", "nginx/nginx.conf".to_string()),
        (
            "nginx/admin-webapi.conf",
            format!("nginx/{name}-admin-webapi.conf"),
        ),
        (
            "nginx/client-webapi.conf",
            format!("nginx/{name}-client-webapi.conf"),
        ),
        (
            "service/admin-webapi.service",
            format!("service/{name}-admin-webapi.service"),
        ),
        (
            "service/client-webapi.service",
            format!("service/{name}-client-webapi.service"),
        ),
        (
            "service/task-service.service",
            format!("service/{name}-task-service.service"),
        ),
        (
            "service/install-services.txt",
            format!("service/{name}-install-services.txt"),
        ),
        (
            "service/runtime.env.example",
            format!("service/{name}-runtime.env.example"),
        ),
        (
            "yunxiao/admin-webapi.yaml",
            format!("yunxiao/{name}-admin-webapi.yaml"),
        ),
        (
            "yunxiao/client-webapi.yaml",
            format!("yunxiao/{name}-client-webapi.yaml"),
        ),
        (
            "yunxiao/task-service.yaml",
            format!("yunxiao/{name}-task-service.yaml"),
        ),
    ];
    let rendered = files
        .iter()
        .map(|(template, output)| {
            let source = directory
                .join("templates")
                .join(format!("{template}.template"));
            let content = fs::read_to_string(&source)
                .map_err(|error| format!("读取模板 {} 失败：{error}", source.display()))?;
            Ok((output.as_str(), render(&content, &values, template)?))
        })
        .collect::<Result<Vec<_>>>()?;
    replace_generated(&directory, &rendered)?;
    for (output, _) in rendered {
        println!("已生成 generated/{output}");
    }
    println!("部署配置已生成到 {}", directory.join("generated").display());
    Ok(())
}

fn find_generator_directory() -> Result<PathBuf> {
    let current = env::current_dir()?;
    let executable = env::current_exe()?;
    for start in [
        &current,
        executable.parent().ok_or("无法定位生成器可执行文件")?,
    ] {
        for parent in start.ancestors() {
            for candidate in [parent.to_path_buf(), parent.join("deployment")] {
                if candidate.join("deploysettings.json").is_file()
                    && candidate.join("templates").is_dir()
                {
                    return Ok(candidate.canonicalize()?);
                }
            }
        }
    }
    Err("未找到包含 deploysettings.json 和 templates 的 deployment 目录".into())
}

fn render(content: &str, values: &BTreeMap<String, String>, template: &str) -> Result<String> {
    let mut result = content.trim_start_matches('\u{feff}').replace("\r\n", "\n");
    for (key, value) in values {
        result = result.replace(&format!("{{{{{key}}}}}"), value);
    }
    if result.contains("{{") || result.contains("}}") {
        return Err(format!("模板 {template} 中存在未替换的占位符").into());
    }
    Ok(result)
}

/// 全部内容生成成功后才切换目录；切换失败时恢复旧结果。
fn replace_generated(directory: &Path, files: &[(&str, String)]) -> Result<()> {
    let suffix = Uuid::new_v4().simple().to_string();
    let staging = directory.join(format!(".generated-{suffix}"));
    let backup = directory.join(format!(".generated-backup-{suffix}"));
    let generated = directory.join("generated");
    let existed = match fs::symlink_metadata(&generated) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err("generated 必须是普通目录，不能是文件或符号链接".into());
        }
        Ok(_) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(error.into()),
    };

    fs::create_dir(&staging)?;
    let result = (|| -> Result<()> {
        for (relative, content) in files {
            let output = staging.join(relative);
            fs::create_dir_all(output.parent().ok_or("生成路径缺少父目录")?)?;
            fs::write(output, content)?;
        }
        if existed {
            fs::rename(&generated, &backup)?;
        }
        if let Err(replace_error) = fs::rename(&staging, &generated) {
            if existed && let Err(restore_error) = fs::rename(&backup, &generated) {
                return Err(format!(
                    "替换失败：{replace_error}；恢复失败：{restore_error}；原结果保留在 {}",
                    backup.display()
                )
                .into());
            }
            return Err(replace_error.into());
        }
        if existed {
            cleanup_temporary(directory, &backup);
        }
        Ok(())
    })();
    cleanup_temporary(directory, &staging);
    result
}

fn cleanup_temporary(directory: &Path, target: &Path) {
    // 仅删除当前生成器创建的同级临时目录，不递归清理任何配置输入路径。
    if target.parent() != Some(directory)
        || !target
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with(".generated-"))
    {
        return;
    }
    match fs::remove_dir_all(target) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => eprintln!("无法清理临时目录 {}：{error}", target.display()),
    }
}
