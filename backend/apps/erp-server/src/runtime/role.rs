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
    // 供启动装配识别 Web API 角色，只有该角色挂载业务 HTTP 路由。
    pub(crate) fn serves_api(self) -> bool {
        self == Self::Api
    }
    // 供启动和调度配置校验识别独立 Worker，避免 API 进程领取后台工作。
    pub(crate) fn runs_workers(self) -> bool {
        self == Self::Worker
    }
    // 按二进制角色选择默认 telemetry 服务名，便于区分 API 和 Worker 的日志与追踪。
    pub(crate) fn service_name(self) -> &'static str {
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
