//! API 与 Worker 的固定进程职责。

/// 职责由二进制入口决定，不提供内嵌或运行时切换模式。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessRole {
    /// 仅处理 HTTP 请求。
    Api,
    /// 消费常驻任务并接受 XXL-JOB 调度。
    Worker,
}

impl ProcessRole {
    /// 判断是否启动 HTTP 业务路由。
    pub(super) fn serves_api(self) -> bool {
        self == Self::Api
    }
    /// 判断是否启动常驻任务。
    pub(super) fn runs_workers(self) -> bool {
        self == Self::Worker
    }
    /// 返回默认 telemetry 服务标识。
    pub(super) fn service_name(self) -> &'static str {
        match self {
            Self::Api => "erp-web-api",
            Self::Worker => "erp-worker",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn binary_roles_separate_http_and_background_work() {
        assert!(ProcessRole::Api.serves_api());
        assert!(!ProcessRole::Api.runs_workers());
        assert!(!ProcessRole::Worker.serves_api());
        assert!(ProcessRole::Worker.runs_workers());
    }
}
