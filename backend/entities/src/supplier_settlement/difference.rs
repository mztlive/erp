//! `supplier_settlement_difference`（数据模型 §6.20 供应商结算差异）。
//!
//! 对账只生成差异，不直接修正式事实（§9.4）；未解决差异不得直接修改供应商订单或原
//! 成本（§6.20，P3 校验）。差异类型与状态是固定枚举（§4.6、§13.3）；处理结果三元组
//! （`resolution`/`resolved_by`/`resolved_at`）成组出现，待处理不得填写，已补偿/已关闭
//! 必填。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{SupplierSettlementDifferenceId, SupplierSettlementItemId};
use crate::money::Amount;
use crate::validation::{normalize_optional_text, normalize_required_text};

/// 处理结果文本最大长度。
const RESOLUTION_MAX_LEN: usize = 512;
/// 处理人标识最大长度。
const ACTOR_MAX_LEN: usize = 128;

/// 结算差异类型（数据模型 §6.20：漏单、重复、金额、退款、状态）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SettlementDifferenceType {
    /// 漏单。
    Missing,
    /// 重复。
    Duplicate,
    /// 金额。
    Amount,
    /// 退款。
    Refund,
    /// 状态。
    Status,
}

impl SettlementDifferenceType {
    /// 返回差异类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Missing => "漏单",
            Self::Duplicate => "重复",
            Self::Amount => "金额",
            Self::Refund => "退款",
            Self::Status => "状态",
        }
    }

    /// 返回差异类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Missing => "MISSING",
            Self::Duplicate => "DUPLICATE",
            Self::Amount => "AMOUNT",
            Self::Refund => "REFUND",
            Self::Status => "STATUS",
        }
    }
}

/// 结算差异状态（数据模型 §6.20：待处理、供应商认可、ERP 认可、已补偿、关闭）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SettlementDifferenceStatus {
    /// 待处理。
    Pending,
    /// 供应商认可。
    SupplierAcknowledged,
    /// ERP 认可。
    ErpAcknowledged,
    /// 已补偿。
    Compensated,
    /// 关闭。
    Closed,
}

impl SettlementDifferenceStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Pending => "待处理",
            Self::SupplierAcknowledged => "供应商认可",
            Self::ErpAcknowledged => "ERP 认可",
            Self::Compensated => "已补偿",
            Self::Closed => "关闭",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::SupplierAcknowledged => "SUPPLIER_ACKNOWLEDGED",
            Self::ErpAcknowledged => "ERP_ACKNOWLEDGED",
            Self::Compensated => "COMPENSATED",
            Self::Closed => "CLOSED",
        }
    }
}

/// 结算差异创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierSettlementDifferenceData {
    /// 所属结算明细。
    pub statement_item_id: SupplierSettlementItemId,
    /// 差异类型。
    pub difference_type: SettlementDifferenceType,
    /// 差异金额（有符号，负数表示 ERP 金额大于供应商金额）。
    pub difference_amount: Amount,
    /// 差异状态。
    pub status: SettlementDifferenceStatus,
    /// 处理结果文本。
    pub resolution: Option<String>,
    /// 处理人。
    pub resolved_by: Option<String>,
    /// 处理时间。
    pub resolved_at: Option<Instant>,
}

/// 结算差异更新数据（不含系统字段与关键字段）。
///
/// 结算明细、差异类型与差异金额创建后不可修改；处理结果三元组以 `Some` 设置、
/// `None` 保持原值。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SupplierSettlementDifferenceUpdate {
    /// 差异状态；`None` 表示不修改。
    pub status: Option<SettlementDifferenceStatus>,
    /// 处理结果文本；`None` 表示不修改。
    pub resolution: Option<String>,
    /// 处理人；`None` 表示不修改。
    pub resolved_by: Option<String>,
    /// 处理时间；`None` 表示不修改。
    pub resolved_at: Option<Instant>,
}

/// 供应商结算差异实体（数据模型 §6.20）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SupplierSettlementDifference {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属结算明细。
    pub statement_item_id: SupplierSettlementItemId,
    /// 差异类型。
    pub difference_type: SettlementDifferenceType,
    /// 差异金额。
    pub difference_amount: Amount,
    /// 差异状态。
    pub status: SettlementDifferenceStatus,
    /// 处理结果文本。
    pub resolution: Option<String>,
    /// 处理人。
    pub resolved_by: Option<String>,
    /// 处理时间。
    pub resolved_at: Option<Instant>,
}

impl SupplierSettlementDifference {
    /// 创建结算差异。
    ///
    /// 完成处理结果字段的校验和规范化，并强制三元组成组约束（§6.20）：
    /// 处理结果/处理人/处理时间必须同时提供或同时省略；待处理不得填写；
    /// 已补偿或已关闭必填。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SupplierSettlementDifferenceId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的结算差异实体。
    ///
    /// # 错误
    /// 文本超长或处理结果三元组与状态不一致时返回错误。
    pub fn new(id: SupplierSettlementDifferenceId, data: SupplierSettlementDifferenceData) -> Result<Self> {
        let resolution = normalize_optional_text(data.resolution, "处理结果", RESOLUTION_MAX_LEN)?;
        let resolved_by = normalize_optional_text(data.resolved_by, "处理人", ACTOR_MAX_LEN)?;
        validate_resolution_state(data.status, &resolution, &resolved_by, data.resolved_at)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            statement_item_id: data.statement_item_id,
            difference_type: data.difference_type,
            difference_amount: data.difference_amount,
            status: data.status,
            resolution,
            resolved_by,
            resolved_at: data.resolved_at,
        })
    }

    /// 更新结算差异。
    ///
    /// 复用 `new` 的校验规则；结算明细、差异类型与差异金额不可修改。
    ///
    /// # 参数
    /// * `update` - 更新数据
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 文本为空/超长或处理结果三元组与目标状态不一致时返回错误。
    pub fn update(&mut self, update: SupplierSettlementDifferenceUpdate) -> Result<()> {
        let status = update.status.unwrap_or(self.status);
        let resolution = if let Some(resolution) = update.resolution {
            Some(normalize_required_text(
                resolution,
                "处理结果不能为空",
                RESOLUTION_MAX_LEN,
                "处理结果过长",
            )?)
        } else {
            self.resolution.clone()
        };
        let resolved_by = if let Some(resolved_by) = update.resolved_by {
            Some(normalize_required_text(
                resolved_by,
                "处理人不能为空",
                ACTOR_MAX_LEN,
                "处理人过长",
            )?)
        } else {
            self.resolved_by.clone()
        };
        let resolved_at = update.resolved_at.or(self.resolved_at);
        validate_resolution_state(status, &resolution, &resolved_by, resolved_at)?;

        self.status = status;
        self.resolution = resolution;
        self.resolved_by = resolved_by;
        self.resolved_at = resolved_at;
        Ok(())
    }
}

/// 校验处理结果三元组与状态的成组约束。
///
/// # 参数
/// * `status` - 差异状态
/// * `resolution` - 处理结果文本
/// * `resolved_by` - 处理人
/// * `resolved_at` - 处理时间
///
/// # 错误
/// 三元组不完整、待处理填写了处理结果或已补偿/已关闭缺少处理结果时返回错误。
fn validate_resolution_state(
    status: SettlementDifferenceStatus,
    resolution: &Option<String>,
    resolved_by: &Option<String>,
    resolved_at: Option<Instant>,
) -> Result<()> {
    let trio_present = resolution.is_some() || resolved_by.is_some() || resolved_at.is_some();
    let trio_complete = resolution.is_some() && resolved_by.is_some() && resolved_at.is_some();
    if trio_present && !trio_complete {
        return Err(Error::from("处理结果、处理人与处理时间必须同时提供或同时省略"));
    }
    match status {
        SettlementDifferenceStatus::Pending if trio_present => Err(Error::from("待处理差异不得填写处理结果")),
        SettlementDifferenceStatus::Compensated | SettlementDifferenceStatus::Closed if !trio_complete => {
            Err(Error::from("已补偿或已关闭差异必须填写处理结果"))
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::SupplierSettlementDifferenceId;
    use std::str::FromStr;

    fn sample_data() -> SupplierSettlementDifferenceData {
        SupplierSettlementDifferenceData {
            statement_item_id: SupplierSettlementItemId::new("statement-item-1"),
            difference_type: SettlementDifferenceType::Amount,
            difference_amount: Amount::from_str("12.00").unwrap(),
            status: SettlementDifferenceStatus::Pending,
            resolution: None,
            resolved_by: None,
            resolved_at: None,
        }
    }

    fn compensated_data() -> SupplierSettlementDifferenceData {
        SupplierSettlementDifferenceData {
            status: SettlementDifferenceStatus::Compensated,
            resolution: Some(" 已追加成本差额 ".to_string()),
            resolved_by: Some(" 财务-1 ".to_string()),
            resolved_at: Some(Instant::from_unix_secs(1_700_000_000)),
            ..sample_data()
        }
    }

    #[test]
    fn new_accepts_pending_without_resolution() {
        let difference = SupplierSettlementDifference::new(
            SupplierSettlementDifferenceId::new("difference-1"),
            sample_data(),
        )
        .unwrap();

        assert_eq!(difference.difference_type, SettlementDifferenceType::Amount);
        assert_eq!(difference.status, SettlementDifferenceStatus::Pending);
        assert!(difference.resolution.is_none());
    }

    #[test]
    fn new_accepts_compensated_with_resolution() {
        let difference = SupplierSettlementDifference::new(
            SupplierSettlementDifferenceId::new("difference-2"),
            compensated_data(),
        )
        .unwrap();

        assert_eq!(difference.status, SettlementDifferenceStatus::Compensated);
        assert_eq!(difference.resolution.as_deref(), Some("已追加成本差额"));
        assert_eq!(difference.resolved_by.as_deref(), Some("财务-1"));
        assert!(difference.resolved_at.is_some());
    }

    #[test]
    fn new_rejects_inconsistent_resolution_trio() {
        let compensated_without_resolution = SupplierSettlementDifferenceData {
            status: SettlementDifferenceStatus::Compensated,
            ..sample_data()
        };
        assert!(SupplierSettlementDifference::new(
            SupplierSettlementDifferenceId::new("difference-3"),
            compensated_without_resolution
        )
        .is_err());

        let pending_with_resolution = SupplierSettlementDifferenceData {
            resolution: Some("补偿完成".to_string()),
            resolved_by: Some("财务-1".to_string()),
            resolved_at: Some(Instant::from_unix_secs(1_700_000_000)),
            ..sample_data()
        };
        assert!(SupplierSettlementDifference::new(
            SupplierSettlementDifferenceId::new("difference-4"),
            pending_with_resolution
        )
        .is_err());

        let partial_trio = SupplierSettlementDifferenceData {
            status: SettlementDifferenceStatus::Closed,
            resolution: Some("关闭".to_string()),
            ..sample_data()
        };
        assert!(SupplierSettlementDifference::new(
            SupplierSettlementDifferenceId::new("difference-5"),
            partial_trio
        )
        .is_err());
    }

    #[test]
    fn new_rejects_overlong_resolution() {
        let data = SupplierSettlementDifferenceData {
            status: SettlementDifferenceStatus::Closed,
            resolution: Some("r".repeat(513)),
            resolved_by: Some("财务-1".to_string()),
            resolved_at: Some(Instant::from_unix_secs(1_700_000_000)),
            ..sample_data()
        };
        assert!(
            SupplierSettlementDifference::new(SupplierSettlementDifferenceId::new("difference-6"), data)
                .is_err()
        );
    }

    #[test]
    fn update_applies_status_and_resolution() {
        let mut difference = SupplierSettlementDifference::new(
            SupplierSettlementDifferenceId::new("difference-1"),
            sample_data(),
        )
        .unwrap();
        difference
            .update(SupplierSettlementDifferenceUpdate {
                status: Some(SettlementDifferenceStatus::ErpAcknowledged),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(difference.status, SettlementDifferenceStatus::ErpAcknowledged);

        difference
            .update(SupplierSettlementDifferenceUpdate {
                status: Some(SettlementDifferenceStatus::Compensated),
                resolution: Some("补偿".to_string()),
                resolved_by: Some("财务-2".to_string()),
                resolved_at: Some(Instant::from_unix_secs(1_700_000_100)),
            })
            .unwrap();
        assert_eq!(difference.status, SettlementDifferenceStatus::Compensated);
        assert_eq!(difference.resolved_by.as_deref(), Some("财务-2"));
        assert_eq!(
            difference.difference_type,
            SettlementDifferenceType::Amount,
            "关键字段不可修改"
        );
    }

    #[test]
    fn update_rejects_missing_resolution_for_closed() {
        let mut difference = SupplierSettlementDifference::new(
            SupplierSettlementDifferenceId::new("difference-1"),
            sample_data(),
        )
        .unwrap();
        assert!(difference
            .update(SupplierSettlementDifferenceUpdate {
                status: Some(SettlementDifferenceStatus::Closed),
                ..Default::default()
            })
            .is_err());
    }
}
