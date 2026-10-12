//! 管理端销售方案开单预填；不得复用到公开回执。
use erp_core::money::Amount;
use serde::Serialize;

use crate::entity::sales_order::BusinessType;
use crate::entity::sales_order::selection_source::SalesSelectionSource;
use crate::entity::sales_selection::{
    SalesSelectionProposal, SalesSelectionProposalSkuLine, SpecificationAttributeSnapshot,
};
use crate::{Error, Result};

/// 方案带入的开单内容；商业条款与交付信息由销售补齐。
#[derive(Debug, Serialize)]
pub struct SelectionOrderBasis {
    pub source: SalesSelectionSource,
    pub customer_id: String,
    pub customer_name: String,
    pub lines: Vec<SelectionOrderLine>,
}

/// 保留来源行身份及冻结 SKU 修订、数量和报价。
#[derive(Debug, Serialize)]
pub struct SelectionOrderLine {
    pub id: String,
    pub sku_id: String,
    pub sku_revision_id: String,
    pub name: String,
    pub specification: Vec<SpecificationAttributeSnapshot>,
    pub unit: String,
    pub quantity: u32,
    pub unit_price: Amount,
}

impl SelectionOrderBasis {
    /// 从已授权方案及其完整 SKU 行构造预填。
    /// # 参数
    /// `proposal` 为方案，`lines` 为该方案 SKU 明细，套餐重复 SKU 保持独立行。
    /// # 返回
    /// 返回开单客户、来源和明细。
    /// # 错误
    /// 模式不支持、明细为空、归属不符或数量无效时拒绝。
    pub fn from_proposal(
        proposal: &SalesSelectionProposal,
        lines: Vec<SalesSelectionProposalSkuLine>,
    ) -> Result<Self> {
        let source =
            SalesSelectionSource::from_proposal(proposal, &proposal.customer_id, BusinessType::GoodsService)?;
        if lines.is_empty() {
            return Err(Error::ValidationError("销售方案没有可开单的商品明细".into()));
        }
        let lines = lines
            .into_iter()
            .map(|line| SelectionOrderLine::from_line(proposal, line))
            .collect::<Result<_>>()?;
        Ok(Self {
            source,
            customer_id: proposal.customer_id.to_string(),
            customer_name: proposal.customer_name.clone(),
            lines,
        })
    }
}

impl SelectionOrderLine {
    fn from_line(proposal: &SalesSelectionProposal, line: SalesSelectionProposalSkuLine) -> Result<Self> {
        let quantity = line
            .quantity
            .filter(|value| *value > 0)
            .ok_or_else(|| Error::ValidationError("销售方案商品数量无效".into()))?;
        if line.proposal_id.as_ref() != proposal.base.id
            || line.sku_id.is_empty()
            || line.sku_revision_id.is_empty()
        {
            return Err(Error::ValidationError("销售方案商品来源不完整".into()));
        }
        Ok(Self {
            id: line.base.id,
            sku_id: line.sku_id,
            sku_revision_id: line.sku_revision_id,
            name: line.name,
            specification: line.specification,
            unit: line.unit,
            quantity,
            unit_price: line.unit_price,
        })
    }
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use erp_core::common::time::Instant;
    use erp_core::ids::{
        CustomerAccountId, SalesSelectionBookletId, SalesSelectionDisplayItemId, SalesSelectionProposalId,
    };

    use super::*;
    use crate::entity::sales_selection::{SalesSelectionProposalData, SelectionForm, SubmitMode};

    fn proposal() -> SalesSelectionProposal {
        SalesSelectionProposal::new(
            SalesSelectionProposalId::new("proposal"),
            SalesSelectionProposalData {
                proposal_no: "XP-1".into(),
                customer_id: CustomerAccountId::new("customer"),
                customer_name: "客户".into(),
                booklet_id: SalesSelectionBookletId::new("book"),
                sales_owner_user_id: "sales".into(),
                business_org_unit_id: "org".into(),
                batch_id: "batch".into(),
                form: SelectionForm::Package,
                submit_mode: SubmitMode::ByQuantity,
                participant_id: String::new(),
                recipient: None,
                session_version: 1,
                submitted_at: Instant::now(),
                total_amount: Some("576.00".parse().unwrap()),
            },
        )
        .unwrap()
    }

    fn line(id: &str) -> SalesSelectionProposalSkuLine {
        SalesSelectionProposalSkuLine {
            base: BaseModel::new(id.to_string()),
            proposal_id: SalesSelectionProposalId::new("proposal"),
            display_item_id: SalesSelectionDisplayItemId::new(id),
            sku_id: "sku".into(),
            sku_revision_id: "revision".into(),
            name: "商品".into(),
            specification: vec![],
            unit: "箱".into(),
            unit_price: "96.00".parse().unwrap(),
            quantity: Some(3),
            line_amount: Some("288.00".parse().unwrap()),
        }
    }

    #[test]
    fn package_preserves_duplicate_skus_prices_and_source_rows() {
        let basis =
            SelectionOrderBasis::from_proposal(&proposal(), vec![line("first"), line("second")]).unwrap();
        assert_eq!(basis.source.proposal_no, "XP-1");
        assert_eq!(basis.customer_id, "customer");
        assert_eq!(basis.lines.len(), 2);
        assert_ne!(basis.lines[0].id, basis.lines[1].id);
        assert_eq!(basis.lines[0].sku_id, basis.lines[1].sku_id);
        assert_eq!(basis.lines[0].quantity, 3);
        assert_eq!(basis.lines[0].unit_price, "96.00".parse().unwrap());
        assert_eq!(basis.lines[0].sku_revision_id, "revision");
    }

    #[test]
    fn rejects_other_modes_customer_and_business_type() {
        let mut proposal = proposal();
        assert!(
            SalesSelectionSource::from_proposal(
                &proposal,
                &CustomerAccountId::new("other"),
                BusinessType::GoodsService
            )
            .is_err()
        );
        assert!(
            SalesSelectionSource::from_proposal(&proposal, &proposal.customer_id, BusinessType::Voucher)
                .is_err()
        );
        for mode in [SubmitMode::MallRedeem, SubmitMode::PickupVoucher] {
            proposal.submit_mode = mode;
            assert!(SelectionOrderBasis::from_proposal(&proposal, vec![line("line")]).is_err());
        }
    }

    #[test]
    fn rejects_missing_foreign_or_invalid_lines() {
        assert!(SelectionOrderBasis::from_proposal(&proposal(), vec![]).is_err());
        for quantity in [None, Some(0)] {
            let mut line = line("line");
            line.quantity = quantity;
            assert!(SelectionOrderBasis::from_proposal(&proposal(), vec![line]).is_err());
        }
        let mut foreign = line("line");
        foreign.proposal_id = SalesSelectionProposalId::new("other");
        assert!(SelectionOrderBasis::from_proposal(&proposal(), vec![foreign]).is_err());
    }
    #[test]
    fn source_survives_order_round_trip_and_legacy_orders_default_to_none() {
        use erp_core::ids::{ContractId, PartyId, SalesOrderId};

        use crate::entity::sales_order::{OriginSystem, SalesOrder, SalesOrderData};
        let mut order = SalesOrder::new(
            SalesOrderId::new("order"),
            SalesOrderData {
                sales_owner_user_id: "sales".into(),
                business_org_unit_id: "org".into(),
                order_no: "XS-1".into(),
                business_type: BusinessType::GoodsService,
                origin_system: OriginSystem::Erp,
                source_identity_id: None,
                source_status_code: None,
                customer_id: CustomerAccountId::new("customer"),
                contract_id: Some(ContractId::new("contract")),
                settlement_party_id: PartyId::new("party"),
            },
            "sales",
        )
        .unwrap();
        order.base = BaseModel::fake();
        order.selection_source = Some(
            SalesSelectionSource::from_proposal(&proposal(), &order.customer_id, order.business_type)
                .unwrap(),
        );
        let encoded = serde_json::to_value(&order).unwrap();
        let restored: SalesOrder = serde_json::from_value(encoded.clone()).unwrap();
        assert_eq!(restored, order);
        let mut legacy = encoded;
        legacy.as_object_mut().unwrap().remove("selection_source");
        let legacy: SalesOrder = serde_json::from_value(legacy).unwrap();
        assert!(legacy.selection_source.is_none());
    }
}
