//! `product_publication_delivery`：发布投递记录（数据模型 §6.15，页面 W22）。
//!
//! 投递状态是普通记录字段：§7.7 规定投递的人工处理状态由
//! `integration_error_task.status` 表达，不另设消息投递状态机，因此本实体
//! 不实现 [`crate::common::state::DocumentState`]；`delivery_status` 按字典
//! 实现固定枚举，并通过成对/状态一致性校验保证记录完整。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{ProductPublicationDeliveryId, ProductPublicationRevisionId, SourceSystemId};
use crate::validation::normalize_optional_text;

/// 商城确认版本最大长度。
const MALL_VERSION_MAX_LEN: usize = 128;
/// 错误码最大长度。
const ERROR_CODE_MAX_LEN: usize = 128;
/// 错误摘要最大长度。
const ERROR_SUMMARY_MAX_LEN: usize = 1024;

/// 发布投递状态（数据模型 §6.15：待发送、重试中、已确认、失败、转人工；
/// 固定枚举，不设状态机）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicationDeliveryStatus {
    /// 待发送。
    #[default]
    PendingSend,
    /// 重试中。
    Retrying,
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
            Self::Retrying => "retrying",
            Self::Confirmed => "confirmed",
            Self::Failed => "failed",
            Self::Manual => "manual",
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
    /// 商城确认时间；与 `mall_version` 必须成对出现。
    pub mall_ack_at: Option<Instant>,
    /// 商城确认版本；与 `mall_ack_at` 必须成对出现。
    pub mall_version: Option<String>,
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
    pub last_attempt_at: Option<Instant>,
    /// 商城确认时间；`None` 表示不修改。
    pub mall_ack_at: Option<Instant>,
    /// 商城确认版本；`None` 表示不修改。
    pub mall_version: Option<String>,
    /// 错误码；`None` 表示不修改。
    pub error_code: Option<String>,
    /// 错误摘要；`None` 表示不修改。
    pub error_summary: Option<String>,
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
    /// 投递状态。
    pub delivery_status: PublicationDeliveryStatus,
    /// 发送次数。
    pub attempt_count: u32,
    /// 最近发送时间。
    pub last_attempt_at: Option<Instant>,
    /// 商城确认时间。
    pub mall_ack_at: Option<Instant>,
    /// 商城确认版本。
    pub mall_version: Option<String>,
    /// 错误码。
    pub error_code: Option<String>,
    /// 错误摘要。
    pub error_summary: Option<String>,
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
        let mall_version = normalize_optional_text(data.mall_version, "商城版本", MALL_VERSION_MAX_LEN)?;
        let error_code = normalize_optional_text(data.error_code, "错误码", ERROR_CODE_MAX_LEN)?;
        let error_summary = normalize_optional_text(data.error_summary, "错误摘要", ERROR_SUMMARY_MAX_LEN)?;
        validate_send_state(
            data.delivery_status,
            data.attempt_count,
            data.last_attempt_at,
            data.mall_ack_at,
            mall_version.as_deref(),
            error_code.as_deref(),
            error_summary.as_deref(),
        )?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            publication_revision_id: data.publication_revision_id,
            target_mall_id: data.target_mall_id,
            delivery_status: data.delivery_status,
            attempt_count: data.attempt_count,
            last_attempt_at: data.last_attempt_at,
            mall_ack_at: data.mall_ack_at,
            mall_version,
            error_code,
            error_summary,
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
        let attempt_count = update.attempt_count.unwrap_or(self.attempt_count);
        let last_attempt_at = update.last_attempt_at.or(self.last_attempt_at);
        let mall_ack_at = update.mall_ack_at.or(self.mall_ack_at);
        let mall_version = normalize_optional_text(update.mall_version, "商城版本", MALL_VERSION_MAX_LEN)?
            .or_else(|| self.mall_version.clone());
        let error_code = normalize_optional_text(update.error_code, "错误码", ERROR_CODE_MAX_LEN)?
            .or_else(|| self.error_code.clone());
        let error_summary = normalize_optional_text(update.error_summary, "错误摘要", ERROR_SUMMARY_MAX_LEN)?
            .or_else(|| self.error_summary.clone());
        validate_send_state(
            delivery_status,
            attempt_count,
            last_attempt_at,
            mall_ack_at,
            mall_version.as_deref(),
            error_code.as_deref(),
            error_summary.as_deref(),
        )?;

        self.delivery_status = delivery_status;
        self.attempt_count = attempt_count;
        self.last_attempt_at = last_attempt_at;
        self.mall_ack_at = mall_ack_at;
        self.mall_version = mall_version;
        self.error_code = error_code;
        self.error_summary = error_summary;
        Ok(())
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
fn validate_send_state(
    status: PublicationDeliveryStatus,
    attempt_count: u32,
    last_attempt_at: Option<Instant>,
    mall_ack_at: Option<Instant>,
    mall_version: Option<&str>,
    error_code: Option<&str>,
    error_summary: Option<&str>,
) -> Result<()> {
    if (attempt_count == 0) != last_attempt_at.is_none() {
        return Err(Error::from("发送次数为零时必须没有最近发送时间"));
    }
    if mall_ack_at.is_some() != mall_version.is_some() {
        return Err(Error::from("商城确认时间与商城版本必须同时提供或同时省略"));
    }
    if error_code.is_some() != error_summary.is_some() {
        return Err(Error::from("错误码与错误摘要必须同时提供或同时省略"));
    }
    if status == PublicationDeliveryStatus::Confirmed && mall_ack_at.is_none() {
        return Err(Error::from("已确认投递必须记录商城确认信息"));
    }
    if status == PublicationDeliveryStatus::Failed && error_code.is_none() {
        return Err(Error::from("失败投递必须记录错误信息"));
    }
    if mall_ack_at.is_some() && status != PublicationDeliveryStatus::Confirmed {
        return Err(Error::from("只有已确认投递才能记录商城确认信息"));
    }
    if error_code.is_some()
        && !matches!(
            status,
            PublicationDeliveryStatus::Failed | PublicationDeliveryStatus::Manual
        )
    {
        return Err(Error::from("只有失败或转人工投递才能记录错误信息"));
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

    fn delivery_data() -> ProductPublicationDeliveryData {
        ProductPublicationDeliveryData {
            publication_revision_id: ProductPublicationRevisionId::new("pub-rev-1"),
            target_mall_id: SourceSystemId::new("mall-1"),
            delivery_status: PublicationDeliveryStatus::PendingSend,
            attempt_count: 0,
            last_attempt_at: None,
            mall_ack_at: None,
            mall_version: None,
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
                delivery_status: Some(PublicationDeliveryStatus::Retrying),
                attempt_count: Some(1),
                last_attempt_at: Some(Instant::from_unix_secs(1_700_000_000)),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(delivery.delivery_status, PublicationDeliveryStatus::Retrying);
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
            PublicationDeliveryStatus::Retrying,
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
