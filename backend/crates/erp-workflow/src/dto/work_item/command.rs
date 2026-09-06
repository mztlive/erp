use serde::{Deserialize, Serialize};
use validator::Validate;

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
/// HTTP 使用冲突任务 ID 再向 read-models 投影；命令层不持有查询视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorkItemConflict {
    #[serde(skip)]
    kind: WorkItemConflictKind,
    #[serde(skip)]
    work_item_id: Option<String>,
}

impl WorkItemConflict {
    /// 创建责任命令冲突数据。
    ///
    /// # 参数
    /// * `kind` - 稳定冲突分类
    /// * `work_item_id` - 仍存在时的任务 ID；已删除时为空
    ///
    /// # 返回
    /// 返回命令层冲突事实。
    pub fn new(kind: WorkItemConflictKind, work_item_id: Option<String>) -> Self {
        Self { kind, work_item_id }
    }

    /// 返回稳定冲突分类。
    pub fn kind(&self) -> WorkItemConflictKind {
        self.kind
    }

    /// 返回冲突时仍存在的任务 ID。
    pub fn work_item_id(&self) -> Option<&str> {
        self.work_item_id.as_deref()
    }
}

/// 责任命令的服务端结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkItemMutationOutcome {
    /// 命令已应用；HTTP 再向 read-models 投影。
    Applied { work_item_id: String },
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
