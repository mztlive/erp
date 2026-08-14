//! `product_publication_delivery`：发布投递记录（数据模型 §6.15，页面 W22）。
//!
//! 投递记录持有跨全部尝试不变的消息身份，并以显式状态迁移约束发送、结果未知、
//! 重试与转人工。W29 任务状态只表达人工处理进度，不得替代商城投递事实。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{
    InboxMessageId, IntegrationErrorTaskId, ProductPublicationDeliveryId, ProductPublicationRevisionId,
    SourceSystemId, WorkItemId,
};
use crate::integration_ops::ErrorClass;
use crate::validation::{normalize_optional_text, normalize_required_text};

/// 稳定消息键最大长度。
const MESSAGE_KEY_MAX_LEN: usize = 256;

/// 商城确认版本最大长度。
const MALL_VERSION_MAX_LEN: usize = 128;
/// 错误码最大长度。
const ERROR_CODE_MAX_LEN: usize = 128;
/// 错误摘要最大长度。
const ERROR_SUMMARY_MAX_LEN: usize = 1024;

/// 发布投递状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicationDeliveryStatus {
    /// 待发送。
    #[default]
    PendingSend,
    /// 发送中，最终结果尚未落库。
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

impl PublicationDeliveryStatus {
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

/// 发布投递创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProductPublicationDeliveryData {
    /// 发布版本。
    pub publication_revision_id: ProductPublicationRevisionId,
    /// 目标商城（来源系统，类型 MALL）。
    pub target_mall_id: SourceSystemId,
    /// 投递状态。
    pub delivery_status: PublicationDeliveryStatus,
    /// 发送次数；与 `last_attempt_at` 必须成对出现。
    pub attempt_count: u32,
    /// 最近发送时间；与 `attempt_count` 必须成对出现。
    pub last_attempt_at: Option<Instant>,
    /// 下次受控处理时间；只允许重试中记录持有。
    pub next_attempt_at: Option<Instant>,
    /// 商城确认时间；与 `mall_version` 必须成对出现。
    pub mall_ack_at: Option<Instant>,
    /// 商城确认版本；与 `mall_ack_at` 必须成对出现。
    pub mall_version: Option<String>,
    /// 错误码；与 `error_summary` 必须成对出现。
    pub error_class: Option<ErrorClass>,
    /// 错误码；与 `error_summary` 必须成对出现。
    pub error_code: Option<String>,
    /// 错误摘要；与 `error_code` 必须成对出现。
    pub error_summary: Option<String>,
}

/// 发布投递更新数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ProductPublicationDeliveryUpdate {
    /// 投递状态；`None` 表示不修改。
    pub delivery_status: Option<PublicationDeliveryStatus>,
    /// 发送次数；`None` 表示不修改。
    pub attempt_count: Option<u32>,
    /// 最近发送时间；`None` 表示不修改。
    pub last_attempt_at: Option<Option<Instant>>,
    /// 下次受控处理时间；外层 `None` 表示不修改。
    pub next_attempt_at: Option<Option<Instant>>,
    /// 商城确认时间；`None` 表示不修改。
    pub mall_ack_at: Option<Option<Instant>>,
    /// 商城确认版本；`None` 表示不修改。
    pub mall_version: Option<Option<String>>,
    /// 错误分类；外层 `None` 表示不修改。
    pub error_class: Option<Option<ErrorClass>>,
    /// 错误码；`None` 表示不修改。
    pub error_code: Option<Option<String>>,
    /// 错误摘要；`None` 表示不修改。
    pub error_summary: Option<Option<String>>,
    /// 原消息信封；外层 `None` 表示不修改。
    pub inbox_message_id: Option<Option<InboxMessageId>>,
    /// W29 错误对象；外层 `None` 表示不修改。
    pub error_task_id: Option<Option<IntegrationErrorTaskId>>,
    /// W29 正式待办；外层 `None` 表示不修改。
    pub work_item_id: Option<Option<WorkItemId>>,
}

/// 发布投递实体（数据模型 §6.15）。
///
/// 按字典精确建模，只用 `BaseModel` 承载持久化元数据；`publication_revision_id`
/// 与 `target_mall_id` 是稳定键。对外幂等键 `(sku_id, revision_no, target_mall_id)`
/// 由发布修订与目标商城组合的唯一索引保证（§6.15，P3）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct ProductPublicationDelivery {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 发布版本。
    pub publication_revision_id: ProductPublicationRevisionId,
    /// 目标商城（来源系统，类型 MALL）。
    pub target_mall_id: SourceSystemId,
    /// 跨全部尝试保持不变的外部消息身份。
    pub message_key: String,
    /// 投递状态。
    pub delivery_status: PublicationDeliveryStatus,
    /// 发送次数。
    pub attempt_count: u32,
    /// 最近发送时间。
    pub last_attempt_at: Option<Instant>,
    /// 下次受控处理时间。
    pub next_attempt_at: Option<Instant>,
    /// 商城确认时间。
    pub mall_ack_at: Option<Instant>,
    /// 商城确认版本。
    pub mall_version: Option<String>,
    /// 最近失败或结果未知的错误分类。
    pub error_class: Option<ErrorClass>,
    /// 错误码。
    pub error_code: Option<String>,
    /// 错误摘要。
    pub error_summary: Option<String>,
    /// 承接稳定消息身份的信封。
    pub inbox_message_id: Option<InboxMessageId>,
    /// 升级后的 W29 错误对象。
    pub error_task_id: Option<IntegrationErrorTaskId>,
    /// 升级后的正式待办。
    pub work_item_id: Option<WorkItemId>,
}

impl ProductPublicationDelivery {
    /// 创建发布投递。
    ///
    /// 完成版本引用/错误信息的校验与规范化，并强制四条记录不变式：
    /// 发送次数与最近发送时间成对、商城确认时间与版本成对、错误码与摘要成对、
    /// 状态与结果字段一致（已确认必须有商城确认，失败必须有错误码；确认信息只
    /// 属于已确认，错误信息只属于失败或转人工）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::ProductPublicationDeliveryId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的投递实体。
    ///
    /// # 错误
    /// 当文本超长或上述记录不变式被违反时返回错误。
    pub fn new(id: ProductPublicationDeliveryId, data: ProductPublicationDeliveryData) -> Result<Self> {
        let message_key = normalize_required_text(
            format!(
                "publication_delivery:{}:{}",
                data.publication_revision_id, data.target_mall_id
            ),
            "投递消息键不能为空",
            MESSAGE_KEY_MAX_LEN,
            "投递消息键过长",
        )?;
        let mall_version = normalize_optional_text(data.mall_version, "商城版本", MALL_VERSION_MAX_LEN)?;
        let error_code = normalize_optional_text(data.error_code, "错误码", ERROR_CODE_MAX_LEN)?;
        let error_summary = normalize_optional_text(data.error_summary, "错误摘要", ERROR_SUMMARY_MAX_LEN)?;
        validate_send_state(&PublicationDeliveryState {
            status: data.delivery_status,
            attempt_count: data.attempt_count,
            last_attempt_at: data.last_attempt_at,
            next_attempt_at: data.next_attempt_at,
            mall_ack_at: data.mall_ack_at,
            mall_version: mall_version.as_deref(),
            error_class: data.error_class,
            error_code: error_code.as_deref(),
            error_summary: error_summary.as_deref(),
            error_task_id: None,
            work_item_id: None,
        })?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            publication_revision_id: data.publication_revision_id,
            target_mall_id: data.target_mall_id,
            message_key,
            delivery_status: data.delivery_status,
            attempt_count: data.attempt_count,
            last_attempt_at: data.last_attempt_at,
            next_attempt_at: data.next_attempt_at,
            mall_ack_at: data.mall_ack_at,
            mall_version,
            error_class: data.error_class,
            error_code,
            error_summary,
            inbox_message_id: None,
            error_task_id: None,
            work_item_id: None,
        })
    }

    /// 更新发布投递。
    ///
    /// 复用 `new` 的校验规则；`publication_revision_id`/`target_mall_id` 是
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
    pub fn update(&mut self, update: ProductPublicationDeliveryUpdate) -> Result<()> {
        let delivery_status = update.delivery_status.unwrap_or(self.delivery_status);
        if !self.delivery_status.can_transition_to(delivery_status) {
            return Err(Error::from("非法的发布投递状态迁移"));
        }
        let attempt_count = update.attempt_count.unwrap_or(self.attempt_count);
        let last_attempt_at = update.last_attempt_at.unwrap_or(self.last_attempt_at);
        let next_attempt_at = update.next_attempt_at.unwrap_or(self.next_attempt_at);
        let mall_ack_at = update.mall_ack_at.unwrap_or(self.mall_ack_at);
        let mall_version = match update.mall_version {
            Some(value) => normalize_optional_text(value, "商城版本", MALL_VERSION_MAX_LEN)?,
            None => self.mall_version.clone(),
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
        validate_send_state(&PublicationDeliveryState {
            status: delivery_status,
            attempt_count,
            last_attempt_at,
            next_attempt_at,
            mall_ack_at,
            mall_version: mall_version.as_deref(),
            error_class,
            error_code: error_code.as_deref(),
            error_summary: error_summary.as_deref(),
            error_task_id: error_task_id.as_ref(),
            work_item_id: work_item_id.as_ref(),
        })?;

        self.delivery_status = delivery_status;
        self.attempt_count = attempt_count;
        self.last_attempt_at = last_attempt_at;
        self.next_attempt_at = next_attempt_at;
        self.mall_ack_at = mall_ack_at;
        self.mall_version = mall_version;
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
        self.delivery_status == PublicationDeliveryStatus::PendingSend
            || (self.delivery_status == PublicationDeliveryStatus::Retrying
                && self
                    .next_attempt_at
                    .is_some_and(|at| at.unix_secs() <= now.unix_secs()))
    }

    /// 判断当前事实是否允许查询原请求结果。
    pub fn can_query_result(&self) -> bool {
        matches!(
            self.delivery_status,
            PublicationDeliveryStatus::Sending
                | PublicationDeliveryStatus::ResultUnknown
                | PublicationDeliveryStatus::Failed
        ) && self.error_class != Some(ErrorClass::MappingError)
    }

    /// 判断当前事实是否允许沿原消息身份安排重试。
    pub fn can_retry(&self) -> bool {
        self.delivery_status == PublicationDeliveryStatus::Failed
            && self.error_class.is_some_and(|class| class.can_auto_retry())
    }

    /// 判断当前事实是否允许升级 W29。
    pub fn can_escalate(&self) -> bool {
        matches!(
            self.delivery_status,
            PublicationDeliveryStatus::Sending
                | PublicationDeliveryStatus::ResultUnknown
                | PublicationDeliveryStatus::Failed
        )
    }
}

/// 校验投递记录不变式（发送计数、成对字段与状态一致性）。
///
/// # 参数
/// * `status` - 投递状态
/// * `attempt_count` - 发送次数
/// * `last_attempt_at` - 最近发送时间
/// * `mall_ack_at` - 商城确认时间
/// * `mall_version` - 商城确认版本
/// * `error_code` - 错误码
/// * `error_summary` - 错误摘要
///
/// # 错误
/// 当发送计数与时间不配对、成对字段只有一边出现或状态与结果字段不一致时
/// 返回错误。
struct PublicationDeliveryState<'a> {
    status: PublicationDeliveryStatus,
    attempt_count: u32,
    last_attempt_at: Option<Instant>,
    next_attempt_at: Option<Instant>,
    mall_ack_at: Option<Instant>,
    mall_version: Option<&'a str>,
    error_class: Option<ErrorClass>,
    error_code: Option<&'a str>,
    error_summary: Option<&'a str>,
    error_task_id: Option<&'a IntegrationErrorTaskId>,
    work_item_id: Option<&'a WorkItemId>,
}

fn validate_send_state(state: &PublicationDeliveryState<'_>) -> Result<()> {
    if (state.attempt_count == 0) != state.last_attempt_at.is_none() {
        return Err(Error::from("发送次数为零时必须没有最近发送时间"));
    }
    if state.status == PublicationDeliveryStatus::PendingSend && state.attempt_count != 0 {
        return Err(Error::from("待发送记录的发送次数必须为零"));
    }
    if state.status != PublicationDeliveryStatus::PendingSend && state.attempt_count == 0 {
        return Err(Error::from("已开始处理的投递必须至少记录一次发送"));
    }
    if (state.status == PublicationDeliveryStatus::Retrying) != state.next_attempt_at.is_some() {
        return Err(Error::from("只有重试中记录必须安排下一次处理时间"));
    }
    if state.mall_ack_at.is_some() != state.mall_version.is_some() {
        return Err(Error::from("商城确认时间与商城版本必须同时提供或同时省略"));
    }
    if state.error_class.is_some() != state.error_code.is_some()
        || state.error_code.is_some() != state.error_summary.is_some()
    {
        return Err(Error::from("错误分类、错误码与错误摘要必须同时提供或同时省略"));
    }
    if state.status == PublicationDeliveryStatus::Confirmed && state.mall_ack_at.is_none() {
        return Err(Error::from("已确认投递必须记录商城确认信息"));
    }
    if matches!(
        state.status,
        PublicationDeliveryStatus::Failed
            | PublicationDeliveryStatus::ResultUnknown
            | PublicationDeliveryStatus::Retrying
            | PublicationDeliveryStatus::Manual
    ) && state.error_code.is_none()
    {
        return Err(Error::from("失败、结果未知、重试或转人工投递必须记录错误信息"));
    }
    if state.status == PublicationDeliveryStatus::ResultUnknown
        && state.error_class != Some(ErrorClass::ResultUnknown)
    {
        return Err(Error::from("结果未知投递必须使用结果未知错误分类"));
    }
    if state.mall_ack_at.is_some() && state.status != PublicationDeliveryStatus::Confirmed {
        return Err(Error::from("只有已确认投递才能记录商城确认信息"));
    }
    if state.status == PublicationDeliveryStatus::Confirmed && state.error_code.is_some() {
        return Err(Error::from("已确认投递不得保留错误信息"));
    }
    if state.error_task_id.is_some() != state.work_item_id.is_some() {
        return Err(Error::from("W29 错误对象与正式待办必须同时关联或同时为空"));
    }
    if state.status == PublicationDeliveryStatus::Manual && state.error_task_id.is_none() {
        return Err(Error::from("转人工投递必须关联 W29 错误对象与正式待办"));
    }
    if state.status != PublicationDeliveryStatus::Manual && state.error_task_id.is_some() {
        return Err(Error::from("只有转人工投递才能关联 W29 错误对象与正式待办"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        ProductPublicationDelivery, ProductPublicationDeliveryData, ProductPublicationDeliveryUpdate,
        PublicationDeliveryStatus,
    };
    use crate::common::time::Instant;
    use crate::ids::{ProductPublicationDeliveryId, ProductPublicationRevisionId, SourceSystemId};
    use crate::integration_ops::ErrorClass;

    fn delivery_data() -> ProductPublicationDeliveryData {
        ProductPublicationDeliveryData {
            publication_revision_id: ProductPublicationRevisionId::new("pub-rev-1"),
            target_mall_id: SourceSystemId::new("mall-1"),
            delivery_status: PublicationDeliveryStatus::PendingSend,
            attempt_count: 0,
            last_attempt_at: None,
            next_attempt_at: None,
            mall_ack_at: None,
            mall_version: None,
            error_class: None,
            error_code: None,
            error_summary: None,
        }
    }

    #[test]
    fn delivery_new_builds_pending_send_record() {
        let delivery =
            ProductPublicationDelivery::new(ProductPublicationDeliveryId::new("del-1"), delivery_data())
                .unwrap();

        assert_eq!(delivery.delivery_status, PublicationDeliveryStatus::PendingSend);
        assert_eq!(delivery.attempt_count, 0);
        assert_eq!(
            delivery.publication_revision_id,
            ProductPublicationRevisionId::new("pub-rev-1")
        );
        assert_eq!(delivery.target_mall_id, SourceSystemId::new("mall-1"));
    }

    #[test]
    fn delivery_new_accepts_confirmed_and_failed_complete_records() {
        let confirmed = ProductPublicationDeliveryData {
            delivery_status: PublicationDeliveryStatus::Confirmed,
            attempt_count: 2,
            last_attempt_at: Some(Instant::from_unix_secs(1_700_000_000)),
            mall_ack_at: Some(Instant::from_unix_secs(1_700_000_100)),
            mall_version: Some(" v1 ".to_string()),
            error_code: None,
            error_summary: None,
            ..delivery_data()
        };
        let delivery =
            ProductPublicationDelivery::new(ProductPublicationDeliveryId::new("del-2"), confirmed).unwrap();
        assert_eq!(delivery.mall_version.as_deref(), Some("v1"));

        let failed = ProductPublicationDeliveryData {
            delivery_status: PublicationDeliveryStatus::Failed,
            attempt_count: 3,
            last_attempt_at: Some(Instant::from_unix_secs(1_700_000_200)),
            mall_ack_at: None,
            mall_version: None,
            error_code: Some(" 500 ".to_string()),
            error_summary: Some(" 商城超时 ".to_string()),
            error_class: Some(ErrorClass::TransientFailure),
            ..delivery_data()
        };
        let delivery =
            ProductPublicationDelivery::new(ProductPublicationDeliveryId::new("del-3"), failed).unwrap();
        assert_eq!(delivery.error_code.as_deref(), Some("500"));
    }

    #[test]
    fn delivery_new_rejects_overlong_text_fields() {
        let overlong_version = ProductPublicationDeliveryData {
            mall_version: Some("v".repeat(129)),
            ..delivery_data()
        };
        assert!(ProductPublicationDelivery::new(
            ProductPublicationDeliveryId::new("del-4"),
            overlong_version
        )
        .is_err());

        let overlong_summary = ProductPublicationDeliveryData {
            error_code: Some("500".to_string()),
            error_summary: Some("s".repeat(1025)),
            error_class: Some(ErrorClass::TransientFailure),
            ..delivery_data()
        };
        assert!(ProductPublicationDelivery::new(
            ProductPublicationDeliveryId::new("del-5"),
            overlong_summary
        )
        .is_err());
    }

    #[test]
    fn delivery_new_rejects_half_pairs_and_inconsistent_status_fields() {
        let half_attempt = ProductPublicationDeliveryData {
            attempt_count: 1,
            last_attempt_at: None,
            ..delivery_data()
        };
        assert!(
            ProductPublicationDelivery::new(ProductPublicationDeliveryId::new("del-6"), half_attempt)
                .is_err()
        );

        let half_ack = ProductPublicationDeliveryData {
            delivery_status: PublicationDeliveryStatus::Confirmed,
            attempt_count: 1,
            last_attempt_at: Some(Instant::from_unix_secs(1_700_000_000)),
            mall_ack_at: Some(Instant::from_unix_secs(1_700_000_100)),
            mall_version: None,
            error_code: None,
            error_summary: None,
            ..delivery_data()
        };
        assert!(
            ProductPublicationDelivery::new(ProductPublicationDeliveryId::new("del-7"), half_ack).is_err()
        );

        let confirmed_without_ack = ProductPublicationDeliveryData {
            delivery_status: PublicationDeliveryStatus::Confirmed,
            attempt_count: 1,
            last_attempt_at: Some(Instant::from_unix_secs(1_700_000_000)),
            mall_ack_at: None,
            mall_version: None,
            error_code: None,
            error_summary: None,
            ..delivery_data()
        };
        assert!(ProductPublicationDelivery::new(
            ProductPublicationDeliveryId::new("del-8"),
            confirmed_without_ack
        )
        .is_err());

        let failed_without_error = ProductPublicationDeliveryData {
            delivery_status: PublicationDeliveryStatus::Failed,
            attempt_count: 1,
            last_attempt_at: Some(Instant::from_unix_secs(1_700_000_000)),
            mall_ack_at: None,
            mall_version: None,
            error_code: None,
            error_summary: None,
            ..delivery_data()
        };
        assert!(ProductPublicationDelivery::new(
            ProductPublicationDeliveryId::new("del-9"),
            failed_without_error
        )
        .is_err());

        let pending_with_ack = ProductPublicationDeliveryData {
            delivery_status: PublicationDeliveryStatus::PendingSend,
            attempt_count: 0,
            last_attempt_at: None,
            mall_ack_at: Some(Instant::from_unix_secs(1_700_000_100)),
            mall_version: Some("v1".to_string()),
            error_code: None,
            error_summary: None,
            ..delivery_data()
        };
        assert!(ProductPublicationDelivery::new(
            ProductPublicationDeliveryId::new("del-10"),
            pending_with_ack
        )
        .is_err());

        let retrying_with_error = ProductPublicationDeliveryData {
            delivery_status: PublicationDeliveryStatus::Retrying,
            attempt_count: 1,
            last_attempt_at: Some(Instant::from_unix_secs(1_700_000_000)),
            mall_ack_at: None,
            mall_version: None,
            error_code: Some("500".to_string()),
            error_summary: Some("超时".to_string()),
            error_class: Some(ErrorClass::TransientFailure),
            ..delivery_data()
        };
        assert!(ProductPublicationDelivery::new(
            ProductPublicationDeliveryId::new("del-11"),
            retrying_with_error
        )
        .is_err());
    }

    #[test]
    fn delivery_update_merges_state_and_validates_combined_record() {
        let mut delivery =
            ProductPublicationDelivery::new(ProductPublicationDeliveryId::new("del-1"), delivery_data())
                .unwrap();

        delivery
            .update(ProductPublicationDeliveryUpdate {
                delivery_status: Some(PublicationDeliveryStatus::Sending),
                attempt_count: Some(1),
                last_attempt_at: Some(Some(Instant::from_unix_secs(1_700_000_000))),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(delivery.delivery_status, PublicationDeliveryStatus::Sending);
        assert_eq!(delivery.attempt_count, 1);
        assert_eq!(delivery.last_attempt_at.unwrap().unix_secs(), 1_700_000_000);

        let invalid = ProductPublicationDeliveryUpdate {
            delivery_status: Some(PublicationDeliveryStatus::Confirmed),
            attempt_count: None,
            mall_ack_at: None,
            mall_version: None,
            ..Default::default()
        };
        assert!(delivery.update(invalid).is_err(), "合并后仍缺商城确认信息");
        assert_eq!(
            delivery.delivery_status,
            PublicationDeliveryStatus::Sending,
            "失败更新不改变实体"
        );
    }

    #[test]
    fn delivery_status_serializes_with_stable_codes_and_exposes_labels() {
        assert_eq!(
            serde_json::to_string(&PublicationDeliveryStatus::Manual).unwrap(),
            "\"manual\""
        );
        assert_eq!(PublicationDeliveryStatus::Confirmed.label(), "已确认");
        assert_eq!(PublicationDeliveryStatus::PendingSend.as_str(), "pending_send");
    }
}
