//! 采购依据更新记录（第一期 §6.3“销售变更对采购的影响”第 4、5、11 条）。
//!
//! 记录一张采购因销售变更受影响的原因、采购负责人的待办以及最终处理方式；同一
//! 采购同时只有一条开放记录。受影响状态按事实重算，记录只承载处理生命周期与
//! 维持登记形成的判定基准，不改写采购、应付或付款申请的冻结来源。

use std::collections::HashMap;

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
use erp_core::money::Quantity;
use erp_core::validation::normalize_required_text_ref;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::ReturnSettlementDecision;
use super::order::PurchaseOrder;
use super::source_impact::{PurchaseSourceImpact, SourceImpactKind, SourceImpactReason};

/// 维持原采购说明的最大长度。
const MAINTAIN_NOTE_MAX_LEN: usize = 500;

/// 采购依据更新记录状态。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PurchaseSourceUpdateStatus {
    /// 采购仍受影响，等待采购负责人处理。
    Open,
    /// 已处理完成。
    Resolved,
}

impl PurchaseSourceUpdateStatus {
    /// 返回持久化代码。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回与序列化一致的 `SCREAMING_SNAKE_CASE` 代码。
    ///
    /// # 错误
    /// 无。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "OPEN",
            Self::Resolved => "RESOLVED",
        }
    }
}

/// 采购依据更新的处理方式。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PurchaseSourceResolution {
    /// 未生效采购按新销售版本修改原单。
    Resubmitted,
    /// 已生效采购完成采购变更。
    Changed,
    /// 采购已作废。
    Voided,
    /// 采购核实无法调整，登记维持原采购。
    Maintained,
    /// 因其他采购调减或销售再次变更，不再受影响。
    NoLongerAffected,
}

impl PurchaseSourceResolution {
    /// 返回处理方式中文名。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回稳定中文展示名。
    ///
    /// # 错误
    /// 无。
    pub fn label(self) -> &'static str {
        match self {
            Self::Resubmitted => "已按新销售版本修改原单",
            Self::Changed => "已办理采购变更",
            Self::Voided => "已作废",
            Self::Maintained => "已登记维持原采购",
            Self::NoLongerAffected => "已不受影响",
        }
    }
}

/// 一张采购的依据更新记录。
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct PurchaseSourceUpdate {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 受影响采购单。
    pub purchase_order_id: String,
    /// 来源销售单。
    pub sales_order_id: String,
    /// 最近一次标记或刷新时的销售当前版本。
    pub sales_order_revision_id: String,
    /// 标记时采购引用的销售版本；采购此后换到新版本时，维持基准失效。
    pub purchase_sales_revision_id: String,
    /// 触发标记的销售变更单；非销售变更路径标记时为空。
    pub trigger_sales_change_order_id: Option<String>,
    /// 最近一次判定的影响原因。
    pub reasons: Vec<SourceImpactReason>,
    /// 最近一次判定是否暂停付款。
    pub blocks_payment: bool,
    /// 记录状态。
    pub status: PurchaseSourceUpdateStatus,
    /// 草稿已按销售当前事实修改、不再受影响，等待重新提交审批。
    #[serde(default)]
    pub awaiting_submission: bool,
    /// 采购确认的款项处置方案；历史记录为空。
    #[serde(default)]
    pub return_settlement: Option<ReturnSettlementDecision>,
    #[serde(default)]
    pub return_settlement_history: Vec<ReturnSettlementDecision>,
    /// 处理方式；开放时为空。
    pub resolution: Option<PurchaseSourceResolution>,
    /// 处理所形成的单据，如采购提交或采购变更单。
    pub resolution_document_id: Option<String>,
    /// 维持原采购的说明。
    pub maintain_note: Option<String>,
    /// 采购负责人的待办；负责人缺失时为空。
    pub work_item_id: Option<String>,
    /// 处理完成时间。
    pub resolved_at: Option<Instant>,
    /// 处理人；系统按事实判定时为 `__system__`。
    pub resolved_by: Option<String>,
}

/// 新建依据更新记录所需事实。
pub struct SourceUpdateOpening<'a> {
    /// 受影响采购。
    pub order: &'a PurchaseOrder,
    /// 本次判定。
    pub impact: &'a PurchaseSourceImpact,
    /// 销售当前版本。
    pub sales_order_revision_id: &'a str,
    /// 触发的销售变更单。
    pub trigger_sales_change_order_id: Option<&'a str>,
}

impl PurchaseSourceUpdate {
    /// 为受影响采购开立依据更新记录。
    ///
    /// # 参数
    /// * `id` - 记录主键
    /// * `opening` - 受影响采购、判定与触发来源
    ///
    /// # 返回
    /// 返回开放状态的记录。
    ///
    /// # 错误
    /// 无。
    pub fn open(id: String, opening: SourceUpdateOpening<'_>) -> Self {
        Self {
            base: BaseModel::new(id),
            purchase_order_id: opening.order.base.id.clone(),
            sales_order_id: opening.order.sales_order_id.to_string(),
            sales_order_revision_id: opening.sales_order_revision_id.to_string(),
            purchase_sales_revision_id: opening.order.sales_order_revision_id.to_string(),
            trigger_sales_change_order_id: opening.trigger_sales_change_order_id.map(str::to_string),
            reasons: opening.impact.reasons.clone(),
            blocks_payment: opening.impact.blocks_payment(),
            status: PurchaseSourceUpdateStatus::Open,
            awaiting_submission: false,
            return_settlement: None,
            return_settlement_history: Vec::new(),
            resolution: None,
            resolution_document_id: None,
            maintain_note: None,
            work_item_id: None,
            resolved_at: None,
            resolved_by: None,
        }
    }

    /// 用最新判定刷新开放记录。
    ///
    /// # 参数
    /// * `impact` - 最新判定
    /// * `sales_order_revision_id` - 销售当前版本
    /// * `trigger` - 新的触发销售变更单；为空时保留原值
    ///
    /// # 返回
    /// 原因、付款暂停、版本或触发来源有变化时返回 `true`。
    ///
    /// # 错误
    /// 记录已处理完成时返回错误。
    pub fn refresh(
        &mut self,
        impact: &PurchaseSourceImpact,
        sales_order_revision_id: &str,
        trigger: Option<&str>,
    ) -> Result<bool> {
        self.ensure_open()?;
        let trigger = trigger.map(str::to_string).or_else(|| self.trigger_sales_change_order_id.clone());
        let changed = self.reasons != impact.reasons
            || self.blocks_payment != impact.blocks_payment()
            || self.awaiting_submission
            || self.sales_order_revision_id != sales_order_revision_id
            || self.trigger_sales_change_order_id != trigger;
        self.reasons = impact.reasons.clone();
        self.blocks_payment = impact.blocks_payment();
        self.awaiting_submission = false;
        self.sales_order_revision_id = sales_order_revision_id.to_string();
        self.trigger_sales_change_order_id = trigger;
        Ok(changed)
    }

    /// 草稿已不再受影响时转为等待重新提交，保留最近一次影响原因。
    ///
    /// # 参数
    /// * `sales_order_revision_id` - 销售当前版本
    ///
    /// # 返回
    /// 首次转入等待提交或销售版本变化时返回 `true`。
    ///
    /// # 错误
    /// 记录已处理完成时返回错误。
    ///
    /// # 关键业务约束
    /// 第一期 §6.3“销售变更对采购的影响”第 5、11 条：草稿及被撤回的采购须重新提交
    /// 才算处理完成，期间待办保持开放。
    pub fn await_submission(&mut self, sales_order_revision_id: &str) -> Result<bool> {
        self.ensure_open()?;
        let changed = !self.awaiting_submission || self.sales_order_revision_id != sales_order_revision_id;
        self.awaiting_submission = true;
        self.blocks_payment = false;
        self.sales_order_revision_id = sales_order_revision_id.to_string();
        Ok(changed)
    }

    /// 关联采购负责人的待办。
    ///
    /// # 参数
    /// * `work_item_id` - 待办主键
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 无。
    pub fn attach_work_item(&mut self, work_item_id: String) {
        self.work_item_id = Some(work_item_id);
    }

    /// 按事实判定处理完成。
    ///
    /// # 参数
    /// * `resolution` - 处理方式，不得为维持原采购
    /// * `document_id` - 处理所形成的单据
    /// * `at` - 完成时间
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 记录已处理完成，或以本方法登记维持原采购时返回错误。
    pub fn resolve(
        &mut self,
        resolution: PurchaseSourceResolution,
        document_id: Option<String>,
        at: Instant,
    ) -> Result<()> {
        self.ensure_open()?;
        if resolution == PurchaseSourceResolution::Maintained {
            return Err(Error::from("维持原采购须由采购负责人登记原因"));
        }
        self.finish(resolution, "__system__", at);
        self.resolution_document_id = document_id;
        Ok(())
    }

    /// 登记维持原采购，以本记录的销售版本作为后续判定基准。
    ///
    /// # 参数
    /// * `note` - 无法调整的原因
    /// * `actor_id` - 登记人
    /// * `at` - 登记时间
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 记录已处理完成、草稿已修改待提交、原因为空或超过 500 字时返回错误。
    pub fn maintain(&mut self, note: &str, actor_id: &str, at: Instant) -> Result<()> {
        self.ensure_open()?;
        if self.awaiting_submission {
            return Err(Error::from("采购已按新销售版本修改，请提交审批"));
        }
        let note = normalize_required_text_ref(
            note,
            "请填写维持原采购的原因",
            MAINTAIN_NOTE_MAX_LEN,
            "维持原采购的原因不能超过 500 字",
        )?;
        self.finish(PurchaseSourceResolution::Maintained, actor_id, at);
        self.maintain_note = Some(note);
        Ok(())
    }

    /// 返回仍然有效的维持基准。
    ///
    /// # 参数
    /// * `purchase_sales_revision_id` - 采购当前引用的销售版本
    ///
    /// # 返回
    /// 已登记维持且采购此后未换到新销售版本时，返回维持时的销售版本。
    ///
    /// # 错误
    /// 无。
    pub fn maintained_baseline(&self, purchase_sales_revision_id: &str) -> Option<&str> {
        (self.resolution == Some(PurchaseSourceResolution::Maintained)
            && self.purchase_sales_revision_id == purchase_sales_revision_id)
            .then_some(self.sales_order_revision_id.as_str())
    }

    /// 生成待办影响摘要。
    ///
    /// # 参数
    /// * `sales_order_no` - 来源销售单号
    ///
    /// # 返回
    /// 返回“销售单号 + 逐行原因”的中文摘要；等待重新提交时提示提交审批。
    ///
    /// # 错误
    /// 无。
    pub fn impact_summary(&self, sales_order_no: &str) -> String {
        if self.awaiting_submission {
            return format!("销售单 {sales_order_no} 已变更，采购已按新销售版本修改，请提交审批");
        }
        let reasons = self.reasons.iter().map(describe_reason).collect::<Vec<_>>().join("；");
        format!("销售单 {sales_order_no} 已变更：{reasons}")
    }

    /// 确认记录仍为开放状态。
    fn ensure_open(&self) -> Result<()> {
        if self.status != PurchaseSourceUpdateStatus::Open {
            return Err(Error::from("采购依据已处理，请刷新后查看"));
        }
        Ok(())
    }

    /// 写入完成状态。
    fn finish(&mut self, resolution: PurchaseSourceResolution, actor_id: &str, at: Instant) {
        self.status = PurchaseSourceUpdateStatus::Resolved;
        self.resolution = Some(resolution);
        self.resolved_at = Some(at);
        self.resolved_by = Some(actor_id.to_string());
    }
}

/// 汇总仍然有效的维持基准。
///
/// # 参数
/// * `orders` - 参与覆盖的采购单
/// * `updates` - 同一销售单的依据更新记录，按创建顺序
///
/// # 返回
/// 返回采购单 ID 到维持时销售版本的映射；同一采购取最后一次维持登记。
///
/// # 错误
/// 无。
pub fn maintained_baselines(
    orders: &[PurchaseOrder],
    updates: &[PurchaseSourceUpdate],
) -> HashMap<String, String> {
    orders
        .iter()
        .filter_map(|order| {
            updates
                .iter()
                .rev()
                .filter(|update| update.purchase_order_id == order.base.id)
                .find(|update| update.resolution == Some(PurchaseSourceResolution::Maintained))
                .and_then(|update| update.maintained_baseline(order.sales_order_revision_id.as_ref()))
                .map(|revision| (order.base.id.clone(), revision.to_string()))
        })
        .collect()
}

/// 描述一行影响原因。
///
/// # 参数
/// * `reason` - 影响原因
///
/// # 返回
/// 返回用户可读的中文描述，不含内部代码。
///
/// # 错误
/// 无。
pub fn describe_reason(reason: &SourceImpactReason) -> String {
    let line = format!("第 {} 行 {}", reason.line_no, reason.item_name);
    match &reason.kind {
        SourceImpactKind::Zeroed { .. } => format!("{line} 数量已归零"),
        SourceImpactKind::QuantityReduced { baseline_quantity, current_quantity, excess_quantity } => {
            format!(
                "{line} 数量 {} → {}，超出 {}",
                plain(*baseline_quantity),
                plain(*current_quantity),
                plain(*excess_quantity)
            )
        },
        SourceImpactKind::DueEarlier { current_due, expected_delivery_date } => {
            format!("{line} 履约期限提前到 {current_due}，早于预计交付 {expected_delivery_date}")
        },
        SourceImpactKind::DeliveryChanged => format!("{line} 收件地址或数量已变化"),
    }
}

/// 去掉数量尾随零。
fn plain(quantity: Quantity) -> String {
    quantity.to_decimal().normalize().to_string()
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::common::time::{BusinessDate, Instant};
    use erp_core::ids::{
        PurchaseOrderId, SalesOrderId, SalesOrderRevisionId, SupplierAccountId, WarehouseId,
    };
    use erp_core::money::Quantity;

    use super::{
        PurchaseSourceResolution, PurchaseSourceUpdate, PurchaseSourceUpdateStatus, SourceUpdateOpening,
    };
    use crate::entity::purchase_order::{
        FulfillmentResponsibility, PurchaseOrder, PurchaseOrderData, PurchaseSourceImpact, PurchaseType,
        SourceImpactKind, SourceImpactReason,
    };

    fn qty(value: &str) -> Quantity {
        Quantity::from_str(value).unwrap()
    }

    fn order() -> PurchaseOrder {
        PurchaseOrder::new(
            PurchaseOrderId::new("po-1"),
            PurchaseOrderData {
                business_org_unit_id: "org".into(),
                purchase_no: "PO-1".into(),
                sales_order_id: SalesOrderId::new("so-1"),
                sales_order_revision_id: SalesOrderRevisionId::new("rev-1"),
                creation_basis_id: "basis".into(),
                supplier_id: SupplierAccountId::new("sup-1"),
                purchase_type: PurchaseType::Physical,
                payment_term_code: "NET-30".into(),
                fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
                owner_user_id: "buyer".into(),
                target_warehouse_id: Some(WarehouseId::new("wh-1")),
            },
            "buyer",
            crate::entity::test_support::payment_term,
        )
        .unwrap()
    }

    fn impact(kind: SourceImpactKind) -> PurchaseSourceImpact {
        PurchaseSourceImpact {
            purchase_order_id: "po-1".into(),
            reasons: vec![SourceImpactReason {
                sales_order_line_id: "sol-1".into(),
                line_no: 2,
                item_name: "礼盒".into(),
                kind,
            }],
        }
    }

    fn reduced() -> SourceImpactKind {
        SourceImpactKind::QuantityReduced {
            baseline_quantity: qty("10.000000"),
            current_quantity: qty("8"),
            excess_quantity: qty("2"),
        }
    }

    fn opened() -> PurchaseSourceUpdate {
        let order = order();
        let impact = impact(reduced());
        PurchaseSourceUpdate::open(
            "psu-1".into(),
            SourceUpdateOpening {
                order: &order,
                impact: &impact,
                sales_order_revision_id: "rev-2",
                trigger_sales_change_order_id: Some("sco-1"),
            },
        )
    }

    /// 开立时冻结采购引用版本、触发变更、原因与付款暂停。
    #[test]
    fn open_freezes_sources_and_payment_block() {
        let update = opened();

        assert_eq!(update.status, PurchaseSourceUpdateStatus::Open);
        assert_eq!(update.purchase_sales_revision_id, "rev-1");
        assert_eq!(update.sales_order_revision_id, "rev-2");
        assert_eq!(update.trigger_sales_change_order_id.as_deref(), Some("sco-1"));
        assert!(update.blocks_payment);
        assert_eq!(update.impact_summary("SO-9"), "销售单 SO-9 已变更：第 2 行 礼盒 数量 10 → 8，超出 2");
    }

    /// 刷新只在事实变化时报告变化，未提供新触发时保留原触发变更。
    #[test]
    fn refresh_reports_changes_and_keeps_trigger() {
        let mut update = opened();

        assert!(!update.refresh(&impact(reduced()), "rev-2", None).unwrap());
        let later = impact(SourceImpactKind::DueEarlier {
            current_due: BusinessDate::from_str("2026-10-10").unwrap(),
            expected_delivery_date: BusinessDate::from_str("2026-10-15").unwrap(),
        });
        assert!(update.refresh(&later, "rev-3", None).unwrap());
        assert!(!update.blocks_payment);
        assert_eq!(update.trigger_sales_change_order_id.as_deref(), Some("sco-1"));
        assert!(update.impact_summary("SO-9").contains("履约期限提前到 2026-10-10，早于预计交付 2026-10-15"));
    }

    /// 草稿修改后转为等待提交：解除付款暂停、拒绝维持登记；再次受影响时恢复待处理。
    #[test]
    fn await_submission_keeps_record_open_until_affected_again() {
        let mut update = opened();

        assert!(update.await_submission("rev-2").unwrap());
        assert!(!update.await_submission("rev-2").unwrap());

        assert_eq!(update.status, PurchaseSourceUpdateStatus::Open);
        assert!(!update.blocks_payment);
        assert_eq!(update.reasons.len(), 1);
        assert_eq!(update.impact_summary("SO-9"), "销售单 SO-9 已变更，采购已按新销售版本修改，请提交审批");
        assert!(update.maintain("供应商不接受减量", "buyer", Instant::from_unix_secs(1)).is_err());
        assert!(update.refresh(&impact(reduced()), "rev-3", None).unwrap());
        assert!(!update.awaiting_submission);
        assert!(update.blocks_payment);
    }

    /// 系统判定完成后不得再次处理或刷新，也不得由系统登记维持。
    #[test]
    fn resolve_is_terminal_and_rejects_maintained() {
        let mut update = opened();
        assert!(
            update.resolve(PurchaseSourceResolution::Maintained, None, Instant::from_unix_secs(1)).is_err()
        );

        update
            .resolve(PurchaseSourceResolution::Changed, Some("pco-1".into()), Instant::from_unix_secs(2))
            .unwrap();

        assert_eq!(update.status, PurchaseSourceUpdateStatus::Resolved);
        assert_eq!(update.resolved_by.as_deref(), Some("__system__"));
        assert_eq!(update.resolution_document_id.as_deref(), Some("pco-1"));
        assert!(update.resolve(PurchaseSourceResolution::Voided, None, Instant::from_unix_secs(3)).is_err());
        assert!(update.refresh(&impact(reduced()), "rev-2", None).is_err());
        assert!(update.await_submission("rev-2").is_err());
        assert_eq!(update.maintained_baseline("rev-1"), None);
    }

    /// 同一采购取最后一次维持登记；采购换版本后维持基准失效。
    #[test]
    fn maintained_baselines_use_latest_registration_for_current_source() {
        let mut first = opened();
        first.maintain("供应商不接受减量", "buyer", Instant::from_unix_secs(1)).unwrap();
        let mut second = opened();
        second.sales_order_revision_id = "rev-3".into();
        second.maintain("仍不接受", "buyer", Instant::from_unix_secs(2)).unwrap();
        let mut rebased = order();
        rebased.sales_order_revision_id = SalesOrderRevisionId::new("rev-3");

        let baselines = super::maintained_baselines(&[order()], &[first.clone(), second.clone()]);
        let after_rebase = super::maintained_baselines(&[rebased], &[first, second]);

        assert_eq!(baselines.get("po-1").map(String::as_str), Some("rev-3"));
        assert!(after_rebase.is_empty());
    }

    /// 维持登记要求原因，并在采购未换版本前提供判定基准。
    #[test]
    fn maintain_requires_note_and_provides_baseline_until_rebased() {
        let mut update = opened();
        assert!(update.maintain("  ", "buyer", Instant::from_unix_secs(1)).is_err());
        assert!(update.maintain(&"长".repeat(501), "buyer", Instant::from_unix_secs(1)).is_err());

        update.maintain(" 供应商不接受减量 ", "buyer", Instant::from_unix_secs(2)).unwrap();

        assert_eq!(update.resolution, Some(PurchaseSourceResolution::Maintained));
        assert_eq!(update.maintain_note.as_deref(), Some("供应商不接受减量"));
        assert_eq!(update.resolved_by.as_deref(), Some("buyer"));
        assert_eq!(update.maintained_baseline("rev-1"), Some("rev-2"));
        assert_eq!(update.maintained_baseline("rev-2"), None);
    }
}
