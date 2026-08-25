//! `integration_error_task`：投递或处理失败的集成任务与人工处理（数据模型 §6.21、§7.7）。
//!
//! 投递状态由本表 `status` 表达（待处理、自动重试中、待人工、已解决、已关闭），
//! 不另设消息投递状态机，也不建 `integration_attempt` 表：重试次数、最近尝试时间与
//! 脱敏结果直接记录在本任务上。终态（已解决/已关闭）无出边，邻接矩阵逐边固化
//! （§13 禁止运行时扩展），测试采用逐边定向断言。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::state::{ensure_transition, DocumentState};
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::validation::{normalize_optional_text, normalize_required_text};

use super::{InboxMessageId, IntegrationErrorTaskId};

/// 业务对象 ID 最大长度。
const BUSINESS_OBJECT_ID_MAX_LEN: usize = 128;
/// 责任角色最大长度。
const OWNER_ROLE_MAX_LEN: usize = 64;
/// 责任人标识最大长度。
const OWNER_USER_ID_MAX_LEN: usize = 128;
/// 最近尝试结果（脱敏）最大长度。
const ATTEMPT_SUMMARY_MAX_LEN: usize = 512;
/// 解决/关闭证据文本最大长度。
const RESOLUTION_MAX_LEN: usize = 1024;

/// 错误分类（数据模型 §6.21：能力不足、映射错误、业务拒绝、临时故障、结果未知、
/// 鉴权签名、限流、乱序；固定业务代码）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorClass {
    /// 能力不足。
    CapabilityGap,
    /// 映射错误。
    MappingError,
    /// 业务拒绝。
    BusinessRejected,
    /// 临时故障。
    TransientFailure,
    /// 结果未知。
    ResultUnknown,
    /// 鉴权签名。
    AuthSignature,
    /// 限流。
    RateLimited,
    /// 乱序。
    OutOfOrder,
}

impl ErrorClass {
    /// 返回错误分类的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::CapabilityGap => "能力不足",
            Self::MappingError => "映射错误",
            Self::BusinessRejected => "业务拒绝",
            Self::TransientFailure => "临时故障",
            Self::ResultUnknown => "结果未知",
            Self::AuthSignature => "鉴权签名",
            Self::RateLimited => "限流",
            Self::OutOfOrder => "乱序",
        }
    }

    /// 返回错误分类的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::CapabilityGap => "capability_gap",
            Self::MappingError => "mapping_error",
            Self::BusinessRejected => "business_rejected",
            Self::TransientFailure => "transient_failure",
            Self::ResultUnknown => "result_unknown",
            Self::AuthSignature => "auth_signature",
            Self::RateLimited => "rate_limited",
            Self::OutOfOrder => "out_of_order",
        }
    }

    /// 判断该错误分类是否允许按规则自动重试。
    ///
    /// §7.7：网络超时、临时不可用和限流可按规则自动重试；参数/映射错误、业务明确
    /// 拒绝、鉴权或签名失败不自动重试；结果未知先查询原请求，不盲目重试。
    ///
    /// # 返回
    /// 属于可自动重试分类时返回 `true`。
    pub fn can_auto_retry(&self) -> bool {
        matches!(self, Self::TransientFailure | Self::RateLimited)
    }

    /// 判断服务端确认原动作无结果后是否允许人工重放。
    ///
    /// 结果未知必须先查询原结果；确认不存在后可安全重放。临时故障与限流
    /// 同样允许沿原业务事实键重放，其余分类失败关闭。
    ///
    /// # 返回
    /// 允许在无结果结论后重放时返回 `true`。
    pub fn allows_replay_after_no_result(self) -> bool {
        self.can_auto_retry() || self == Self::ResultUnknown
    }
}

/// 错误任务状态（数据模型 §6.21、§7.7：待处理、自动重试中、待人工、已解决、已关闭）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorTaskStatus {
    /// 待处理。
    #[default]
    Pending,
    /// 自动重试中。
    AutoRetrying,
    /// 待人工。
    ManualRequired,
    /// 已解决。
    Resolved,
    /// 已关闭。
    Closed,
}

impl ErrorTaskStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Pending => "待处理",
            Self::AutoRetrying => "自动重试中",
            Self::ManualRequired => "待人工",
            Self::Resolved => "已解决",
            Self::Closed => "已关闭",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::AutoRetrying => "auto_retrying",
            Self::ManualRequired => "manual_required",
            Self::Resolved => "resolved",
            Self::Closed => "closed",
        }
    }
}

impl DocumentState for ErrorTaskStatus {
    /// 返回当前状态的全部合法后继状态。
    ///
    /// 迁移规则（数据模型 §6.21、§7.7）：
    /// - 待处理可按规则进入自动重试中，或直接转人工（不自动重试的分类）；
    /// - 自动重试中因重试耗尽、鉴权失败或结果未知转人工，取得可验证终态则解决；
    /// - 重复或误派任务（含待处理/自动重试中/待人工）经证据核对后关闭；
    /// - 已解决/已关闭是终态，无出边。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Pending => &[
                Self::AutoRetrying,
                Self::ManualRequired,
                Self::Resolved,
                Self::Closed,
            ],
            Self::AutoRetrying => &[Self::ManualRequired, Self::Resolved, Self::Closed],
            Self::ManualRequired => &[Self::Resolved, Self::Closed],
            Self::Resolved | Self::Closed => &[],
        }
    }
}

/// 解决方式（数据模型 §6.21：查询确认、修复映射、重放、补偿、关闭；固定业务代码）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionType {
    /// 查询确认。
    QueryConfirm,
    /// 修复映射。
    FixMapping,
    /// 重放。
    Replay,
    /// 补偿。
    Compensate,
    /// 关闭。
    Close,
}

impl ResolutionType {
    /// 返回解决方式的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::QueryConfirm => "查询确认",
            Self::FixMapping => "修复映射",
            Self::Replay => "重放",
            Self::Compensate => "补偿",
            Self::Close => "关闭",
        }
    }

    /// 返回解决方式的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::QueryConfirm => "query_confirm",
            Self::FixMapping => "fix_mapping",
            Self::Replay => "replay",
            Self::Compensate => "compensate",
            Self::Close => "close",
        }
    }

    /// 根据已验证终态证据的领域含义选择解决方式。
    ///
    /// 补偿结果优先于业务对象核验；两者均无时使用查询确认。
    ///
    /// # 参数
    /// * `has_compensation` - 是否含正式补偿结果
    /// * `has_business_verification` - 是否含业务对象核验证据
    ///
    /// # 返回
    /// 返回唯一解决方式。
    pub fn from_verified_evidence(has_compensation: bool, has_business_verification: bool) -> Self {
        if has_compensation {
            Self::Compensate
        } else if has_business_verification {
            Self::FixMapping
        } else {
            Self::QueryConfirm
        }
    }
}

/// 错误任务创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IntegrationErrorTaskData {
    /// 关联的消息（消息类失败必填其一）。
    pub message_id: Option<InboxMessageId>,
    /// 关联的业务对象（非消息类失败必填其一）。
    pub business_object_id: Option<String>,
    /// 错误分类。
    pub error_class: ErrorClass,
    /// 责任角色。
    pub owner_role: Option<String>,
    /// 责任人。
    pub owner_user_id: Option<String>,
}

/// 错误任务更新数据（只允许修改责任信息；消息/业务对象与错误分类是关键字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct IntegrationErrorTaskUpdate {
    /// 新的责任角色；`None` 表示不修改。
    pub owner_role: Option<String>,
    /// 新的责任人；`None` 表示不修改。
    pub owner_user_id: Option<String>,
}

/// 集成错误任务实体（数据模型 §6.21）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct IntegrationErrorTask {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 关联的消息。
    pub message_id: Option<InboxMessageId>,
    /// 关联的业务对象。
    pub business_object_id: Option<String>,
    /// 错误分类。
    pub error_class: ErrorClass,
    /// 任务状态。
    pub status: ErrorTaskStatus,
    /// 责任角色。
    pub owner_role: Option<String>,
    /// 责任人。
    pub owner_user_id: Option<String>,
    /// 重试次数。
    pub attempt_count: u32,
    /// 最近尝试时间。
    pub last_attempt_at: Option<Instant>,
    /// 最近尝试结果（脱敏）。
    pub last_attempt_summary: Option<String>,
    /// 解决方式。
    pub resolution_type: Option<ResolutionType>,
    /// 解决/关闭证据说明。
    pub resolution: Option<String>,
    /// 完成时间。
    pub resolved_at: Option<Instant>,
}

impl IntegrationErrorTask {
    /// 创建集成错误任务。
    ///
    /// 完成 `business_object_id` 与责任字段的校验与规范化，并强制不变式：任务必须
    /// 至少关联消息或业务对象之一（关联一致性）。新任务一律为待处理，重试次数从 0 起。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::IntegrationErrorTaskId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的错误任务实体。
    ///
    /// # 错误
    /// 当消息与业务对象都为空、业务对象 ID 超长或责任字段超长时返回错误。
    pub fn new(id: IntegrationErrorTaskId, data: IntegrationErrorTaskData) -> Result<Self> {
        let business_object_id =
            normalize_optional_text(data.business_object_id, "业务对象ID", BUSINESS_OBJECT_ID_MAX_LEN)?;
        if data.message_id.is_none() && business_object_id.is_none() {
            return Err(Error::from("错误任务必须关联消息或业务对象"));
        }
        let owner_role = normalize_optional_text(data.owner_role, "责任角色", OWNER_ROLE_MAX_LEN)?;
        let owner_user_id = normalize_optional_text(data.owner_user_id, "责任人", OWNER_USER_ID_MAX_LEN)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            message_id: data.message_id,
            business_object_id,
            error_class: data.error_class,
            status: ErrorTaskStatus::Pending,
            owner_role,
            owner_user_id,
            attempt_count: 0,
            last_attempt_at: None,
            last_attempt_summary: None,
            resolution_type: None,
            resolution: None,
            resolved_at: None,
        })
    }

    /// 更新错误任务的责任信息。
    ///
    /// 消息、业务对象与错误分类是关键字段，不在通用更新中修改。
    ///
    /// # 参数
    /// * `update` - 更新数据
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当责任字段超长时返回错误。
    pub fn update(&mut self, update: IntegrationErrorTaskUpdate) -> Result<()> {
        self.owner_role = normalize_optional_text(update.owner_role, "责任角色", OWNER_ROLE_MAX_LEN)?;
        self.owner_user_id = normalize_optional_text(update.owner_user_id, "责任人", OWNER_USER_ID_MAX_LEN)?;
        Ok(())
    }

    /// 记录一次重试尝试。
    ///
    /// 重试次数递增并记录最近尝试时间与脱敏结果（自动重试与人工重放均使用原幂等键，
    /// §7.7；幂等键由 `inbox_message.business_fact_key` 承载）。
    ///
    /// # 参数
    /// * `at` - 本次尝试时间
    /// * `summary` - 脱敏的尝试结果摘要
    ///
    /// # 返回
    /// 记录成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 终态（已解决/已关闭）任务不允许再记录尝试时返回错误。
    pub fn record_attempt(&mut self, at: Instant, summary: Option<String>) -> Result<()> {
        if self.is_terminal() {
            return Err(Error::from("终态任务不允许再记录尝试"));
        }
        self.attempt_count += 1;
        self.last_attempt_at = Some(at);
        self.last_attempt_summary =
            normalize_optional_text(summary, "最近尝试结果", ATTEMPT_SUMMARY_MAX_LEN)?;
        Ok(())
    }

    /// 按固定邻接矩阵迁移任务状态。
    ///
    /// 终态要求（数据模型 §6.21）：
    /// - 迁移到已解决：必须提供非「关闭」的解决方式与终态证据（取得可验证终态，
    ///   或形成经复核的取消/退款/冲正/补偿事实并完成对账后才能解决；对账完整性
    ///   校验依赖跨聚合查询，属 P3 条目 §8.4）；
    /// - 迁移到已关闭：必须使用「关闭」解决方式并提供替代任务或误派证据；
    ///   W02 受控关闭入口负责校验替代任务、对象类别与管理范围；
    /// - 非终态迁移不允许携带解决信息。
    ///
    /// # 参数
    /// * `to` - 目标状态
    /// * `resolution_type` - 解决方式（终态迁移必填）
    /// * `resolution` - 解决/关闭证据说明（终态迁移必填）
    /// * `at` - 完成时间
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 迁移不在邻接矩阵中，或终态迁移缺少解决方式/证据、非终态迁移携带解决信息时返回错误。
    pub fn transition(
        &mut self,
        to: ErrorTaskStatus,
        resolution_type: Option<ResolutionType>,
        resolution: Option<String>,
        at: Instant,
    ) -> Result<()> {
        ensure_transition(self.status, to)?;
        match to {
            ErrorTaskStatus::Resolved => self.apply_resolve(resolution_type, resolution, at)?,
            ErrorTaskStatus::Closed => self.apply_close(resolution_type, resolution, at)?,
            _ => {
                if resolution_type.is_some() || resolution.is_some() {
                    return Err(Error::from("非终态迁移不允许携带解决信息"));
                }
            }
        }
        self.status = to;
        Ok(())
    }

    /// 判断任务是否处于终态。
    ///
    /// # 返回
    /// 状态为已解决或已关闭时返回 `true`。
    pub fn is_terminal(&self) -> bool {
        matches!(self.status, ErrorTaskStatus::Resolved | ErrorTaskStatus::Closed)
    }

    /// 判断最近一次动作是否已由服务端确认原动作无结果。
    ///
    /// # 返回
    /// 最近摘要同时包含查询原结果动作与无结果结论时返回 `true`。
    pub fn prior_query_confirmed_no_result(&self) -> bool {
        self.last_attempt_summary.as_deref().is_some_and(|summary| {
            summary.contains("w29_action=QUERY_ORIGINAL_RESULT")
                && summary.contains("outcome=NoResultConfirmed")
        })
    }

    /// 判断当前任务是否满足人工重放的全部纯领域前置条件。
    ///
    /// # 返回
    /// 服务端已确认原动作无结果，且错误分类允许重放时返回 `true`。
    pub fn can_replay_original(&self) -> bool {
        !self.is_terminal()
            && self.prior_query_confirmed_no_result()
            && self.error_class.allows_replay_after_no_result()
    }

    /// 判断命令携带的业务主题版本是否仍与任务一致。
    ///
    /// # 参数
    /// * `expected` - 客户端冻结的十进制版本字符串
    ///
    /// # 返回
    /// 去除首尾空白后与当前乐观锁版本一致时返回 `true`。
    pub fn has_subject_version(&self, expected: &str) -> bool {
        self.base.version.to_string() == expected.trim()
    }

    /// 判断任务当前是否允许自动重试。
    ///
    /// 要求错误分类可自动重试（§7.7）且任务仍处于待处理/自动重试中（转人工后由
    /// 人工决定，人工重放仍使用原幂等键）。
    ///
    /// # 返回
    /// 允许自动重试时返回 `true`。
    pub fn can_auto_retry(&self) -> bool {
        self.error_class.can_auto_retry()
            && matches!(
                self.status,
                ErrorTaskStatus::Pending | ErrorTaskStatus::AutoRetrying
            )
    }

    /// 应用「已解决」终态要求。
    ///
    /// # 参数
    /// * `resolution_type` - 解决方式（不得为「关闭」）
    /// * `resolution` - 终态证据
    /// * `at` - 完成时间
    ///
    /// # 错误
    /// 解决方式缺失或为「关闭」、证据为空或超长时返回错误。
    fn apply_resolve(
        &mut self,
        resolution_type: Option<ResolutionType>,
        resolution: Option<String>,
        at: Instant,
    ) -> Result<()> {
        let resolution_type = match resolution_type {
            Some(ResolutionType::Close) => {
                return Err(Error::from("解决任务不得使用“关闭”解决方式，请走关闭迁移"));
            }
            Some(other) => other,
            None => return Err(Error::from("解决任务必须提供解决方式")),
        };
        let resolution = match resolution {
            Some(resolution) => resolution,
            None => return Err(Error::from("解决任务必须提供终态证据")),
        };
        let resolution = normalize_required_text(
            resolution,
            "解决任务必须提供终态证据",
            RESOLUTION_MAX_LEN,
            "解决证据过长",
        )?;
        self.resolution_type = Some(resolution_type);
        self.resolution = Some(resolution);
        self.resolved_at = Some(at);
        Ok(())
    }

    /// 应用「已关闭」终态要求。
    ///
    /// # 参数
    /// * `resolution_type` - 解决方式（必须为「关闭」）
    /// * `resolution` - 替代任务或终态证据
    /// * `at` - 完成时间
    ///
    /// # 错误
    /// 解决方式不是「关闭」或证据为空、超长时返回错误。
    fn apply_close(
        &mut self,
        resolution_type: Option<ResolutionType>,
        resolution: Option<String>,
        at: Instant,
    ) -> Result<()> {
        if !matches!(resolution_type, Some(ResolutionType::Close)) {
            return Err(Error::from("关闭任务必须使用“关闭”解决方式"));
        }
        let resolution = match resolution {
            Some(resolution) => resolution,
            None => return Err(Error::from("关闭任务必须提供替代任务或终态证据")),
        };
        let resolution = normalize_required_text(
            resolution,
            "关闭任务必须提供替代任务或终态证据",
            RESOLUTION_MAX_LEN,
            "关闭证据过长",
        )?;
        self.resolution_type = Some(ResolutionType::Close);
        self.resolution = Some(resolution);
        self.resolved_at = Some(at);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ensure_transition, ErrorClass, ErrorTaskStatus, IntegrationErrorTask, IntegrationErrorTaskData,
        IntegrationErrorTaskUpdate, ResolutionType,
    };
    use crate::common::time::Instant;
    use crate::ids::{InboxMessageId, IntegrationErrorTaskId};

    const NOW: i64 = 1_700_000_000;

    fn task_data() -> IntegrationErrorTaskData {
        IntegrationErrorTaskData {
            message_id: Some(InboxMessageId::new("msg-1")),
            business_object_id: None,
            error_class: ErrorClass::TransientFailure,
            owner_role: Some(" ops ".to_string()),
            owner_user_id: None,
        }
    }

    fn task() -> IntegrationErrorTask {
        IntegrationErrorTask::new(IntegrationErrorTaskId::new("task-1"), task_data()).unwrap()
    }

    fn task_with_class(error_class: ErrorClass) -> IntegrationErrorTask {
        let data = IntegrationErrorTaskData {
            error_class,
            ..task_data()
        };
        IntegrationErrorTask::new(IntegrationErrorTaskId::new("task-2"), data).unwrap()
    }

    #[test]
    fn new_normalizes_fields_and_starts_pending() {
        let data = IntegrationErrorTaskData {
            message_id: None,
            business_object_id: Some(" so-2026-001 ".to_string()),
            owner_user_id: Some(" u-1 ".to_string()),
            ..task_data()
        };
        let created = IntegrationErrorTask::new(IntegrationErrorTaskId::new("task-3"), data).unwrap();

        assert_eq!(created.message_id, None);
        assert_eq!(created.business_object_id.as_deref(), Some("so-2026-001"));
        assert_eq!(created.owner_role.as_deref(), Some("ops"));
        assert_eq!(created.owner_user_id.as_deref(), Some("u-1"));
        assert_eq!(created.status, ErrorTaskStatus::Pending);
        assert_eq!(created.attempt_count, 0);
        assert!(created.last_attempt_at.is_none());
        assert!(created.resolution_type.is_none());
        assert!(created.resolved_at.is_none());
    }

    #[test]
    fn new_rejects_missing_message_and_object() {
        let data = IntegrationErrorTaskData {
            message_id: None,
            business_object_id: None,
            ..task_data()
        };
        assert!(IntegrationErrorTask::new(IntegrationErrorTaskId::new("task-4"), data).is_err());
    }

    #[test]
    fn new_rejects_overlong_fields() {
        let overlong_object = IntegrationErrorTaskData {
            business_object_id: Some("o".repeat(129)),
            ..task_data()
        };
        assert!(IntegrationErrorTask::new(IntegrationErrorTaskId::new("task-5"), overlong_object).is_err());

        let overlong_role = IntegrationErrorTaskData {
            owner_role: Some("r".repeat(65)),
            ..task_data()
        };
        assert!(IntegrationErrorTask::new(IntegrationErrorTaskId::new("task-6"), overlong_role).is_err());
    }

    #[test]
    fn update_applies_owner_fields_only() {
        let mut created = task();
        created
            .update(IntegrationErrorTaskUpdate {
                owner_role: Some("  finance ".to_string()),
                owner_user_id: Some("u-9".to_string()),
            })
            .unwrap();

        assert_eq!(created.owner_role.as_deref(), Some("finance"));
        assert_eq!(created.owner_user_id.as_deref(), Some("u-9"));
        assert_eq!(
            created.error_class,
            ErrorClass::TransientFailure,
            "错误分类是关键字段"
        );
        assert_eq!(created.message_id, Some(InboxMessageId::new("msg-1")));
    }

    #[test]
    fn record_attempt_increments_and_updates_summary() {
        let mut created = task();
        created
            .record_attempt(Instant::from_unix_secs(NOW), Some("  超时重试  ".to_string()))
            .unwrap();

        assert_eq!(created.attempt_count, 1);
        assert_eq!(created.last_attempt_at, Some(Instant::from_unix_secs(NOW)));
        assert_eq!(created.last_attempt_summary.as_deref(), Some("超时重试"));
    }

    #[test]
    fn record_attempt_rejected_when_terminal() {
        let mut created = task();
        created
            .transition(
                ErrorTaskStatus::Resolved,
                Some(ResolutionType::QueryConfirm),
                Some("查询确认原请求已成功".to_string()),
                Instant::from_unix_secs(NOW),
            )
            .unwrap();
        assert!(created
            .record_attempt(Instant::from_unix_secs(NOW), Some("retry".to_string()))
            .is_err());
    }

    #[test]
    fn status_machine_legal_edges_pass() {
        let cases = [
            (ErrorTaskStatus::Pending, ErrorTaskStatus::AutoRetrying),
            (ErrorTaskStatus::Pending, ErrorTaskStatus::ManualRequired),
            (ErrorTaskStatus::Pending, ErrorTaskStatus::Resolved),
            (ErrorTaskStatus::Pending, ErrorTaskStatus::Closed),
            (ErrorTaskStatus::AutoRetrying, ErrorTaskStatus::ManualRequired),
            (ErrorTaskStatus::AutoRetrying, ErrorTaskStatus::Resolved),
            (ErrorTaskStatus::AutoRetrying, ErrorTaskStatus::Closed),
            (ErrorTaskStatus::ManualRequired, ErrorTaskStatus::Resolved),
            (ErrorTaskStatus::ManualRequired, ErrorTaskStatus::Closed),
        ];
        for (from, to) in cases {
            assert!(
                ensure_transition(from, to).is_ok(),
                "合法迁移被拒：{from:?} → {to:?}"
            );
        }
    }

    #[test]
    fn status_machine_rejects_illegal_and_terminal_edges() {
        assert!(ensure_transition(ErrorTaskStatus::AutoRetrying, ErrorTaskStatus::Pending).is_err());
        assert!(ensure_transition(ErrorTaskStatus::ManualRequired, ErrorTaskStatus::Pending).is_err());
        assert!(ensure_transition(ErrorTaskStatus::ManualRequired, ErrorTaskStatus::AutoRetrying).is_err());

        assert!(ensure_transition(ErrorTaskStatus::Resolved, ErrorTaskStatus::Closed).is_err());
        assert!(ensure_transition(ErrorTaskStatus::Resolved, ErrorTaskStatus::ManualRequired).is_err());
        assert!(ensure_transition(ErrorTaskStatus::Closed, ErrorTaskStatus::Resolved).is_err());
        assert!(ensure_transition(ErrorTaskStatus::Closed, ErrorTaskStatus::Pending).is_err());
    }

    #[test]
    fn status_machine_idempotent_transitions_pass() {
        for state in [
            ErrorTaskStatus::Pending,
            ErrorTaskStatus::AutoRetrying,
            ErrorTaskStatus::ManualRequired,
            ErrorTaskStatus::Resolved,
            ErrorTaskStatus::Closed,
        ] {
            assert!(ensure_transition(state, state).is_ok(), "幂等迁移被拒：{state:?}");
        }
    }

    #[test]
    fn transition_to_manual_required_succeeds_without_resolution() {
        let mut created = task();
        created
            .transition(
                ErrorTaskStatus::ManualRequired,
                None,
                None,
                Instant::from_unix_secs(NOW),
            )
            .unwrap();
        assert_eq!(created.status, ErrorTaskStatus::ManualRequired);
        assert!(created.resolution_type.is_none());
    }

    #[test]
    fn transition_rejects_resolution_on_non_terminal_move() {
        let mut created = task();
        let result = created.transition(
            ErrorTaskStatus::AutoRetrying,
            Some(ResolutionType::Replay),
            Some("证据".to_string()),
            Instant::from_unix_secs(NOW),
        );
        assert!(result.is_err());
        assert_eq!(created.status, ErrorTaskStatus::Pending, "失败迁移不得改变状态");
    }

    #[test]
    fn resolve_requires_reviewed_evidence_and_non_close_type() {
        let mut created = task();
        created
            .transition(
                ErrorTaskStatus::Resolved,
                Some(ResolutionType::FixMapping),
                Some(" 修复映射并完成对账 ".to_string()),
                Instant::from_unix_secs(NOW),
            )
            .unwrap();

        assert_eq!(created.status, ErrorTaskStatus::Resolved);
        assert_eq!(created.resolution_type, Some(ResolutionType::FixMapping));
        assert_eq!(created.resolution.as_deref(), Some("修复映射并完成对账"));
        assert_eq!(created.resolved_at, Some(Instant::from_unix_secs(NOW)));

        let mut no_evidence = task();
        assert!(no_evidence
            .transition(
                ErrorTaskStatus::Resolved,
                Some(ResolutionType::QueryConfirm),
                None,
                Instant::from_unix_secs(NOW),
            )
            .is_err());

        let mut no_type = task();
        assert!(no_type
            .transition(
                ErrorTaskStatus::Resolved,
                None,
                Some("证据".to_string()),
                Instant::from_unix_secs(NOW),
            )
            .is_err());

        let mut close_as_resolve = task();
        assert!(close_as_resolve
            .transition(
                ErrorTaskStatus::Resolved,
                Some(ResolutionType::Close),
                Some("证据".to_string()),
                Instant::from_unix_secs(NOW),
            )
            .is_err());
    }

    #[test]
    fn close_requires_evidence_and_accepts_controlled_result_unknown_evidence() {
        let mut created = task();
        created
            .transition(
                ErrorTaskStatus::Closed,
                Some(ResolutionType::Close),
                Some("重复任务，关联替代任务 task-9".to_string()),
                Instant::from_unix_secs(NOW),
            )
            .unwrap();
        assert_eq!(created.status, ErrorTaskStatus::Closed);
        assert_eq!(created.resolution_type, Some(ResolutionType::Close));
        assert!(created.is_terminal());

        let mut no_evidence = task();
        assert!(no_evidence
            .transition(
                ErrorTaskStatus::Closed,
                Some(ResolutionType::Close),
                None,
                Instant::from_unix_secs(NOW)
            )
            .is_err());

        let mut wrong_type = task();
        assert!(wrong_type
            .transition(
                ErrorTaskStatus::Closed,
                Some(ResolutionType::Compensate),
                Some("证据".to_string()),
                Instant::from_unix_secs(NOW),
            )
            .is_err());

        let mut result_unknown = task_with_class(ErrorClass::ResultUnknown);
        result_unknown
            .transition(
                ErrorTaskStatus::Closed,
                Some(ResolutionType::Close),
                Some("替代任务".to_string()),
                Instant::from_unix_secs(NOW),
            )
            .unwrap();
        assert_eq!(result_unknown.status, ErrorTaskStatus::Closed);
    }

    #[test]
    fn terminal_status_blocks_further_transitions() {
        let mut created = task();
        created
            .transition(
                ErrorTaskStatus::Closed,
                Some(ResolutionType::Close),
                Some("重复任务".to_string()),
                Instant::from_unix_secs(NOW),
            )
            .unwrap();
        assert!(created
            .transition(
                ErrorTaskStatus::Resolved,
                Some(ResolutionType::QueryConfirm),
                Some("证据".to_string()),
                Instant::from_unix_secs(NOW),
            )
            .is_err());
    }

    #[test]
    fn auto_retry_policy_follows_error_class_and_status() {
        for class in [
            ErrorClass::CapabilityGap,
            ErrorClass::MappingError,
            ErrorClass::BusinessRejected,
            ErrorClass::ResultUnknown,
            ErrorClass::AuthSignature,
            ErrorClass::OutOfOrder,
        ] {
            assert!(!class.can_auto_retry(), "不可自动重试分类被放行：{class:?}");
        }
        assert!(ErrorClass::TransientFailure.can_auto_retry());
        assert!(ErrorClass::RateLimited.can_auto_retry());

        assert!(task().can_auto_retry(), "待处理 + 临时故障应允许自动重试");
        let mut manual = task();
        manual
            .transition(
                ErrorTaskStatus::ManualRequired,
                None,
                None,
                Instant::from_unix_secs(NOW),
            )
            .unwrap();
        assert!(!manual.can_auto_retry(), "转人工后不再自动重试");
        assert!(!task_with_class(ErrorClass::MappingError).can_auto_retry());
    }

    #[test]
    fn replay_requires_server_confirmed_no_result_and_allowed_class() {
        let mut retryable = task_with_class(ErrorClass::ResultUnknown);
        retryable.last_attempt_summary =
            Some("w29_action=QUERY_ORIGINAL_RESULT;outcome=NoResultConfirmed".to_string());
        assert!(retryable.can_replay_original());
        assert!(retryable.has_subject_version(" 1 "));

        let mut mapping = task_with_class(ErrorClass::MappingError);
        mapping.last_attempt_summary = retryable.last_attempt_summary.clone();
        assert!(!mapping.can_replay_original());
        assert!(!mapping.has_subject_version("2"));
    }

    #[test]
    fn resolution_type_is_derived_from_verified_evidence() {
        assert_eq!(
            ResolutionType::from_verified_evidence(true, true),
            ResolutionType::Compensate
        );
        assert_eq!(
            ResolutionType::from_verified_evidence(false, true),
            ResolutionType::FixMapping
        );
        assert_eq!(
            ResolutionType::from_verified_evidence(false, false),
            ResolutionType::QueryConfirm
        );
    }

    #[test]
    fn enums_serialize_with_stable_codes_and_expose_labels() {
        assert_eq!(
            serde_json::to_string(&ErrorClass::ResultUnknown).unwrap(),
            "\"result_unknown\""
        );
        assert_eq!(
            serde_json::to_string(&ErrorTaskStatus::AutoRetrying).unwrap(),
            "\"auto_retrying\""
        );
        assert_eq!(
            serde_json::to_string(&ResolutionType::FixMapping).unwrap(),
            "\"fix_mapping\""
        );

        assert_eq!(ErrorClass::AuthSignature.label(), "鉴权签名");
        assert_eq!(ErrorTaskStatus::ManualRequired.label(), "待人工");
        assert_eq!(ResolutionType::Replay.label(), "重放");
        assert_eq!(ErrorTaskStatus::Closed.as_str(), "closed");
    }

    #[test]
    fn entity_roundtrip_through_bson() {
        let created = task();
        let roundtrip: IntegrationErrorTask =
            bson::deserialize_from_document(bson::serialize_to_document(&created).unwrap()).unwrap();
        assert_eq!(roundtrip, created);
    }
}
