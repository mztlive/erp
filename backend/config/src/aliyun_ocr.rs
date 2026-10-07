//! 可选阿里云 OCR 配置；凭据始终通过 SafeConfig 文件或 Nacos 快照读取。
use std::path::PathBuf;

use aliyun_ocr::Credentials;
use serde::Deserialize;

use crate::{Error, Result};

/// 缺省整个配置节时继续使用未配置 OCR，不自动启用外部调用。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AliyunOcrConfig {
    /// 签名凭据，Debug 已脱敏。
    pub credentials: Credentials,
    /// Poppler 可执行文件名或绝对路径；禁止来自用户上传数据。
    #[serde(default = "default_renderer")]
    pub pdftoppm_path: PathBuf,
}
fn default_renderer() -> PathBuf {
    "pdftoppm".into()
}
impl AliyunOcrConfig {
    pub(crate) fn validate(&self) -> Result<()> {
        self.credentials.validate().map_err(|_| {
            Error::Invalid("aliyun_ocr.credentials must contain valid AK/SK and optional STS token".into())
        })?;
        if self.pdftoppm_path.as_os_str().is_empty() {
            return Err(Error::Invalid("aliyun_ocr.pdftoppm_path must not be empty".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_renderer_and_redacts_all_credentials() {
        let config: AliyunOcrConfig = toml::from_str(
            "[credentials]\naccess_key_id='id123'\naccess_key_secret='secret123'\nsecurity_token='token123'",
        )
        .unwrap();
        config.validate().unwrap();
        assert_eq!(config.pdftoppm_path, PathBuf::from("pdftoppm"));
        let debug = format!("{config:?}");
        for value in ["id123", "secret123", "token123"] {
            assert!(!debug.contains(value));
        }
    }
    #[test]
    fn rejects_incomplete_or_blank_credentials() {
        assert!(toml::from_str::<AliyunOcrConfig>("[credentials]\naccess_key_id='id'").is_err());
        let config: AliyunOcrConfig =
            toml::from_str("[credentials]\naccess_key_id='id'\naccess_key_secret=' '").unwrap();
        assert!(config.validate().is_err());
    }
}
