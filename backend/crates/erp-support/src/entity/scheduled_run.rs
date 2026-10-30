//! 调度回执：领取、业务执行结果与调度中心回报分离。
use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

/// 首批可由外部调度触发的固定能力。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScheduledHandler {
    #[serde(rename = "supplier_tracking_scan")]
    Tracking,
    #[serde(rename = "supplier_catalog_schedule")]
    Catalog,
}
/// 不可变调度身份；不同身份不得复用同一日志号。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduledRequest {
    pub app: String,
    pub job_id: i32,
    pub log_id: i64,
    pub log_time: i64,
    pub handler: ScheduledHandler,
}
impl ScheduledRequest {
    /// 构造确定性回执主键。
    /// # 参数
    /// 无。
    /// # 返回
    /// app 与日志号组成的稳定键；app 由配置限制为字母、数字和连字符。
    /// # 错误
    /// 无。
    pub fn key(&self) -> String {
        format!("{}:{}", self.app, self.log_id)
    }
}

/// 持久化执行状态。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScheduledState {
    Accepted,
    Running,
    Completed,
}

/// 仅保存固定脱敏摘要的调度回执。
#[derive(Clone, Debug, Serialize, Deserialize, Entity)]
pub struct ScheduledRun {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub request: ScheduledRequest,
    pub state: ScheduledState,
    pub owner: Option<String>,
    pub handle_code: i32,
    pub handle_msg: String,
    pub callback_done: bool,
    pub callback_after: i64,
}
impl ScheduledRun {
    /// 创建尚未执行业务的回执。
    /// # 参数
    /// `request` 为协议层已校验身份。
    /// # 返回
    /// 占用该 Job ID 活动槽位的 accepted 回执。
    /// # 错误
    /// 无。
    pub fn accepted(request: ScheduledRequest) -> Self {
        Self {
            base: BaseModel::new(request.key()),
            request,
            state: ScheduledState::Accepted,
            owner: None,
            handle_code: 0,
            handle_msg: "等待执行".into(),
            callback_done: false,
            callback_after: 0,
        }
    }
}
