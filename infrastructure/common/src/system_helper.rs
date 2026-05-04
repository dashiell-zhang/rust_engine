use std::{
    io,
    net::{IpAddr, ToSocketAddrs},
    process::Command,
};

/// 获取本机全部 IP
pub fn get_all_ip_address() -> Vec<String> {
    resolve_host_ips()
}

/// 获取本机 IPv4 地址
pub fn get_ipv4_address() -> Option<String> {
    resolve_host_ips()
        .into_iter()
        .find(|item| item.parse::<IpAddr>().ok().is_some_and(|ip| ip.is_ipv4()))
}

/// 获取本机 IPv6 地址
pub fn get_ipv6_address() -> Option<String> {
    resolve_host_ips()
        .into_iter()
        .find(|item| item.parse::<IpAddr>().ok().is_some_and(|ip| ip.is_ipv6()))
}

/// Linux 运行 shell 脚本
pub fn linux_shell(shell: &str) -> std::io::Result<String> {
    let output = Command::new("/bin/bash").arg("-c").arg(shell).output()?;
    ensure_command_success(output.status, &output.stderr)?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Windows 运行 shell 脚本
pub fn windows_shell(shell: &str) -> std::io::Result<String> {
    let output = Command::new("powershell").arg("-c").arg(shell).output()?;
    ensure_command_success(output.status, &output.stderr)?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// 检查命令是否执行成功
fn ensure_command_success(status: std::process::ExitStatus, stderr: &[u8]) -> io::Result<()> {
    if status.success() {
        return Ok(());
    }

    let message = String::from_utf8_lossy(stderr).trim().to_string();
    let message = if message.is_empty() {
        format!("命令执行失败，退出码: {:?}", status.code())
    } else {
        message
    };

    Err(io::Error::other(message))
}

/// 解析本机地址
fn resolve_host_ips() -> Vec<String> {
    let Ok(hostname) = hostname::get() else {
        return Vec::new();
    };
    let hostname = hostname.to_string_lossy().into_owned();

    match (hostname.as_str(), 0).to_socket_addrs() {
        Ok(iter) => iter.map(|item| item.ip().to_string()).collect(),
        Err(_) => Vec::new(),
    }
}
