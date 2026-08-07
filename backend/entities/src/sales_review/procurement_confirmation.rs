//! `procurement_confirmation` 与 `procurement_confirmation_line`（数据模型 §6.5）。
//!
//! 采购二次确认的唯一状态源是本表，不重复写成 `sales_order_review.review_stage`；
//! 处理记录不注册为正式业务单据，通用审计由 `workflow_action` 与 `work_item` 记录
//! （P3 服务职责）。同一销售提交仅一个有效确认批次；需要外采的销售明细确认数量
//! 合计必须覆盖承诺数量才可整单通过（跨行断言，P3 事务校验并每日复核）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::stable::StableBase;
use crate::common::state::{ensure_transition, DocumentState};
use crate::common::time::{BusinessDate, Instant};
use crate::errors::{Error, Result};
use crate::ids::{
    ProcurementConfirmationId, ProcurementConfirmationLineId, SalesOrderId, SalesOrderSubmissionId,
    SalesOrderSubmissionLineId, SupplierAccountId, SupplierCapabilityRevisionId, SupplierOfferingRevisionId,
};
use crate::money::{Quantity, Rate, UnitPrice};
use crate::validation::{normalize_optional_text, normalize_required_text};

use super::types::FulfillmentMode;

/// 处理人标识最大长度。
const HANDLER_MAX_LEN: usize = 128;
/// 补充说明最大长度。
const COMMENT_MAX_LEN: usize = 512;

/// 采购确认状态（数据模型 §6.5：待处理、通过、驳回）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProcurementConfirmationStatus {
    /// 待处理。
    Pending,
    /// 通过。
    Approved,
    /// 驳回。
    Rejected,
}

impl ProcurementConfirmationStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Pending => "待处理",
            Self::Approved => "通过",
            Self::Rejected => "驳回",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Approved => "APPROVED",
            Self::Rejected => "REJECTED",
        }
    }
}

impl DocumentState for ProcurementConfirmationStatus {
    /// 待处理可被通过或驳回；通过/驳回为终态（驳回只改变确认与审核状态，
    /// 销售单回到销售处理，不把「驳回」混入已生效状态，§6.5）。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Pending => &[Self::Approved, Self::Rejected],
            Self::Approved | Self::Rejected => &[],
        }
    }
}

/// 采购驳回原因代码（数据模型 §6.5：无法履约、成本上涨、交期不满足、资质失效等；
/// `COST_INCREASE` 仅按服务端规则触发，销售端只接收风险结论）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProcurementRejectReasonCode {
    /// 无法履约。
    CannotFulfill,
    /// 成本上涨。
    CostIncrease,
    /// 交期不满足。
    DeliveryNotMet,
    /// 资质失效。
    QualificationExpired,
    /// 其他。
    Other,
}

impl ProcurementRejectReasonCode {
    /// 返回原因的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::CannotFulfill => "无法履约",
            Self::CostIncrease => "成本上涨",
            Self::DeliveryNotMet => "交期不满足",
            Self::QualificationExpired => "资质失效",
            Self::Other => "其他",
        }
    }

    /// 返回原因的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::CannotFulfill => "CANNOT_FULFILL",
            Self::CostIncrease => "COST_INCREASE",
            Self::DeliveryNotMet => "DELIVERY_NOT_MET",
            Self::QualificationExpired => "QUALIFICATION_EXPIRED",
            Self::Other => "OTHER",
        }
    }
}

/// 采购确认创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProcurementConfirmationData {
    /// 被确认的销售单。
    pub sales_order_id: SalesOrderId,
    /// 被确认的销售提交。
    pub submission_id: SalesOrderSubmissionId,
    /// 驳回原因代码（驳回结论必填，P3 决策动作校验）。
    pub reject_reason_code: Option<ProcurementRejectReasonCode>,
    /// 补充说明。
    pub comment: Option<String>,
}

/// 采购确认实体（处理记录，数据模型 §6.5）。
///
/// `StableBase` 是 P0 冻结基元且未派生 `PartialEq`，因此本实体手工实现
/// `PartialEq`/`Eq`（全字段语义相等）以替代约定中的派生写法。
#[derive(Debug, Serialize, Deserialize, Clone, Entity)]
pub struct ProcurementConfirmation {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub stable: StableBase<ProcurementConfirmationStatus>,
    /// 被确认的销售单。
    pub sales_order_id: SalesOrderId,
    /// 被确认的销售提交。
    pub submission_id: SalesOrderSubmissionId,
    /// 采购处理人。
    pub handled_by: Option<String>,
    /// 采购处理时间。
    pub handled_at: Option<Instant>,
    /// 驳回原因代码。
    pub reject_reason_code: Option<ProcurementRejectReasonCode>,
    /// 补充说明。
    pub comment: Option<String>,
}

impl PartialEq for ProcurementConfirmation {
    /// 全字段语义相等。
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.stable.status == other.stable.status
            && self.stable.current_revision_id == other.stable.current_revision_id
            && self.stable.created_by == other.stable.created_by
            && self.stable.updated_by == other.stable.updated_by
            && self.sales_order_id == other.sales_order_id
            && self.submission_id == other.submission_id
            && self.handled_by == other.handled_by
            && self.handled_at == other.handled_at
            && self.reject_reason_code == other.reject_reason_code
            && self.comment == other.comment
    }
}

impl Eq for ProcurementConfirmation {}

impl ProcurementConfirmation {
    /// 创建采购确认（初始 `Pending`；处理人/时间由决策动作写入）。
    ///
    /// 完成补充说明的规范化（trim、长度上限）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::ProcurementConfirmationId`）
    /// * `data` - 创建数据
    /// * `created_by` - 创建人（通常为工作流任务创建者）
    ///
    /// # 返回
    /// 返回新建的采购确认。
    ///
    /// # 错误
    /// 补充说明超长时返回错误。
    pub fn new(
        id: ProcurementConfirmationId,
        data: ProcurementConfirmationData,
        created_by: impl Into<String>,
    ) -> Result<Self> {
        let comment = normalize_optional_text(data.comment, "补充说明", COMMENT_MAX_LEN)?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            stable: StableBase::new(ProcurementConfirmationStatus::Pending, created_by),
            sales_order_id: data.sales_order_id,
            submission_id: data.submission_id,
            handled_by: None,
            handled_at: None,
            reject_reason_code: data.reject_reason_code,
            comment,
        })
    }

    /// 更新采购确认（仅 `Pending` 状态允许补充说明与驳回原因）。
    ///
    /// # 参数
    /// * `data` - 更新数据
    /// * `updated_by` - 本次更新执行人
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 状态非 `Pending`，或补充说明超长时返回错误。
    pub fn update(&mut self, data: ProcurementConfirmationData, updated_by: impl Into<String>) -> Result<()> {
        ensure_transition(self.stable.status, ProcurementConfirmationStatus::Pending)?;
        if let Some(comment) = data.comment {
            self.comment = normalize_optional_text(Some(comment), "补充说明", COMMENT_MAX_LEN)?;
        }
        if let Some(reason_code) = data.reject_reason_code {
            self.reject_reason_code = Some(reason_code);
        }
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 通过采购确认（`Pending → Approved`）。
    ///
    /// 处理人与时间必须成对提供；「确认数量覆盖承诺数量」属于跨行断言（§6.5），
    /// 由 P3 在通过事务校验。
    ///
    /// # 参数
    /// * `handled_by` - 采购处理人
    /// * `handled_at` - 处理时间
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 非待处理状态，或处理人为空/超长时返回错误。
    pub fn approve(&mut self, handled_by: impl Into<String>, handled_at: Instant) -> Result<()> {
        self.decide(ProcurementConfirmationStatus::Approved, handled_by, handled_at)
    }

    /// 驳回采购确认（`Pending → Rejected`；驳回原因代码必填）。
    ///
    /// # 参数
    /// * `handled_by` - 采购处理人
    /// * `handled_at` - 处理时间
    /// * `reject_reason_code` - 驳回原因代码
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 非待处理状态、缺原因代码，或处理人为空/超长时返回错误。
    pub fn reject(
        &mut self,
        handled_by: impl Into<String>,
        handled_at: Instant,
        reject_reason_code: ProcurementRejectReasonCode,
    ) -> Result<()> {
        ensure_transition(self.stable.status, ProcurementConfirmationStatus::Rejected)?;
        let handled_by =
            normalize_required_text(handled_by.into(), "处理人不能为空", HANDLER_MAX_LEN, "处理人过长")?;
        self.reject_reason_code = Some(reject_reason_code);
        self.stable.status = ProcurementConfirmationStatus::Rejected;
        self.handled_by = Some(handled_by.clone());
        self.handled_at = Some(handled_at);
        self.stable.touch(handled_by);
        Ok(())
    }

    /// 执行一次通过结论（写入处理人/时间并迁移状态）。
    ///
    /// # 参数
    /// * `to` - 目标状态
    /// * `handled_by` - 处理人
    /// * `handled_at` - 处理时间
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 非待处理状态或处理人为空、超长时返回错误。
    fn decide(
        &mut self,
        to: ProcurementConfirmationStatus,
        handled_by: impl Into<String>,
        handled_at: Instant,
    ) -> Result<()> {
        ensure_transition(self.stable.status, to)?;
        let handled_by =
            normalize_required_text(handled_by.into(), "处理人不能为空", HANDLER_MAX_LEN, "处理人过长")?;
        self.stable.status = to;
        self.handled_by = Some(handled_by.clone());
        self.handled_at = Some(handled_at);
        self.stable.touch(handled_by);
        Ok(())
    }
}

/// 采购确认行创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProcurementConfirmationLineData {
    /// 所属确认批次。
    pub procurement_confirmation_id: ProcurementConfirmationId,
    /// 分行序号。
    pub line_no: u32,
    /// 被确认的提交快照明细。
    pub sales_order_submission_line_id: SalesOrderSubmissionLineId,
    /// 确认供应商。
    pub supplier_id: SupplierAccountId,
    /// 采购决策采用的不可变供给修订。
    pub supplier_offering_revision_id: SupplierOfferingRevisionId,
    /// 确认可供数量。
    pub confirmed_quantity: Quantity,
    /// 最新含税成本。
    pub latest_cost_gross: UnitPrice,
    /// 进项税率。
    pub input_tax_rate: Rate,
    /// 预计交期。
    pub expected_delivery_date: BusinessDate,
    /// 确认履约方式。
    pub fulfillment_mode: FulfillmentMode,
    /// 使用的能力版本。
    pub supplier_capability_revision_id: SupplierCapabilityRevisionId,
}

/// 采购确认行实体（数据模型 §6.5：`(procurement_confirmation_id, line_no)` 唯一；
/// 同一销售提交明细允许按多个供应商拆分确认，每条分行明确供应商和数量）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct ProcurementConfirmationLine {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属确认批次。
    pub procurement_confirmation_id: ProcurementConfirmationId,
    /// 分行序号。
    pub line_no: u32,
    /// 被确认的提交快照明细。
    pub sales_order_submission_line_id: SalesOrderSubmissionLineId,
    /// 确认供应商。
    pub supplier_id: SupplierAccountId,
    /// 采购决策采用的不可变供给修订。
    ///
    /// 旧数据可能缺失；新建确认分行强制写入，审批时失败关闭。
    #[serde(default)]
    pub supplier_offering_revision_id: Option<SupplierOfferingRevisionId>,
    /// 确认可供数量。
    pub confirmed_quantity: Quantity,
    /// 最新含税成本。
    pub latest_cost_gross: UnitPrice,
    /// 进项税率。
    pub input_tax_rate: Rate,
    /// 预计交期。
    pub expected_delivery_date: BusinessDate,
    /// 确认履约方式。
    pub fulfillment_mode: FulfillmentMode,
    /// 使用的能力版本。
    pub supplier_capability_revision_id: SupplierCapabilityRevisionId,
}

impl ProcurementConfirmationLine {
    /// 创建采购确认行。
    ///
    /// 完成必填校验：行号为正、确认可供数量为正、含税成本与进项税率非负。
    /// 「确认数量合计覆盖承诺数量才可整单通过」属于跨行断言（§6.5），P3 校验。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::ProcurementConfirmationLineId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的采购确认行。
    ///
    /// # 错误
    /// 行号为零、确认数量不为正，或成本/税率小于零时返回错误。
    pub fn new(id: ProcurementConfirmationLineId, data: ProcurementConfirmationLineData) -> Result<Self> {
        if data.line_no == 0 {
            return Err(Error::from("分行序号必须为正整数"));
        }
        if data.confirmed_quantity.to_decimal() <= rust_decimal::Decimal::ZERO {
            return Err(Error::from("确认可供数量必须为正数"));
        }
        if data.latest_cost_gross.to_decimal() < rust_decimal::Decimal::ZERO {
            return Err(Error::from("含税成本不能为负数"));
        }
        if data.input_tax_rate.to_decimal() < rust_decimal::Decimal::ZERO {
            return Err(Error::from("进项税率不能为负数"));
        }
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            procurement_confirmation_id: data.procurement_confirmation_id,
            line_no: data.line_no,
            sales_order_submission_line_id: data.sales_order_submission_line_id,
            supplier_id: data.supplier_id,
            supplier_offering_revision_id: Some(data.supplier_offering_revision_id),
            confirmed_quantity: data.confirmed_quantity,
            latest_cost_gross: data.latest_cost_gross,
            input_tax_rate: data.input_tax_rate,
            expected_delivery_date: data.expected_delivery_date,
            fulfillment_mode: data.fulfillment_mode,
            supplier_capability_revision_id: data.supplier_capability_revision_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use crate::money::Quantity;

    fn data() -> ProcurementConfirmationData {
        ProcurementConfirmationData {
            sales_order_id: SalesOrderId::new("o-1"),
            submission_id: SalesOrderSubmissionId::new("s-1"),
            reject_reason_code: None,
            comment: Some(" 按确认结果执行 ".to_string()),
        }
    }

    #[test]
    fn new_trims_comment_and_starts_pending() {
        let confirmation =
            ProcurementConfirmation::new(ProcurementConfirmationId::new("pc-1"), data(), "system-1").unwrap();

        assert_eq!(
            confirmation.stable.status(),
            ProcurementConfirmationStatus::Pending
        );
        assert_eq!(confirmation.comment.as_deref(), Some("按确认结果执行"));
        assert!(confirmation.handled_by.is_none());
        assert!(confirmation.handled_at.is_none());
    }

    #[test]
    fn new_rejects_overlong_comment() {
        let overlong = ProcurementConfirmationData {
            comment: Some("x".repeat(513)),
            ..data()
        };
        assert!(
            ProcurementConfirmation::new(ProcurementConfirmationId::new("pc-1"), overlong, "system-1")
                .is_err()
        );
    }

    #[test]
    fn approve_and_reject_decisions() {
        let mut confirmation =
            ProcurementConfirmation::new(ProcurementConfirmationId::new("pc-1"), data(), "system-1").unwrap();
        confirmation
            .approve(" procurement-1 ", Instant::from_unix_secs(1_800_000_000))
            .unwrap();
        assert_eq!(
            confirmation.stable.status(),
            ProcurementConfirmationStatus::Approved
        );
        assert_eq!(confirmation.handled_by.as_deref(), Some("procurement-1"));
        assert_eq!(confirmation.handled_at.unwrap().unix_secs(), 1_800_000_000);

        let mut rejected =
            ProcurementConfirmation::new(ProcurementConfirmationId::new("pc-2"), data(), "system-1").unwrap();
        rejected
            .reject(
                "procurement-2",
                Instant::from_unix_secs(1_800_000_100),
                ProcurementRejectReasonCode::DeliveryNotMet,
            )
            .unwrap();
        assert_eq!(rejected.stable.status(), ProcurementConfirmationStatus::Rejected);
        assert_eq!(
            rejected.reject_reason_code,
            Some(ProcurementRejectReasonCode::DeliveryNotMet)
        );

        // 终态逐边定向断言
        assert!(rejected
            .approve("procurement-2", Instant::from_unix_secs(1_800_000_200))
            .is_err());
        assert!(ProcurementConfirmationStatus::Approved.allowed_next().is_empty());
        assert!(ProcurementConfirmationStatus::Rejected.allowed_next().is_empty());
    }

    #[test]
    fn update_only_while_pending() {
        let mut confirmation =
            ProcurementConfirmation::new(ProcurementConfirmationId::new("pc-1"), data(), "system-1").unwrap();
        confirmation
            .update(
                ProcurementConfirmationData {
                    comment: Some(" 更新说明 ".to_string()),
                    ..data()
                },
                "procurement-1",
            )
            .unwrap();
        assert_eq!(confirmation.comment.as_deref(), Some("更新说明"));

        confirmation
            .approve("procurement-1", Instant::from_unix_secs(1_800_000_000))
            .unwrap();
        assert!(
            confirmation.update(data(), "procurement-1").is_err(),
            "已通过不可再更新"
        );
    }

    fn line_data() -> ProcurementConfirmationLineData {
        ProcurementConfirmationLineData {
            procurement_confirmation_id: ProcurementConfirmationId::new("pc-1"),
            line_no: 1,
            sales_order_submission_line_id: SalesOrderSubmissionLineId::new("sl-1"),
            supplier_id: SupplierAccountId::new("sup-1"),
            supplier_offering_revision_id: SupplierOfferingRevisionId::new("off-rev-1"),
            confirmed_quantity: Quantity::from_str("10.000000").unwrap(),
            latest_cost_gross: UnitPrice::from_str("8.5000").unwrap(),
            input_tax_rate: Rate::from_str("0.130000").unwrap(),
            expected_delivery_date: BusinessDate::from_ymd(2026, 9, 30).unwrap(),
            fulfillment_mode: FulfillmentMode::SupplierDirect,
            supplier_capability_revision_id: SupplierCapabilityRevisionId::new("cap-rev-1"),
        }
    }

    #[test]
    fn line_new_accepts_positive_quantity_and_cost() {
        let line = ProcurementConfirmationLine::new(ProcurementConfirmationLineId::new("pcl-1"), line_data())
            .unwrap();
        assert_eq!(line.confirmed_quantity, Quantity::from_str("10.000000").unwrap());
        assert_eq!(
            line.supplier_offering_revision_id,
            Some(SupplierOfferingRevisionId::new("off-rev-1"))
        );
        assert_eq!(line.expected_delivery_date.to_string(), "2026-09-30");
        assert_eq!(line.fulfillment_mode, FulfillmentMode::SupplierDirect);
    }

    #[test]
    fn line_new_rejects_zero_no_negative_quantity_and_negative_cost() {
        let zero_no = ProcurementConfirmationLineData {
            line_no: 0,
            ..line_data()
        };
        assert!(
            ProcurementConfirmationLine::new(ProcurementConfirmationLineId::new("pcl-1"), zero_no).is_err()
        );

        let negative_qty = ProcurementConfirmationLineData {
            confirmed_quantity: Quantity::from_str("-1.000000").unwrap(),
            ..line_data()
        };
        assert!(
            ProcurementConfirmationLine::new(ProcurementConfirmationLineId::new("pcl-1"), negative_qty)
                .is_err()
        );

        let negative_cost = ProcurementConfirmationLineData {
            latest_cost_gross: UnitPrice::from_str("-1.0000").unwrap(),
            ..line_data()
        };
        assert!(
            ProcurementConfirmationLine::new(ProcurementConfirmationLineId::new("pcl-1"), negative_cost)
                .is_err()
        );
    }
}
