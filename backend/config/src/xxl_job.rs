//! 固定启动期的 XXL-JOB 2.4.1 执行器配置。
use std::fmt;

use serde::Deserialize;
use url::Url;

use crate::{Error, Result};

/// 独立 Worker 的调度中心连接与执行器身份。
#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct XxlJobConfig {
    pub admin_url: String,
    pub executor_url: String,
    pub app_name: String,
    pub access_token: String,
}

impl fmt::Debug for XxlJobConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("XxlJobConfig")
            .field("app_name", &self.app_name)
            .field("access_token", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

impl XxlJobConfig {
    /// 加载已提供的 XXL-JOB 配置时，检查调度中心和执行器地址、执行器应用名及访问令牌，
    /// 在 Worker 尝试注册前发现无效配置。API 可省略这段配置，但提供时执行同样校验。
    pub fn validate(&self) -> Result<()> {
        validate_url(&self.admin_url, false)?;
        validate_url(&self.executor_url, true)?;
        if self.app_name.is_empty()
            || self.app_name.len() > 80
            || !self.app_name.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        {
            return Err(Error::Invalid(
                "xxl_job.app_name must be 1-80 lowercase letters, digits or hyphens".into(),
            ));
        }
        if self.access_token.len() < 16
            || self.access_token.len() > 256
            || !self.access_token.bytes().all(|c| c.is_ascii_graphic())
        {
            return Err(Error::Invalid(
                "xxl_job.access_token must contain 16-256 printable non-space ASCII bytes".into(),
            ));
        }
        Ok(())
    }
}

// 校验调度地址有主机、使用 HTTP(S) 且不含凭据、查询参数或片段；执行器必须用根路径，
// 只有调度中心可带路径前缀。
fn validate_url(value: &str, executor: bool) -> Result<()> {
    let invalid = || {
        Error::Invalid(
            "xxl_job URLs require HTTP(S), host and no credentials/query/fragment; executor uses root path"
                .into(),
        )
    };
    let url = Url::parse(value).map_err(|_| invalid())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || executor && url.path() != "/"
    {
        return Err(invalid());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn valid() -> XxlJobConfig {
        XxlJobConfig {
            admin_url: "http://admin/xxl-job-admin".into(),
            executor_url: "http://erp-worker.test.svc:10001".into(),
            app_name: "erp-test".into(),
            access_token: "test-secret-123456".into(),
        }
    }
    #[test]
    fn connection_config_requires_no_job_ids_and_rejects_unknown_settings() {
        assert!(valid().validate().is_ok());
        assert!(!format!("{:?}", valid()).contains("test-secret"));
        assert!(toml::from_str::<XxlJobConfig>("").is_err());
        let complete = r#"admin_url = "http://admin/xxl-job-admin"
executor_url = "http://worker:10002"
app_name = "erp-local"
access_token = "test-token-123456"
"#;
        assert!(toml::from_str::<XxlJobConfig>(complete).unwrap().validate().is_ok());
        for invalid in [
            complete.replace("app_name = \"erp-local\"", ""),
            format!("{complete}enabled = false\n"),
            format!("{complete}tracking_job_id = 1\n"),
        ] {
            assert!(toml::from_str::<XxlJobConfig>(&invalid).is_err());
        }
        for url in [
            "file:///tmp/test",
            "http://u:p@host/",
            "http://host/?token=x",
            "http://host/#x",
            "http://host/run",
        ] {
            let mut c = valid();
            c.executor_url = url.into();
            assert!(c.validate().is_err());
        }
        let mut c = valid();
        c.access_token = "bad\nsecret".into();
        assert!(c.validate().is_err());
    }
}
