use serde::{Deserialize, Serialize};
use validator::Validate;

use super::view::WorkItemView;

/// 责任命令的稳定冲突分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkItemConflictKind {
    /// 客户端提交的任务版本已经陈旧。
    Version,
    /// 任务当前责任已经由其他操作改变。
    Responsibility,
}

impl WorkItemConflictKind {
    /// 返回 HTTP 契约使用的稳定错误码。
    ///
    /// # 返回
    /// 返回不依赖展示文案的冲突代码。
    pub fn code(self) -> &'static str {
        match self {
            Self::Version => "WORK_ITEM_VERSION_CONFLICT",
            Self::Responsibility => "WORK_ITEM_RESPONSIBILITY_CONFLICT",
        }
    }

    /// 返回权限安全的用户提示。
    ///
    /// # 返回
    /// 返回不包含处理人 ID 或内部版本细节的提示。
    pub fn message(self) -> &'static str {
        match self {
            Self::Version => "任务已被其他操作更新，请按最新状态重试",
            Self::Responsibility => "任务责任已变化，请按最新状态处理",
        }
    }
}

/// 责任命令冲突时返回的权限安全数据。
///
/// `current_work_item` 必须由 Service 使用当前 actor 重新投影；若最新任务
/// 已不在 actor 的查看范围内，则固定返回 `null`，不得降级返回原始实体。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorkItemConflict {
    #[serde(skip)]
    kind: WorkItemConflictKind,
    /// 当前仍可见时的最新安全投影；不可见或已删除时为空。
    pub current_work_item: Option<WorkItemView>,
}

impl WorkItemConflict {
    /// 创建责任命令冲突数据。
    ///
    /// # 参数
    /// * `kind` - 稳定冲突分类
    /// * `current_work_item` - actor 重新授权后的最新安全投影
    ///
    /// # 返回
    /// 返回可直接放入 409 响应 `data` 的冲突数据。
    pub fn new(kind: WorkItemConflictKind, current_work_item: Option<WorkItemView>) -> Self {
        Self {
            kind,
            current_work_item,
        }
    }

    /// 返回稳定冲突分类。
    ///
    /// # 返回
    /// 返回版本冲突或责任冲突。
    pub fn kind(&self) -> WorkItemConflictKind {
        self.kind
    }
}

/// 责任命令的服务端结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkItemMutationOutcome {
    /// 命令已应用并返回更新后的安全投影。
    Applied(WorkItemView),
    /// 命令因并发版本或责任变化未应用。
    Conflict(WorkItemConflict),
}

/// 转交任务请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ReassignWorkItemRequest {
    #[validate(length(min = 1, max = 20, message = "任务版本格式非法"))]
    pub expected_task_version: String,
    #[validate(length(min = 1, max = 128, message = "目标用户不能为空或过长"))]
    pub target_user_id: String,
    #[validate(length(min = 1, max = 150, message = "原因长度必须在1-150之间"))]
    pub reason: String,
    #[validate(length(min = 1, max = 128, message = "幂等键长度必须在1-128之间"))]
    pub idempotency_key: String,
}

/// 关闭无效任务请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CloseWorkItemRequest {
    #[validate(length(min = 1, max = 20, message = "任务版本格式非法"))]
    pub expected_task_version: String,
    #[validate(length(min = 1, max = 64, message = "关闭原因代码不能为空或过长"))]
    pub reason_code: String,
    #[validate(length(max = 100, message = "关闭说明不能超过100个字符"))]
    pub comment: Option<String>,
    /// `DUPLICATE` 时必填的有效替代正式任务。
    #[validate(length(min = 1, max = 128, message = "替代任务ID不能为空或过长"))]
    pub replacement_work_item_id: Option<String>,
    #[validate(length(min = 1, max = 128, message = "幂等键长度必须在1-128之间"))]
    pub idempotency_key: String,
}
