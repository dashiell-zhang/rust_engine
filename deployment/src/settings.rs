use crate::Result;
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DeploymentSettings {
    pub project_name: String,
    pub host: Hosts,
    pub port: Ports,
    pub deploy_root: String,
    #[serde(rename = "YunXiao")]
    pub yun_xiao: YunXiao,
}

#[derive(Deserialize)]
pub struct Hosts {
    #[serde(rename = "AdminWebAPI")]
    pub admin_webapi: Host,
    #[serde(rename = "ClientWebAPI")]
    pub client_webapi: Host,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Host {
    pub domain: String,
    pub certificate_file: String,
    pub certificate_key_file: String,
}

#[derive(Deserialize)]
pub struct Ports {
    #[serde(rename = "AdminWebAPI")]
    pub admin_webapi: u16,
    #[serde(rename = "ClientWebAPI")]
    pub client_webapi: u16,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct YunXiao {
    pub codeup: Codeup,
    pub machine_group: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Codeup {
    pub name: String,
    pub endpoint: String,
    pub branch: String,
    pub service_connection: String,
}

impl DeploymentSettings {
    pub fn validate(&self) -> Result<()> {
        if !self
            .project_name
            .split('-')
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_alphanumeric()))
        {
            return Err("ProjectName 只能由英文字母、数字和中间连字符组成".into());
        }
        let root = self
            .deploy_root
            .strip_suffix('/')
            .unwrap_or(&self.deploy_root);
        if !root.starts_with('/') || root.len() < 2 || !valid_path(&root[1..]) {
            return Err("DeployRoot 必须为非根目录的 Linux 绝对路径，不能含空段、. 或 ..".into());
        }
        if self.port.admin_webapi == 0
            || self.port.client_webapi == 0
            || self.port.admin_webapi == self.port.client_webapi
        {
            return Err("两个 API 端口必须在 1 到 65535 之间，且不能相同".into());
        }
        for (name, host) in [
            ("AdminWebAPI", &self.host.admin_webapi),
            ("ClientWebAPI", &self.host.client_webapi),
        ] {
            if host.domain.len() > 253
                || !host.domain.split('.').all(|part| {
                    !part.is_empty()
                        && part.len() <= 63
                        && part.as_bytes()[0].is_ascii_alphanumeric()
                        && part.as_bytes()[part.len() - 1].is_ascii_alphanumeric()
                        && part
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                })
            {
                return Err(
                    format!("Host.{name}.Domain 必须是域名，不能包含协议、端口或路径").into(),
                );
            }
            for (key, path) in [
                ("CertificateFile", &host.certificate_file),
                ("CertificateKeyFile", &host.certificate_key_file),
            ] {
                if !valid_path(path.strip_prefix('/').unwrap_or(path)) {
                    return Err(format!("Host.{name}.{key} 必须是有效 Linux 文件路径").into());
                }
            }
        }
        for (name, value) in self.yunxiao_values() {
            if value.trim().is_empty() || value.chars().any(char::is_control) {
                return Err(format!("{name} 不能为空或包含控制字符").into());
            }
        }
        Ok(())
    }

    fn yunxiao_values(&self) -> [(&'static str, &str); 5] {
        let codeup = &self.yun_xiao.codeup;
        [
            ("YunXiao.Codeup.Name", &codeup.name),
            ("YunXiao.Codeup.Endpoint", &codeup.endpoint),
            ("YunXiao.Codeup.Branch", &codeup.branch),
            (
                "YunXiao.Codeup.ServiceConnection",
                &codeup.service_connection,
            ),
            ("YunXiao.MachineGroup", &self.yun_xiao.machine_group),
        ]
    }

    pub fn template_values(&self) -> Result<BTreeMap<String, String>> {
        let mut values = BTreeMap::from([
            ("ProjectName".into(), self.project_name.clone()),
            (
                "ProjectNameLower".into(),
                self.project_name.to_ascii_lowercase(),
            ),
            (
                "DeployRoot".into(),
                self.deploy_root.trim_end_matches('/').to_string(),
            ),
            (
                "Port.AdminWebAPI".into(),
                self.port.admin_webapi.to_string(),
            ),
            (
                "Port.ClientWebAPI".into(),
                self.port.client_webapi.to_string(),
            ),
        ]);
        for (name, host) in [
            ("AdminWebAPI", &self.host.admin_webapi),
            ("ClientWebAPI", &self.host.client_webapi),
        ] {
            for (key, value) in [
                ("Domain", &host.domain),
                ("CertificateFile", &host.certificate_file),
                ("CertificateKeyFile", &host.certificate_key_file),
            ] {
                values.insert(format!("Host.{name}.{key}"), value.clone());
            }
        }
        // JSON 双引号字符串同时是合法 YAML 标量，避免分支名等被当作 YAML 语法。
        for (key, value) in self.yunxiao_values() {
            values.insert(key.to_string(), serde_json::to_string(value)?);
        }
        Ok(values)
    }
}

fn valid_path(path: &str) -> bool {
    path.split('/').all(|part| {
        !part.is_empty()
            && part != "."
            && part != ".."
            && part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    })
}
