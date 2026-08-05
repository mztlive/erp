//! `sales_order_projection_delivery`：执行投影下发记录（数据模型 §6.16，页面 W23）。
//!
//! 投递状态是普通记录字段：§7.7 规定投递的人工处理状态由
//! `integration_error_task.status` 表达，不另设消息投递状态机，因此本实体
//! 不实现 [`crate::common::state::DocumentState`]；`status` 按字典实现固定枚举，
//! 并通过成对/状态一致性校验保证记录完整。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{SalesOrderProjectionDeliveryId, SalesOrderProjectionRevisionId, SourceSystemId};
use crate::validation::normalize_optional_text;

/// 商城执行基线最大长度。
const EXECUTION_BASELINE_MAX_LEN: usize = 256;
/// 错误码最大长度。
const ERROR_CODE_MAX_LEN: usize = 128;
/// 错误摘要最大长度。
const ERROR_SUMMARY_MAX_LEN: usize = 1024;

/// 投影下发状态（数据模型 §6.16：待发送、发送中、重试中、已确认、失败、转人工；
/// 固定枚举，不设状态机）。
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
            Self::Confirmed => "confirmed",
            Self::Failed => "failed",
            Self::Manual => "manual",
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
    pub next_attempt_at: Option<Instant>,
    /// 商城确认时间；`None` 表示不修改。
    pub mall_ack_at: Option<Instant>,
    /// 商城执行基线；`None` 表示不修改。
    pub mall_execution_baseline: Option<String>,
    /// 错误码；`None` 表示不修改。
    pub error_code: Option<String>,
    /// 错误摘要；`None` 表示不修改。
    pub error_summary: Option<String>,
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
    /// 下发状态。
    pub status: ProjectionDeliveryStatus,
    /// 发送次数。
    pub attempt_count: u32,
    /// 下次重试时间。
    pub next_attempt_at: Option<Instant>,
    /// 商城确认时间。
    pub mall_ack_at: Option<Instant>,
    /// 商城执行基线。
    pub mall_execution_baseline: Option<String>,
    /// 错误码。
    pub error_code: Option<String>,
    /// 错误摘要。
    pub error_summary: Option<String>,
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
        let mall_execution_baseline = normalize_optional_text(
            data.mall_execution_baseline,
            "商城执行基线",
            EXECUTION_BASELINE_MAX_LEN,
        )?;
        let error_code = normalize_optional_text(data.error_code, "错误码", ERROR_CODE_MAX_LEN)?;
        let error_summary = normalize_optional_text(data.error_summary, "错误摘要", ERROR_SUMMARY_MAX_LEN)?;
        validate_delivery_state(
            data.status,
            data.next_attempt_at,
            data.mall_ack_at,
            mall_execution_baseline.as_deref(),
            error_code.as_deref(),
            error_summary.as_deref(),
        )?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            projection_revision_id: data.projection_revision_id,
            target_mall_id: data.target_mall_id,
            status: data.status,
            attempt_count: data.attempt_count,
            next_attempt_at: data.next_attempt_at,
            mall_ack_at: data.mall_ack_at,
            mall_execution_baseline,
            error_code,
            error_summary,
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
        let attempt_count = update.attempt_count.unwrap_or(self.attempt_count);
        let next_attempt_at = update.next_attempt_at.or(self.next_attempt_at);
        let mall_ack_at = update.mall_ack_at.or(self.mall_ack_at);
        let mall_execution_baseline = normalize_optional_text(
            update.mall_execution_baseline,
            "商城执行基线",
            EXECUTION_BASELINE_MAX_LEN,
        )?
        .or_else(|| self.mall_execution_baseline.clone());
        let error_code = normalize_optional_text(update.error_code, "错误码", ERROR_CODE_MAX_LEN)?
            .or_else(|| self.error_code.clone());
        let error_summary = normalize_optional_text(update.error_summary, "错误摘要", ERROR_SUMMARY_MAX_LEN)?
            .or_else(|| self.error_summary.clone());
        validate_delivery_state(
            status,
            next_attempt_at,
            mall_ack_at,
            mall_execution_baseline.as_deref(),
            error_code.as_deref(),
            error_summary.as_deref(),
        )?;

        self.status = status;
        self.attempt_count = attempt_count;
        self.next_attempt_at = next_attempt_at;
        self.mall_ack_at = mall_ack_at;
        self.mall_execution_baseline = mall_execution_baseline;
        self.error_code = error_code;
        self.error_summary = error_summary;
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
fn validate_delivery_state(
    status: ProjectionDeliveryStatus,
    next_attempt_at: Option<Instant>,
    mall_ack_at: Option<Instant>,
    mall_execution_baseline: Option<&str>,
    error_code: Option<&str>,
    error_summary: Option<&str>,
) -> Result<()> {
    if matches!(
        status,
        ProjectionDeliveryStatus::PendingSend | ProjectionDeliveryStatus::Sending
    ) && next_attempt_at.is_some()
    {
        return Err(Error::from("待发送或发送中的下发记录不得安排重试时间"));
    }
    if mall_ack_at.is_some() != mall_execution_baseline.is_some() {
        return Err(Error::from("商城确认时间与商城执行基线必须同时提供或同时省略"));
    }
    if error_code.is_some() != error_summary.is_some() {
        return Err(Error::from("错误码与错误摘要必须同时提供或同时省略"));
    }
    if status == ProjectionDeliveryStatus::Confirmed && mall_ack_at.is_none() {
        return Err(Error::from("已确认下发记录必须记录商城确认信息"));
    }
    if status == ProjectionDeliveryStatus::Failed && error_code.is_none() {
        return Err(Error::from("失败下发记录必须记录错误信息"));
    }
    if mall_ack_at.is_some() && status != ProjectionDeliveryStatus::Confirmed {
        return Err(Error::from("只有已确认下发记录才能记录商城确认信息"));
    }
    if error_code.is_some()
        && !matches!(
            status,
            ProjectionDeliveryStatus::Failed | ProjectionDeliveryStatus::Manual
        )
    {
        return Err(Error::from("只有失败或转人工下发记录才能记录错误信息"));
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
    use crate::ids::{SalesOrderProjectionDeliveryId, SalesOrderProjectionRevisionId, SourceSystemId};

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
    fn delivery_new_accepts_complete_sending_and_retrying_records() {
        let sending = SalesOrderProjectionDeliveryData {
            status: ProjectionDeliveryStatus::Sending,
            attempt_count: 1,
            next_attempt_at: None,
            ..delivery_data()
        };
        let delivery =
            SalesOrderProjectionDelivery::new(SalesOrderProjectionDeliveryId::new("del-2"), sending).unwrap();
        assert_eq!(delivery.status, ProjectionDeliveryStatus::Sending);

        let retrying = SalesOrderProjectionDeliveryData {
            status: ProjectionDeliveryStatus::Retrying,
            attempt_count: 2,
            next_attempt_at: Some(Instant::from_unix_secs(1_700_000_300)),
            ..delivery_data()
        };
        let delivery =
            SalesOrderProjectionDelivery::new(SalesOrderProjectionDeliveryId::new("del-3"), retrying)
                .unwrap();
        assert_eq!(delivery.next_attempt_at.unwrap().unix_secs(), 1_700_000_300);
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
                status: Some(ProjectionDeliveryStatus::Failed),
                attempt_count: Some(2),
                error_code: Some(" 502 ".to_string()),
                error_summary: Some(" 商城无响应 ".to_string()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(delivery.status, ProjectionDeliveryStatus::Failed);
        assert_eq!(delivery.error_code.as_deref(), Some("502"));
        assert_eq!(delivery.attempt_count, 2);

        let invalid = SalesOrderProjectionDeliveryUpdate {
            status: Some(ProjectionDeliveryStatus::Confirmed),
            mall_ack_at: None,
            mall_execution_baseline: None,
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
    fn delivery_status_serializes_with_stable_codes_and_exposes_labels() {
        assert_eq!(
            serde_json::to_string(&ProjectionDeliveryStatus::Sending).unwrap(),
            "\"sending\""
        );
        assert_eq!(ProjectionDeliveryStatus::Manual.label(), "转人工");
        assert_eq!(ProjectionDeliveryStatus::Retrying.as_str(), "retrying");
    }
}
