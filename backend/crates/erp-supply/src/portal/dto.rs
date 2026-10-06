//! 门户仅接收供应商可维护字段；供应商归属由服务端注入。
use erp_core::common::time::Instant;
use serde::{Deserialize, Serialize};

use super::QuoteTargetVersion;
use crate::dto::supplier_offering::{SupplierOfferingTermsWrite, UpdateSupplierOfferingAvailabilityResult};
use crate::entity::supplier_offering::{AvailabilityStatus, SupplierOfferingAvailability};

/// 门户可直接报告的两类可供事实。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PortalAvailabilityStatus {
    Available,
    OutOfStock,
}
impl From<PortalAvailabilityStatus> for AvailabilityStatus {
    fn from(value: PortalAvailabilityStatus) -> Self {
        match value {
            PortalAvailabilityStatus::Available => Self::Available,
            PortalAvailabilityStatus::OutOfStock => Self::Unavailable,
        }
    }
}
/// 门户可供更新；版本必填，不能夹带商务或关系字段。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortalAvailabilityInput {
    pub expected_version: u64,
    pub availability_status: PortalAvailabilityStatus,
    pub available_quantity: Option<String>,
    pub reason: String,
    pub idempotency_key: String,
}
/// 用于命令回执和审计的可供事实允许列表。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PortalAvailabilityFact {
    pub availability_status: AvailabilityStatus,
    pub available_quantity: Option<String>,
    pub version: u64,
    pub source_updated_at: i64,
}
impl PortalAvailabilityFact {
    /// 从正式投影生成审计快照。
    /// # 参数
    /// 当前已授权可供投影。
    /// # 返回
    /// 无内部组织和其他商务字段的事实。
    /// # 错误
    /// 无。
    pub fn from_availability(value: &SupplierOfferingAvailability) -> Self {
        Self {
            availability_status: value.availability_status,
            available_quantity: value.available_quantity.map(|q| q.to_string()),
            version: value.base.version,
            source_updated_at: value.source_updated_at.unix_secs(),
        }
    }
}
/// 带变更前后事实的原成功结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortalAvailabilityUpdateResult {
    #[serde(flatten)]
    pub current: UpdateSupplierOfferingAvailabilityResult,
    pub before: PortalAvailabilityFact,
    pub after: PortalAvailabilityFact,
}
/// 商业申请类型。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApplicationKind {
    ExistingQuote,
    TermsChange,
    StopSupply,
}
/// 已有商品报价或供给变更快照；不允许客户端选择供应商。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum OfferingApplicationSnapshot {
    ExistingQuote {
        sku_id: String,
        target_version: Box<QuoteTargetVersion>,
        supplier_sku_code: String,
        supplier_product_code: Option<String>,
        terms: SupplierOfferingTermsWrite,
        availability_status: PortalAvailabilityStatus,
        available_quantity: Option<String>,
        availability_reported_at: Instant,
    },
    TermsChange {
        offering_id: String,
        expected_offering_version: u64,
        expected_revision_no: u32,
        terms: SupplierOfferingTermsWrite,
    },
    StopSupply {
        offering_id: String,
        expected_offering_version: u64,
        expected_revision_no: u32,
    },
}
/// 保存草稿的允许列表。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortalQuoteInput {
    pub snapshot: OfferingApplicationSnapshot,
    pub reason: String,
    pub expected_version: Option<u64>,
    pub idempotency_key: String,
}
impl OfferingApplicationSnapshot {
    /// 返回申请类型。
    /// # 参数
    /// 无。
    /// # 返回
    /// 稳定业务类型。
    /// # 错误
    /// 无。
    pub fn kind(&self) -> ApplicationKind {
        match self {
            Self::ExistingQuote { .. } => ApplicationKind::ExistingQuote,
            Self::TermsChange { .. } => ApplicationKind::TermsChange,
            Self::StopSupply { .. } => ApplicationKind::StopSupply,
        }
    }
    /// 返回已有目标；首次报价没有正式供给版本。
    /// # 参数
    /// 无。
    /// # 返回
    /// 已有供给标识及冻结版本。
    /// # 错误
    /// 无。
    pub fn target(&self) -> Option<(&str, u64, u32)> {
        match self {
            Self::ExistingQuote { .. } => None,
            Self::TermsChange { offering_id, expected_offering_version, expected_revision_no, .. }
            | Self::StopSupply { offering_id, expected_offering_version, expected_revision_no } => {
                Some((offering_id, *expected_offering_version, *expected_revision_no))
            },
        }
    }
}
