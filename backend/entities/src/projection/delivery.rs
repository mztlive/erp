//! `sales_order_projection_delivery`：执行投影下发记录（数据模型 §6.16，页面 W23）。
//!
//! 投递记录持有稳定消息身份，并以显式状态迁移约束发送、结果未知、重试与
//! 转人工。`integration_error_task.status` 只表达 W29 人工任务处理状态，不得
//! 代替本对象的外部投递事实。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{
    InboxMessageId, IntegrationErrorTaskId, SalesOrderProjectionDeliveryId, SalesOrderProjectionRevisionId,
    SourceSystemId, WorkItemId,
};
use crate::integration_ops::ErrorClass;
use crate::validation::{normalize_optional_text, normalize_required_text};

/// 稳定消息键最大长度。
const MESSAGE_KEY_MAX_LEN: usize = 256;

/// 商城执行基线最大长度。
const EXECUTION_BASELINE_MAX_LEN: usize = 256;
/// 错误码最大长度。
const ERROR_CODE_MAX_LEN: usize = 128;
/// 错误摘要最大长度。
const ERROR_SUMMARY_MAX_LEN: usize = 1024;

/// 投影下发状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionDeliveryStatus {
    /// 待发送。
    #[default]
    PendingSend,
    /// 发送中。
    Sending,
    /// 重试中。
    Retrying,
    /// 外部请求可能已受理，但最终结果尚无法确认。
    ResultUnknown,
    /// 已确认。
    Confirmed,
    /// 失败。
    Failed,
    /// 转人工。
    Manual,
}

impl ProjectionDeliveryStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::PendingSend => "待发送",
            Self::Sending => "发送中",
            Self::Retrying => "重试中",
            Self::ResultUnknown => "结果未知",
            Self::Confirmed => "已确认",
            Self::Failed => "失败",
            Self::Manual => "转人工",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PendingSend => "pending_send",
            Self::Sending => "sending",
            Self::Retrying => "retrying",
            Self::ResultUnknown => "result_unknown",
            Self::Confirmed => "confirmed",
            Self::Failed => "failed",
            Self::Manual => "manual",
        }
    }

    /// 判断当前状态是否可以进入目标状态。
    ///
    /// 同状态用于幂等读取；终态确认与已转人工状态不得由普通投递入口回退。
    pub fn can_transition_to(self, target: Self) -> bool {
        if self == target {
            return true;
        }
        match self {
            Self::PendingSend => target == Self::Sending,
            Self::Sending => matches!(
                target,
                Self::Confirmed | Self::Failed | Self::ResultUnknown | Self::Manual
            ),
            Self::Retrying => matches!(target, Self::Sending | Self::Manual),
            Self::ResultUnknown => matches!(target, Self::Confirmed | Self::Failed | Self::Manual),
            Self::Failed => matches!(target, Self::Retrying | Self::Manual),
            Self::Confirmed | Self::Manual => false,
        }
    }
}

/// 投影下发创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesOrderProjectionDeliveryData {
    /// 待下发投影版本。
    pub projection_revision_id: SalesOrderProjectionRevisionId,
    /// 目标商城（来源系统，类型 MALL）。
    pub target_mall_id: SourceSystemId,
    /// 下发状态。
    pub status: ProjectionDeliveryStatus,
    /// 发送次数。
    pub attempt_count: u32,
    /// 下次重试时间；待发送/发送中不得安排重试。
    pub next_attempt_at: Option<Instant>,
    /// 商城确认时间；与 `mall_execution_baseline` 必须成对出现。
    pub mall_ack_at: Option<Instant>,
    /// 商城执行基线；与 `mall_ack_at` 必须成对出现。
    pub mall_execution_baseline: Option<String>,
    /// 错误码；与 `error_summary` 必须成对出现。
    pub error_code: Option<String>,
    /// 错误摘要；与 `error_code` 必须成对出现。
    pub error_summary: Option<String>,
}

/// 投影下发更新数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SalesOrderProjectionDeliveryUpdate {
    /// 下发状态；`None` 表示不修改。
    pub status: Option<ProjectionDeliveryStatus>,
    /// 发送次数；`None` 表示不修改。
    pub attempt_count: Option<u32>,
    /// 下次重试时间；`None` 表示不修改。
    pub next_attempt_at: Option<Option<Instant>>,
    /// 最近发送时间；外层 `None` 表示不修改。
    pub last_attempt_at: Option<Option<Instant>>,
    /// 商城确认时间；`None` 表示不修改。
    pub mall_ack_at: Option<Option<Instant>>,
    /// 商城执行基线；`None` 表示不修改。
    pub mall_execution_baseline: Option<Option<String>>,
    /// 错误分类；外层 `None` 表示不修改。
    pub error_class: Option<Option<ErrorClass>>,
    /// 错误码；`None` 表示不修改。
    pub error_code: Option<Option<String>>,
    /// 错误摘要；`None` 表示不修改。
    pub error_summary: Option<Option<String>>,
    /// 关联消息；外层 `None` 表示不修改。
    pub inbox_message_id: Option<Option<InboxMessageId>>,
    /// W29 错误对象；外层 `None` 表示不修改。
    pub error_task_id: Option<Option<IntegrationErrorTaskId>>,
    /// W29 正式待办；外层 `None` 表示不修改。
    pub work_item_id: Option<Option<WorkItemId>>,
}

/// 执行投影下发实体（数据模型 §6.16）。
///
/// 按字典精确建模，只用 `BaseModel` 承载持久化元数据；`projection_revision_id`
/// 与 `target_mall_id` 是稳定键。商城确认前新单不得开始受该版影响的执行，
/// 属于跨聚合校验（§6.16、phase-2 §8.2，P3）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SalesOrderProjectionDelivery {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 待下发投影版本。
    pub projection_revision_id: SalesOrderProjectionRevisionId,
    /// 目标商城（来源系统，类型 MALL）。
    pub target_mall_id: SourceSystemId,
    /// 跨全部尝试保持不变的外部消息身份。
    pub message_key: String,
    /// 下发状态。
    pub status: ProjectionDeliveryStatus,
    /// 发送次数。
    pub attempt_count: u32,
    /// 最近一次真实发送时间。
    pub last_attempt_at: Option<Instant>,
    /// 下次重试时间。
    pub next_attempt_at: Option<Instant>,
    /// 商城确认时间。
    pub mall_ack_at: Option<Instant>,
    /// 商城执行基线。
    pub mall_execution_baseline: Option<String>,
    /// 最近失败或结果未知的稳定错误分类。
    pub error_class: Option<ErrorClass>,
    /// 错误码。
    pub error_code: Option<String>,
    /// 错误摘要。
    pub error_summary: Option<String>,
    /// 承接稳定消息身份的消息信封。
    pub inbox_message_id: Option<InboxMessageId>,
    /// 升级后关联的 W29 错误对象。
    pub error_task_id: Option<IntegrationErrorTaskId>,
    /// 升级后关联的正式待办。
    pub work_item_id: Option<WorkItemId>,
}

impl SalesOrderProjectionDelivery {
    /// 创建执行投影下发记录。
    ///
    /// 完成执行基线/错误信息的校验与规范化，并强制记录不变式：待发送/发送中
    /// 不得安排重试时间、商城确认时间与执行基线成对、错误码与摘要成对、状态与
    /// 结果字段一致（已确认必须有商城确认，失败必须有错误码；确认信息只属于
    /// 已确认，错误信息只属于失败或转人工）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SalesOrderProjectionDeliveryId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的下发记录实体。
    ///
    /// # 错误
    /// 当文本超长或上述记录不变式被违反时返回错误。
    pub fn new(id: SalesOrderProjectionDeliveryId, data: SalesOrderProjectionDeliveryData) -> Result<Self> {
        let message_key = normalize_required_text(
            format!(
                "projection_delivery:{}:{}",
                data.projection_revision_id, data.target_mall_id
            ),
            "投递消息键不能为空",
            MESSAGE_KEY_MAX_LEN,
            "投递消息键过长",
        )?;
        let mall_execution_baseline = normalize_optional_text(
            data.mall_execution_baseline,
            "商城执行基线",
            EXECUTION_BASELINE_MAX_LEN,
        )?;
        let error_code = normalize_optional_text(data.error_code, "错误码", ERROR_CODE_MAX_LEN)?;
        let error_summary = normalize_optional_text(data.error_summary, "错误摘要", ERROR_SUMMARY_MAX_LEN)?;
        validate_delivery_state(&ProjectionDeliveryState {
            status: data.status,
            attempt_count: data.attempt_count,
            last_attempt_at: None,
            next_attempt_at: data.next_attempt_at,
            mall_ack_at: data.mall_ack_at,
            mall_execution_baseline: mall_execution_baseline.as_deref(),
            error_class: None,
            error_code: error_code.as_deref(),
            error_summary: error_summary.as_deref(),
            error_task_id: None,
            work_item_id: None,
        })?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            projection_revision_id: data.projection_revision_id,
            target_mall_id: data.target_mall_id,
            message_key,
            status: data.status,
            attempt_count: data.attempt_count,
            last_attempt_at: None,
            next_attempt_at: data.next_attempt_at,
            mall_ack_at: data.mall_ack_at,
            mall_execution_baseline,
            error_class: None,
            error_code,
            error_summary,
            inbox_message_id: None,
            error_task_id: None,
            work_item_id: None,
        })
    }

    /// 更新执行投影下发记录。
    ///
    /// 复用 `new` 的校验规则；`projection_revision_id`/`target_mall_id` 是
    /// 稳定键，不允许在通用更新中修改。更新按合并后的完整状态校验，避免
    /// 分步更新产生记录不一致。
    ///
    /// # 参数
    /// * `update` - 更新数据
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当更新字段校验失败或合并后违反记录不变式时返回错误。
    pub fn update(&mut self, update: SalesOrderProjectionDeliveryUpdate) -> Result<()> {
        let status = update.status.unwrap_or(self.status);
        if !self.status.can_transition_to(status) {
            return Err(Error::from("非法的投递状态迁移"));
        }
        let attempt_count = update.attempt_count.unwrap_or(self.attempt_count);
        let last_attempt_at = update.last_attempt_at.unwrap_or(self.last_attempt_at);
        let next_attempt_at = update.next_attempt_at.unwrap_or(self.next_attempt_at);
        let mall_ack_at = update.mall_ack_at.unwrap_or(self.mall_ack_at);
        let mall_execution_baseline = match update.mall_execution_baseline {
            Some(value) => normalize_optional_text(value, "商城执行基线", EXECUTION_BASELINE_MAX_LEN)?,
            None => self.mall_execution_baseline.clone(),
        };
        let error_class = update.error_class.unwrap_or(self.error_class);
        let error_code = match update.error_code {
            Some(value) => normalize_optional_text(value, "错误码", ERROR_CODE_MAX_LEN)?,
            None => self.error_code.clone(),
        };
        let error_summary = match update.error_summary {
            Some(value) => normalize_optional_text(value, "错误摘要", ERROR_SUMMARY_MAX_LEN)?,
            None => self.error_summary.clone(),
        };
        let inbox_message_id = update
            .inbox_message_id
            .unwrap_or_else(|| self.inbox_message_id.clone());
        let error_task_id = update.error_task_id.unwrap_or_else(|| self.error_task_id.clone());
        let work_item_id = update.work_item_id.unwrap_or_else(|| self.work_item_id.clone());
        validate_delivery_state(&ProjectionDeliveryState {
            status,
            attempt_count,
            last_attempt_at,
            next_attempt_at,
            mall_ack_at,
            mall_execution_baseline: mall_execution_baseline.as_deref(),
            error_class,
            error_code: error_code.as_deref(),
            error_summary: error_summary.as_deref(),
            error_task_id: error_task_id.as_ref(),
            work_item_id: work_item_id.as_ref(),
        })?;

        self.status = status;
        self.attempt_count = attempt_count;
        self.last_attempt_at = last_attempt_at;
        self.next_attempt_at = next_attempt_at;
        self.mall_ack_at = mall_ack_at;
        self.mall_execution_baseline = mall_execution_baseline;
        self.error_class = error_class;
        self.error_code = error_code;
        self.error_summary = error_summary;
        self.inbox_message_id = inbox_message_id;
        self.error_task_id = error_task_id;
        self.work_item_id = work_item_id;
        Ok(())
    }

    /// 判断该投递是否可由受控发送入口原子取得。
    pub fn is_send_ready(&self, now: Instant) -> bool {
        self.status == ProjectionDeliveryStatus::PendingSend
            || (self.status == ProjectionDeliveryStatus::Retrying
                && self
                    .next_attempt_at
                    .is_some_and(|at| at.unix_secs() <= now.unix_secs()))
    }

    /// 判断当前事实是否允许查询原请求结果。
    pub fn can_query_result(&self) -> bool {
        matches!(
            self.status,
            ProjectionDeliveryStatus::Sending
                | ProjectionDeliveryStatus::ResultUnknown
                | ProjectionDeliveryStatus::Failed
        ) && self.error_class != Some(ErrorClass::MappingError)
    }

    /// 判断当前事实是否允许沿原消息身份安排重试。
    pub fn can_retry(&self) -> bool {
        self.status == ProjectionDeliveryStatus::Failed
            && self.error_class.is_some_and(|class| class.can_auto_retry())
    }

    /// 判断当前事实是否允许升级为 W29 正式人工任务。
    pub fn can_escalate(&self) -> bool {
        matches!(
            self.status,
            ProjectionDeliveryStatus::Sending
                | ProjectionDeliveryStatus::ResultUnknown
                | ProjectionDeliveryStatus::Failed
        )
    }

    /// 校验投递与命令的身份及版本一致性。
    ///
    /// 用于强命令入口的版本乐观锁校验，确保调用方基于最新投递事实提交命令，
    /// 避免对过时版本的误操作或对错误修订的交叉写入。
    ///
    /// # 参数
    /// * `expected_version` - 命令携带的期望投递版本
    /// * `expected_revision_id` - 命令携带的期望投影修订身份
    ///
    /// # 返回
    /// 一致时返回 `Ok(())`。
    ///
    /// # 错误
    /// 投递版本或修订身份不一致时返回错误，调用方应映射为 409 Conflict。
    ///
    /// # 关键业务约束
    /// 该方法不触及持久化或外部状态；仅比较已加载投递事实与命令身份。
    pub fn ensure_matches_command(
        &self,
        expected_version: u64,
        expected_revision_id: &SalesOrderProjectionRevisionId,
    ) -> Result<()> {
        if self.base.version != expected_version || &self.projection_revision_id != expected_revision_id {
            return Err(Error::from("投递对象版本或修订身份已变化"));
        }
        Ok(())
    }

    /// 校验投递命令的路径投递身份与命令投递身份一致性。
    ///
    /// 防止路径参数与命令体携带的投递身份不一致导致的误操作，确保强命令
    /// 仅作用于路径指定的投递对象。
    ///
    /// # 参数
    /// * `path_delivery_id` - HTTP 路径中的投递身份
    /// * `command_delivery_id` - 命令体中的投递身份
    ///
    /// # 返回
    /// 一致时返回 `Ok(())`。
    ///
    /// # 错误
    /// 两者不一致时返回错误，调用方应映射为 400 Validation。
    ///
    /// # 关键业务约束
    /// 该方法为纯身份比较，不触及持久化或外部状态。
    pub fn ensure_command_identity(
        path_delivery_id: &SalesOrderProjectionDeliveryId,
        command_delivery_id: &SalesOrderProjectionDeliveryId,
    ) -> Result<()> {
        if path_delivery_id != command_delivery_id {
            return Err(Error::from("路径投递ID与命令不一致"));
        }
        Ok(())
    }
}

/// 校验下发记录不变式（重试安排、成对字段与状态一致性）。
///
/// # 参数
/// * `status` - 下发状态
/// * `next_attempt_at` - 下次重试时间
/// * `mall_ack_at` - 商城确认时间
/// * `mall_execution_baseline` - 商城执行基线
/// * `error_code` - 错误码
/// * `error_summary` - 错误摘要
///
/// # 错误
/// 当成对字段只有一边出现或状态与结果字段不一致时返回错误。
struct ProjectionDeliveryState<'a> {
    status: ProjectionDeliveryStatus,
    attempt_count: u32,
    last_attempt_at: Option<Instant>,
    next_attempt_at: Option<Instant>,
    mall_ack_at: Option<Instant>,
    mall_execution_baseline: Option<&'a str>,
    error_class: Option<ErrorClass>,
    error_code: Option<&'a str>,
    error_summary: Option<&'a str>,
    error_task_id: Option<&'a IntegrationErrorTaskId>,
    work_item_id: Option<&'a WorkItemId>,
}

fn validate_delivery_state(state: &ProjectionDeliveryState<'_>) -> Result<()> {
    if (state.attempt_count == 0) != state.last_attempt_at.is_none() {
        return Err(Error::from("发送次数与最近发送时间必须同时存在或同时为空"));
    }
    if state.status == ProjectionDeliveryStatus::PendingSend && state.attempt_count != 0 {
        return Err(Error::from("待发送记录的发送次数必须为零"));
    }
    if state.status != ProjectionDeliveryStatus::PendingSend && state.attempt_count == 0 {
        return Err(Error::from("已开始处理的投递必须至少记录一次发送"));
    }
    if (state.status == ProjectionDeliveryStatus::Retrying) != state.next_attempt_at.is_some() {
        return Err(Error::from("只有重试中记录必须安排下一次处理时间"));
    }
    if state.mall_ack_at.is_some() != state.mall_execution_baseline.is_some() {
        return Err(Error::from("商城确认时间与商城执行基线必须同时提供或同时省略"));
    }
    if state.error_class.is_some() != state.error_code.is_some()
        || state.error_code.is_some() != state.error_summary.is_some()
    {
        return Err(Error::from("错误分类、错误码与错误摘要必须同时提供或同时省略"));
    }
    if state.status == ProjectionDeliveryStatus::Confirmed && state.mall_ack_at.is_none() {
        return Err(Error::from("已确认下发记录必须记录商城确认信息"));
    }
    if matches!(
        state.status,
        ProjectionDeliveryStatus::Failed
            | ProjectionDeliveryStatus::ResultUnknown
            | ProjectionDeliveryStatus::Retrying
            | ProjectionDeliveryStatus::Manual
    ) && state.error_code.is_none()
    {
        return Err(Error::from("失败、结果未知、重试或转人工记录必须保留错误信息"));
    }
    if state.status == ProjectionDeliveryStatus::ResultUnknown
        && state.error_class != Some(ErrorClass::ResultUnknown)
    {
        return Err(Error::from("结果未知记录必须使用结果未知错误分类"));
    }
    if state.mall_ack_at.is_some() && state.status != ProjectionDeliveryStatus::Confirmed {
        return Err(Error::from("只有已确认下发记录才能记录商城确认信息"));
    }
    if state.status == ProjectionDeliveryStatus::Confirmed && state.error_code.is_some() {
        return Err(Error::from("已确认记录不得保留错误信息"));
    }
    if state.error_task_id.is_some() != state.work_item_id.is_some() {
        return Err(Error::from("W29 错误对象与正式待办必须同时关联或同时为空"));
    }
    if state.status == ProjectionDeliveryStatus::Manual && state.error_task_id.is_none() {
        return Err(Error::from("转人工记录必须关联 W29 错误对象与正式待办"));
    }
    if state.status != ProjectionDeliveryStatus::Manual && state.error_task_id.is_some() {
        return Err(Error::from("只有转人工记录才能关联 W29 错误对象与正式待办"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        ProjectionDeliveryStatus, SalesOrderProjectionDelivery, SalesOrderProjectionDeliveryData,
        SalesOrderProjectionDeliveryUpdate,
    };
    use crate::common::time::Instant;
    use crate::ids::{
        IntegrationErrorTaskId, SalesOrderProjectionDeliveryId, SalesOrderProjectionRevisionId,
        SourceSystemId, WorkItemId,
    };
    use crate::integration_ops::ErrorClass;

    fn delivery_data() -> SalesOrderProjectionDeliveryData {
        SalesOrderProjectionDeliveryData {
            projection_revision_id: SalesOrderProjectionRevisionId::new("proj-rev-1"),
            target_mall_id: SourceSystemId::new("mall-1"),
            status: ProjectionDeliveryStatus::PendingSend,
            attempt_count: 0,
            next_attempt_at: None,
            mall_ack_at: None,
            mall_execution_baseline: None,
            error_code: None,
            error_summary: None,
        }
    }

    #[test]
    fn delivery_new_builds_pending_send_record() {
        let delivery =
            SalesOrderProjectionDelivery::new(SalesOrderProjectionDeliveryId::new("del-1"), delivery_data())
                .unwrap();

        assert_eq!(delivery.status, ProjectionDeliveryStatus::PendingSend);
        assert_eq!(delivery.attempt_count, 0);
        assert_eq!(
            delivery.projection_revision_id,
            SalesOrderProjectionRevisionId::new("proj-rev-1")
        );
        assert_eq!(delivery.target_mall_id, SourceSystemId::new("mall-1"));
    }

    #[test]
    fn delivery_new_only_accepts_initial_pending_state_and_builds_stable_message_key() {
        let delivery =
            SalesOrderProjectionDelivery::new(SalesOrderProjectionDeliveryId::new("del-2"), delivery_data())
                .unwrap();
        assert_eq!(delivery.message_key, "projection_delivery:proj-rev-1:mall-1");

        let sending = SalesOrderProjectionDeliveryData {
            status: ProjectionDeliveryStatus::Sending,
            attempt_count: 1,
            ..delivery_data()
        };
        assert!(
            SalesOrderProjectionDelivery::new(SalesOrderProjectionDeliveryId::new("del-3"), sending).is_err()
        );
    }

    #[test]
    fn delivery_new_rejects_overlong_text_fields() {
        let overlong_baseline = SalesOrderProjectionDeliveryData {
            mall_execution_baseline: Some("b".repeat(257)),
            ..delivery_data()
        };
        assert!(SalesOrderProjectionDelivery::new(
            SalesOrderProjectionDeliveryId::new("del-4"),
            overlong_baseline
        )
        .is_err());

        let overlong_code = SalesOrderProjectionDeliveryData {
            error_code: Some("e".repeat(129)),
            error_summary: Some("s".to_string()),
            ..delivery_data()
        };
        assert!(SalesOrderProjectionDelivery::new(
            SalesOrderProjectionDeliveryId::new("del-5"),
            overlong_code
        )
        .is_err());
    }

    #[test]
    fn delivery_new_rejects_half_pairs_and_inconsistent_status_fields() {
        let pending_with_retry = SalesOrderProjectionDeliveryData {
            next_attempt_at: Some(Instant::from_unix_secs(1_700_000_300)),
            ..delivery_data()
        };
        assert!(SalesOrderProjectionDelivery::new(
            SalesOrderProjectionDeliveryId::new("del-6"),
            pending_with_retry
        )
        .is_err());

        let sending_with_retry = SalesOrderProjectionDeliveryData {
            status: ProjectionDeliveryStatus::Sending,
            next_attempt_at: Some(Instant::from_unix_secs(1_700_000_300)),
            ..delivery_data()
        };
        assert!(SalesOrderProjectionDelivery::new(
            SalesOrderProjectionDeliveryId::new("del-7"),
            sending_with_retry
        )
        .is_err());

        let half_ack = SalesOrderProjectionDeliveryData {
            status: ProjectionDeliveryStatus::Confirmed,
            attempt_count: 1,
            mall_ack_at: Some(Instant::from_unix_secs(1_700_000_400)),
            mall_execution_baseline: None,
            ..delivery_data()
        };
        assert!(
            SalesOrderProjectionDelivery::new(SalesOrderProjectionDeliveryId::new("del-8"), half_ack)
                .is_err()
        );

        let confirmed_without_ack = SalesOrderProjectionDeliveryData {
            status: ProjectionDeliveryStatus::Confirmed,
            attempt_count: 1,
            mall_ack_at: None,
            mall_execution_baseline: None,
            ..delivery_data()
        };
        assert!(SalesOrderProjectionDelivery::new(
            SalesOrderProjectionDeliveryId::new("del-9"),
            confirmed_without_ack
        )
        .is_err());

        let failed_without_error = SalesOrderProjectionDeliveryData {
            status: ProjectionDeliveryStatus::Failed,
            attempt_count: 1,
            error_code: None,
            error_summary: None,
            ..delivery_data()
        };
        assert!(SalesOrderProjectionDelivery::new(
            SalesOrderProjectionDeliveryId::new("del-10"),
            failed_without_error
        )
        .is_err());

        let retrying_with_ack = SalesOrderProjectionDeliveryData {
            status: ProjectionDeliveryStatus::Retrying,
            attempt_count: 1,
            mall_ack_at: Some(Instant::from_unix_secs(1_700_000_400)),
            mall_execution_baseline: Some("baseline-1".to_string()),
            ..delivery_data()
        };
        assert!(SalesOrderProjectionDelivery::new(
            SalesOrderProjectionDeliveryId::new("del-11"),
            retrying_with_ack
        )
        .is_err());
    }

    #[test]
    fn delivery_update_merges_state_and_validates_combined_record() {
        let mut delivery =
            SalesOrderProjectionDelivery::new(SalesOrderProjectionDeliveryId::new("del-1"), delivery_data())
                .unwrap();

        delivery
            .update(SalesOrderProjectionDeliveryUpdate {
                status: Some(ProjectionDeliveryStatus::Sending),
                attempt_count: Some(1),
                last_attempt_at: Some(Some(Instant::from_unix_secs(1_700_000_000))),
                ..Default::default()
            })
            .unwrap();
        delivery
            .update(SalesOrderProjectionDeliveryUpdate {
                status: Some(ProjectionDeliveryStatus::Failed),
                error_class: Some(Some(ErrorClass::TransientFailure)),
                error_code: Some(Some(" 502 ".to_string())),
                error_summary: Some(Some(" 商城无响应 ".to_string())),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(delivery.status, ProjectionDeliveryStatus::Failed);
        assert_eq!(delivery.error_code.as_deref(), Some("502"));
        assert_eq!(delivery.attempt_count, 1);
        assert!(delivery.can_retry());

        let invalid = SalesOrderProjectionDeliveryUpdate {
            status: Some(ProjectionDeliveryStatus::Confirmed),
            ..Default::default()
        };
        assert!(delivery.update(invalid).is_err(), "合并后仍缺商城确认信息");
        assert_eq!(
            delivery.status,
            ProjectionDeliveryStatus::Failed,
            "失败更新不改变实体"
        );
    }

    #[test]
    fn result_unknown_only_allows_query_or_escalate_and_manual_requires_w29_links() {
        let mut delivery =
            SalesOrderProjectionDelivery::new(SalesOrderProjectionDeliveryId::new("del-12"), delivery_data())
                .unwrap();
        delivery
            .update(SalesOrderProjectionDeliveryUpdate {
                status: Some(ProjectionDeliveryStatus::Sending),
                attempt_count: Some(1),
                last_attempt_at: Some(Some(Instant::from_unix_secs(1_700_000_000))),
                ..Default::default()
            })
            .unwrap();
        delivery
            .update(SalesOrderProjectionDeliveryUpdate {
                status: Some(ProjectionDeliveryStatus::ResultUnknown),
                error_class: Some(Some(ErrorClass::ResultUnknown)),
                error_code: Some(Some("MALL_TIMEOUT".to_string())),
                error_summary: Some(Some("商城结果待查询".to_string())),
                ..Default::default()
            })
            .unwrap();

        assert!(delivery.can_query_result());
        assert!(!delivery.can_retry());
        assert!(delivery.can_escalate());
        assert!(delivery
            .update(SalesOrderProjectionDeliveryUpdate {
                status: Some(ProjectionDeliveryStatus::Manual),
                ..Default::default()
            })
            .is_err());
        delivery
            .update(SalesOrderProjectionDeliveryUpdate {
                status: Some(ProjectionDeliveryStatus::Manual),
                error_task_id: Some(Some(IntegrationErrorTaskId::new("task-1"))),
                work_item_id: Some(Some(WorkItemId::new("work-1"))),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(delivery.status, ProjectionDeliveryStatus::Manual);
    }

    #[test]
    fn delivery_status_serializes_with_stable_codes_and_exposes_labels() {
        assert_eq!(
            serde_json::to_string(&ProjectionDeliveryStatus::Sending).unwrap(),
            "\"sending\""
        );
        assert_eq!(ProjectionDeliveryStatus::Manual.label(), "转人工");
        assert_eq!(ProjectionDeliveryStatus::Retrying.as_str(), "retrying");
        assert_eq!(
            serde_json::to_string(&ProjectionDeliveryStatus::ResultUnknown).unwrap(),
            "\"result_unknown\""
        );
    }
}
