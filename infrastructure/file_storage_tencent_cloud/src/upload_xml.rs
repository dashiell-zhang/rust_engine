use common::app_error::AppError;
use quick_xml::{Reader, events::Event};
use serde::{Deserialize, de::DeserializeOwned};

#[derive(Deserialize)]
pub(super) struct UploadList {
    #[serde(rename = "Upload", default)]
    pub(super) uploads: Vec<PendingUpload>,
}

#[derive(Deserialize)]
pub(super) struct PendingUpload {
    #[serde(rename = "Key")]
    pub(super) key: Option<String>,
    #[serde(rename = "UploadId")]
    pub(super) id: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct PartList {
    #[serde(rename = "Part", default)]
    pub(super) parts: Vec<RemotePart>,
}

#[derive(Deserialize)]
pub(super) struct RemotePart {
    #[serde(rename = "PartNumber")]
    pub(super) number: Option<String>,
    #[serde(rename = "ETag")]
    pub(super) etag: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct InitiatedUpload {
    #[serde(rename = "UploadId")]
    pub(super) id: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct CompletedUpload {}

pub(super) fn parse<T: DeserializeOwned>(body: &[u8], root: &str) -> Result<T, AppError> {
    let mut reader = Reader::from_reader(body);
    loop {
        match reader.read_event().map_err(|_| invalid_xml())? {
            Event::Start(element) | Event::Empty(element) if element.name().as_ref() == root => {
                if element
                    .try_get_attribute("xmlns")
                    .map_err(|_| invalid_xml())?
                    .is_some_and(|attribute| !attribute.value.is_empty())
                {
                    return Err(invalid_xml());
                }
                break;
            }
            Event::Start(_) | Event::Empty(_) | Event::Eof => return Err(invalid_xml()),
            _ => {}
        }
    }
    quick_xml::de::from_reader(body).map_err(|_| invalid_xml())
}

/// 与 SDK 默认 XmlWriter 一样无 XML 声明，按分片编号顺序输出。
pub(super) fn completion<'a>(parts: impl Iterator<Item = (usize, Option<&'a str>)>) -> Vec<u8> {
    let mut xml = String::from("<CompleteMultipartUpload>");
    for (number, etag) in parts {
        xml.push_str(&format!(
            "\n  <Part>\n    <PartNumber>{number}</PartNumber>"
        ));
        if let Some(etag) = etag {
            let escaped = etag
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            xml.push_str(&format!("\n    <ETag>{escaped}</ETag>"));
        }
        xml.push_str("\n  </Part>");
    }
    xml.push_str("\n</CompleteMultipartUpload>");
    xml.into_bytes()
}

fn invalid_xml() -> AppError {
    AppError::internal("腾讯云 COS 上传响应 XML 无效")
}
