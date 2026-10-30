//! 明确进程职责；生产 API 必须显式关闭内嵌轮询。
use super::Result;

/// 启动时固定的进程职责，不允许配置热更新切换。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessRole {
    /// 仅处理 HTTP 请求。
    Api,
    /// 兼容本地单进程开发。
    Embedded,
    /// 仅执行常驻任务并提供健康探针。
    Worker,
}

impl ProcessRole {
    /// 读取 API 的后台执行模式。
    /// # 参数
    /// 环境变量 `ERP_BACKGROUND_MODE`：embedded 或 external，缺省为 embedded。
    /// # 返回
    /// 固定的 API 职责。
    /// # 错误
    /// 非法值或非 Unicode 值拒绝启动，避免误启后台轮询。
    pub fn from_api_environment() -> Result<Self> {
        match std::env::var("ERP_BACKGROUND_MODE") {
            Ok(value) => Self::parse(&value),
            Err(std::env::VarError::NotPresent) => Ok(Self::Embedded),
            Err(error) => Err(error.into()),
        }
    }

    /// 校验明确的执行模式。
    fn parse(value: &str) -> Result<Self> {
        match value {
            "embedded" => Ok(Self::Embedded),
            "external" => Ok(Self::Api),
            _ => Err("ERP_BACKGROUND_MODE 必须为 embedded 或 external".into()),
        }
    }

    /// 判断是否启动 HTTP 业务路由。
    pub(super) fn serves_api(self) -> bool {
        self != Self::Worker
    }

    /// 判断是否启动常驻任务。
    pub(super) fn runs_workers(self) -> bool {
        self != Self::Api
    }

    /// 返回默认 telemetry 服务标识。
    pub(super) fn service_name(self) -> &'static str {
        if self == Self::Worker { "erp-worker" } else { "erp-web-api" }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_separate_http_and_background_work_and_reject_typos() {
        let api = ProcessRole::parse("external").unwrap();
        assert!(api.serves_api());
        assert!(!api.runs_workers());
        let embedded = ProcessRole::parse("embedded").unwrap();
        assert!(embedded.serves_api() && embedded.runs_workers());
        assert!(!ProcessRole::Worker.serves_api());
        assert!(ProcessRole::Worker.runs_workers());
        for value in ["", "false", "External", "worker"] {
            assert!(ProcessRole::parse(value).is_err());
        }
    }
}
