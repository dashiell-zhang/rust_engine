use std::{collections::HashSet, io, net::IpAddr, path::Path, process::Output, time::Duration};

#[cfg(windows)]
use std::{
    fs::{self, File},
    path::PathBuf,
};

#[cfg(windows)]
use base64::Engine as _;

use tokio::process::Command;

use crate::io_helper::TemporaryFileGuard;
#[cfg(windows)]
use crate::io_helper::replace_file;

/// Shell 默认执行超时时间
const DEFAULT_SHELL_TIMEOUT: Duration = Duration::from_secs(60);

/// 获取本机全部 IP
pub fn get_all_ip_address() -> Vec<String> {
    resolve_host_ips()
}

/// 获取本机 IPv4 地址
pub fn get_ipv4_address() -> Option<String> {
    find_preferred_ip(resolve_host_ips(), IpAddr::is_ipv4)
}

/// 获取本机 IPv6 地址
pub fn get_ipv6_address() -> Option<String> {
    find_preferred_ip(resolve_host_ips(), IpAddr::is_ipv6)
}

/// Linux 运行 Shell 脚本
///
/// 使用默认 60 秒超时，并在独立运行时线程中执行严格 Bash 脚本
pub fn linux_shell(shell: &str) -> io::Result<String> {
    run_shell_blocking(shell, ShellKind::Linux)
}

/// Windows 运行 Shell 脚本
///
/// 使用默认 60 秒超时，并要求 PowerShell 7.3 及以上版本
pub fn windows_shell(shell: &str) -> io::Result<String> {
    run_shell_blocking(shell, ShellKind::Windows)
}

/// 将 Word 文档转换为 PDF
///
/// 运行环境需要 Windows、PowerShell 7.3 及已安装的 Microsoft Word
pub fn word_to_pdf(word_path: impl AsRef<Path>, pdf_path: impl AsRef<Path>) -> io::Result<bool> {
    run_office_conversion_blocking(word_path.as_ref(), pdf_path.as_ref(), OfficeKind::Word)
}

/// 将 Excel 文档转换为 PDF
///
/// 运行环境需要 Windows、PowerShell 7.3 及已安装的 Microsoft Excel
pub fn excel_to_pdf(excel_path: impl AsRef<Path>, pdf_path: impl AsRef<Path>) -> io::Result<bool> {
    run_office_conversion_blocking(excel_path.as_ref(), pdf_path.as_ref(), OfficeKind::Excel)
}

/// 将 PowerPoint 文档转换为 PDF
///
/// 运行环境需要 Windows、PowerShell 7.3 及已安装的 Microsoft PowerPoint
pub fn ppt_to_pdf(ppt_path: impl AsRef<Path>, pdf_path: impl AsRef<Path>) -> io::Result<bool> {
    run_office_conversion_blocking(ppt_path.as_ref(), pdf_path.as_ref(), OfficeKind::PowerPoint)
}

/// 异步运行严格 Bash 脚本
///
/// Future 被取消或提前 Drop 时会尽力终止整个进程树
pub async fn linux_shell_with_timeout(shell: &str, timeout: Duration) -> io::Result<String> {
    validate_shell_timeout(timeout)?;

    let strict_shell = format!("shopt -s inherit_errexit\n{shell}");
    let arguments = [
        "--noprofile",
        "--norc",
        "-e",
        "-o",
        "pipefail",
        "-c",
        &strict_shell,
    ];
    let output = execute_shell("/bin/bash", &arguments, Some(timeout)).await?;
    ensure_command_success(output)
}

/// 异步运行严格 PowerShell 脚本
///
/// Future 被取消或提前 Drop 时会尽力终止整个进程树
pub async fn windows_shell_with_timeout(shell: &str, timeout: Duration) -> io::Result<String> {
    validate_shell_timeout(timeout)?;

    execute_windows_shell(shell, Some(timeout)).await
}

/// 异步执行严格 PowerShell 脚本
async fn execute_windows_shell(shell: &str, timeout: Option<Duration>) -> io::Result<String> {
    let temp_script_path =
        std::env::temp_dir().join(format!("rust_engine.{}.ps1", uuid::Uuid::now_v7().simple()));
    let temp_script_guard = TemporaryFileGuard::new(temp_script_path);
    std::fs::write(temp_script_guard.path(), shell.as_bytes())?;

    let encoded_script_path =
        powershell_single_quoted(temp_script_guard.path().to_str().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "PowerShell 临时路径无效")
        })?);
    let strict_shell = format!(
        "[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)\n\
         $OutputEncoding = [Console]::OutputEncoding\n\
         $ErrorActionPreference = 'Stop'\n\
         if ($PSVersionTable.PSVersion -lt [Version]'7.3') {{ throw '严格模式需要 PowerShell 7.3 及以上版本' }}\n\
         Set-StrictMode -Version Latest\n\
         $PSNativeCommandUseErrorActionPreference = $true\n\
         & '{encoded_script_path}'"
    );
    let arguments = [
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-STA",
        "-Command",
        strict_shell.as_str(),
    ];
    let output = execute_shell("pwsh", &arguments, timeout).await?;
    ensure_command_success(output)
}

/// Office 文档类型
#[derive(Clone, Copy)]
enum OfficeKind {
    /// Word 文档
    Word,

    /// Excel 文档
    Excel,

    /// PowerPoint 文档
    PowerPoint,
}

/// 在独立运行时线程中执行 Office 转换
fn run_office_conversion_blocking(
    source_path: &Path,
    pdf_path: &Path,
    office_kind: OfficeKind,
) -> io::Result<bool> {
    #[cfg(not(windows))]
    {
        let _ = (source_path, pdf_path, office_kind);
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Office 转 PDF 仅支持 Windows",
        ));
    }

    #[cfg(windows)]
    {
        let source_path = source_path.to_path_buf();
        let pdf_path = pdf_path.to_path_buf();
        std::thread::Builder::new()
            .name("common-office-converter".to_string())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()?;
                runtime.block_on(convert_office_to_pdf(&source_path, &pdf_path, office_kind))
            })?
            .join()
            .map_err(|_| io::Error::other("Office 转换线程异常退出"))?
    }
}

/// 执行 Office 转换并安全替换目标 PDF
#[cfg(windows)]
async fn convert_office_to_pdf(
    source_path: &Path,
    pdf_path: &Path,
    office_kind: OfficeKind,
) -> io::Result<bool> {
    let source_path = normalize_office_source_path(source_path)?;
    let output_path = normalize_pdf_output_path(pdf_path)?;
    if source_path == output_path {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "PDF 目标文件不能与源文件相同",
        ));
    }

    let temp_path = temporary_pdf_path(&output_path)?;
    let mut temp_guard = TemporaryFileGuard::new(temp_path);
    let source_text = powershell_path_base64(&source_path)?;
    let output_text = powershell_path_base64(temp_guard.path())?;
    let shell = office_kind.build_script(&source_text, &output_text);

    execute_windows_shell(&shell, None).await?;

    let metadata = match fs::metadata(temp_guard.path()) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    if !metadata.is_file() || metadata.len() == 0 {
        return Ok(false);
    }

    File::options()
        .read(true)
        .write(true)
        .open(temp_guard.path())?
        .sync_all()?;
    replace_file(temp_guard.path(), &output_path)?;
    temp_guard.preserve();
    Ok(true)
}

/// 规范化 Office 源文件路径
#[cfg(windows)]
fn normalize_office_source_path(source_path: &Path) -> io::Result<PathBuf> {
    let source_path = fs::canonicalize(source_path)?;
    if !fs::metadata(&source_path)?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Office 源路径不是普通文件",
        ));
    }

    Ok(source_path)
}

/// 规范化 PDF 输出路径
#[cfg(windows)]
fn normalize_pdf_output_path(pdf_path: &Path) -> io::Result<PathBuf> {
    let file_name = pdf_path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "PDF 目标文件名无效"))?;
    let parent = pdf_path
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let output_path = fs::canonicalize(parent)?.join(file_name);

    if let Ok(metadata) = fs::symlink_metadata(&output_path)
        && (!metadata.is_file() || metadata.file_type().is_symlink())
    {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "PDF 输出路径已存在且不是普通文件",
        ));
    }

    Ok(output_path)
}

/// 创建同目录 PDF 临时文件路径
#[cfg(windows)]
fn temporary_pdf_path(output_path: &Path) -> io::Result<PathBuf> {
    let file_stem = output_path
        .file_stem()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "PDF 目标文件名无效"))?;
    let mut file_name = file_stem.to_os_string();
    file_name.push(format!(".{}.pdf", uuid::Uuid::now_v7().simple()));
    Ok(output_path.with_file_name(file_name))
}

/// 将文件路径转换为 Base64 文本
#[cfg(windows)]
fn powershell_path_base64(path: &Path) -> io::Result<String> {
    path.to_str()
        .map(|value| base64::engine::general_purpose::STANDARD.encode(value))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Office 文件路径不是有效文本"))
}

#[cfg(windows)]
impl OfficeKind {
    /// 构建 Office COM 转换脚本
    fn build_script(self, source_path: &str, output_path: &str) -> String {
        let template = match self {
            Self::Word => WORD_TO_PDF_SCRIPT,
            Self::Excel => EXCEL_TO_PDF_SCRIPT,
            Self::PowerPoint => POWERPOINT_TO_PDF_SCRIPT,
        };

        template
            .replace("__SOURCE_PATH__", source_path)
            .replace("__OUTPUT_PATH__", output_path)
    }
}

/// Word 转 PDF PowerShell 脚本
#[cfg(windows)]
const WORD_TO_PDF_SCRIPT: &str = r#"
$File = [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String('__SOURCE_PATH__'))
$OutFile = [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String('__OUTPUT_PATH__'))
$Word = $null
$Documents = $null
$Doc = $null
try {
    $Word = New-Object -ComObject Word.Application
    $Documents = $Word.Documents
    $Doc = $Documents.Open($File)
    $Doc.ExportAsFixedFormat($OutFile, 17)
}
finally {
    if ($Doc -ne $null) {
        try { $Doc.Close($false) | Out-Null } catch {}
        try { [System.Runtime.InteropServices.Marshal]::ReleaseComObject($Doc) | Out-Null } catch {}
    }
    if ($Documents -ne $null) {
        try { [System.Runtime.InteropServices.Marshal]::ReleaseComObject($Documents) | Out-Null } catch {}
    }
    if ($Word -ne $null) {
        try { $Word.Quit() | Out-Null } catch {}
        try { [System.Runtime.InteropServices.Marshal]::ReleaseComObject($Word) | Out-Null } catch {}
    }
}
"#;

/// Excel 转 PDF PowerShell 脚本
#[cfg(windows)]
const EXCEL_TO_PDF_SCRIPT: &str = r#"
$File = [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String('__SOURCE_PATH__'))
$OutFile = [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String('__OUTPUT_PATH__'))
$Excel = $null
$Workbooks = $null
$Workbook = $null
try {
    $Excel = New-Object -ComObject Excel.Application
    $Workbooks = $Excel.Workbooks
    $Workbook = $Workbooks.Open($File)
    $Workbook.ExportAsFixedFormat(0, $OutFile)
}
finally {
    if ($Workbook -ne $null) {
        try { $Workbook.Close($false) | Out-Null } catch {}
        try { [System.Runtime.InteropServices.Marshal]::ReleaseComObject($Workbook) | Out-Null } catch {}
    }
    if ($Workbooks -ne $null) {
        try { [System.Runtime.InteropServices.Marshal]::ReleaseComObject($Workbooks) | Out-Null } catch {}
    }
    if ($Excel -ne $null) {
        try { $Excel.Quit() | Out-Null } catch {}
        try { [System.Runtime.InteropServices.Marshal]::ReleaseComObject($Excel) | Out-Null } catch {}
    }
}
"#;

/// PowerPoint 转 PDF PowerShell 脚本
#[cfg(windows)]
const POWERPOINT_TO_PDF_SCRIPT: &str = r#"
$File = [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String('__SOURCE_PATH__'))
$OutFile = [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String('__OUTPUT_PATH__'))
$PowerPoint = $null
$Presentations = $null
$Presentation = $null
try {
    $PowerPoint = New-Object -ComObject PowerPoint.Application
    $Presentations = $PowerPoint.Presentations
    $Presentation = $Presentations.Open($File, $true, $false, $false)
    $Presentation.SaveAs($OutFile, 32)
}
finally {
    if ($Presentation -ne $null) {
        try { $Presentation.Close() | Out-Null } catch {}
        try { [System.Runtime.InteropServices.Marshal]::ReleaseComObject($Presentation) | Out-Null } catch {}
    }
    if ($Presentations -ne $null) {
        try { [System.Runtime.InteropServices.Marshal]::ReleaseComObject($Presentations) | Out-Null } catch {}
    }
    if ($PowerPoint -ne $null) {
        try { $PowerPoint.Quit() | Out-Null } catch {}
        try { [System.Runtime.InteropServices.Marshal]::ReleaseComObject($PowerPoint) | Out-Null } catch {}
    }
}
"#;

/// Shell 类型
#[derive(Clone, Copy)]
enum ShellKind {
    /// Linux Bash
    Linux,

    /// Windows PowerShell
    Windows,
}

/// 在独立运行时线程中执行同步 Shell 入口
fn run_shell_blocking(shell: &str, shell_kind: ShellKind) -> io::Result<String> {
    let shell = shell.to_string();
    std::thread::Builder::new()
        .name("common-shell-runner".to_string())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;

            runtime.block_on(async move {
                match shell_kind {
                    ShellKind::Linux => {
                        linux_shell_with_timeout(&shell, DEFAULT_SHELL_TIMEOUT).await
                    }
                    ShellKind::Windows => {
                        windows_shell_with_timeout(&shell, DEFAULT_SHELL_TIMEOUT).await
                    }
                }
            })
        })?
        .join()
        .map_err(|_| io::Error::other("Shell 执行线程异常退出"))?
}

/// 执行 Shell 子进程并并行收集标准输出与错误输出
async fn execute_shell(
    program: &str,
    arguments: &[&str],
    timeout: Option<Duration>,
) -> io::Result<Output> {
    let mut command = Command::new(program);
    command
        .args(arguments)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    #[cfg(unix)]
    command.process_group(0);

    let child = command.spawn()?;
    let process_id = child
        .id()
        .ok_or_else(|| io::Error::other("无法获取 Shell 进程 ID"))?;
    let mut process_tree_guard = ProcessTreeGuard::new(process_id);

    let output = match timeout {
        Some(timeout) => match tokio::time::timeout(timeout, child.wait_with_output()).await {
            Ok(Ok(output)) => output,
            Ok(Err(error)) => return Err(error),
            Err(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("Shell 执行超过 {} 秒", timeout.as_secs_f64()),
                ));
            }
        },
        None => child.wait_with_output().await?,
    };

    process_tree_guard.disarm();
    Ok(output)
}

/// 进程树清理守卫
///
/// 超时、等待失败或 Future 被取消时终止子进程及其后代
struct ProcessTreeGuard {
    /// 根进程 ID
    process_id: u32,

    /// 是否需要执行清理
    active: bool,
}

impl ProcessTreeGuard {
    /// 创建进程树清理守卫
    fn new(process_id: u32) -> Self {
        Self {
            process_id,
            active: true,
        }
    }

    /// 停止自动清理
    fn disarm(&mut self) {
        self.active = false;
    }
}

impl Drop for ProcessTreeGuard {
    /// 作用域退出时尽力终止进程树
    fn drop(&mut self) {
        if self.active {
            terminate_process_tree(self.process_id);
        }
    }
}

/// 终止进程树
#[cfg(unix)]
fn terminate_process_tree(process_id: u32) {
    if let Ok(process_id) = i32::try_from(process_id) {
        // SAFETY: 负进程 ID 表示向该进程组发送 SIGKILL，不解引用任何指针
        unsafe {
            libc::kill(-process_id, libc::SIGKILL);
        }
    }
}

/// 终止进程树
#[cfg(windows)]
fn terminate_process_tree(process_id: u32) {
    let _ = std::process::Command::new("taskkill")
        .args(["/PID", &process_id.to_string(), "/T", "/F"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

/// 终止进程树
#[cfg(not(any(unix, windows)))]
fn terminate_process_tree(_process_id: u32) {}

/// 检查命令是否执行成功并返回标准输出
fn ensure_command_success(output: Output) -> io::Result<String> {
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }

    let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let message = if message.is_empty() {
        format!("命令执行失败，退出码: {:?}", output.status.code())
    } else {
        format!(
            "命令执行失败，退出码: {:?}，错误: {message}",
            output.status.code()
        )
    };

    Err(io::Error::other(message))
}

/// 校验 Shell 超时时间
fn validate_shell_timeout(timeout: Duration) -> io::Result<()> {
    if timeout.is_zero() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Shell 超时时间必须大于 0",
        ));
    }

    Ok(())
}

/// 转义 PowerShell 单引号字符串
fn powershell_single_quoted(value: &str) -> String {
    value.replace('\'', "''")
}

/// 解析本机地址
fn resolve_host_ips() -> Vec<String> {
    let Ok(interfaces) = if_addrs::get_if_addrs() else {
        return Vec::new();
    };

    let mut seen = HashSet::new();
    let mut ips = Vec::new();
    for interface in interfaces {
        let ip = interface.ip().to_string();
        if seen.insert(ip.clone()) {
            ips.push(ip);
        }
    }

    ips
}

/// 查找优先使用的本机地址
fn find_preferred_ip(ips: Vec<String>, matches_family: fn(&IpAddr) -> bool) -> Option<String> {
    let mut fallback = None;
    for item in ips {
        let Ok(ip) = item.parse::<IpAddr>() else {
            continue;
        };
        if !matches_family(&ip) {
            continue;
        }

        if fallback.is_none() {
            fallback = Some(item.clone());
        }
        if !ip.is_loopback() {
            return Some(item);
        }
    }

    fallback
}
