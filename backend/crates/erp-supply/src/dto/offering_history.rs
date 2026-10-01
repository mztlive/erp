//! 供给不可变条款的只读历史合同，不携带当前可供数量。
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::supplier_offering::SupplierOfferingRevision;

/// 条款历史游标；每次固定返回最多 20 个版本。
#[derive(Debug, Clone, Default, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct OfferingHistoryParams {
    /// 只读取此版本号之前的版本；首次请求省略。
    #[validate(range(min = 1))]
    pub before_revision_no: Option<u32>,
}

/// 一页历史条款。
#[derive(Debug, Serialize)]
pub struct OfferingHistoryPage {
    /// 按版本号倒序的条款。
    pub items: Vec<OfferingRevisionView>,
    /// 下一页游标；没有更多版本时为空。
    pub next_before_revision_no: Option<u32>,
}

/// 单个不可变商业条款；无成本权限时金额与税率均为空。
#[derive(Debug, Serialize)]
pub struct OfferingRevisionView {
    /// 条款版本 ID。
    pub id: String,
    /// 条款版本号。
    pub revision_no: u32,
    /// 版本记录时间。
    pub created_at: u64,
    /// 是否为当前指针指向的版本。
    pub is_current: bool,
    /// 一件代发含税价。
    pub dropship_supply_price_gross: Option<String>,
    /// 一件代发不含税价。
    pub dropship_supply_price_net: Option<String>,
    /// 集采含税价。
    pub bulk_supply_price_gross: Option<String>,
    /// 集采不含税价。
    pub bulk_supply_price_net: Option<String>,
    /// 进项税率。
    pub input_tax_rate: Option<String>,
    /// 集采起订量。
    pub bulk_minimum_order_quantity: String,
    /// 供应区域。
    pub supply_region: Vec<String>,
    /// 商品能力。
    pub product_capabilities: Vec<String>,
    /// 快递说明。
    pub dropship_express: Option<String>,
    /// 运费。
    pub freight_amount: Option<String>,
    /// 服务费。
    pub service_fee_amount: Option<String>,
    /// 生效日期。
    pub valid_from: String,
    /// 失效日期。
    pub valid_to: Option<String>,
}
impl OfferingRevisionView {
    /// 将存储版本映射为只读条款，不拼入实时可供情况。
    ///
    /// # 参数
    /// * `revision` - 已授权供给下的不可变条款
    /// * `current_id` - 供给的当前版本指针
    ///
    /// # 返回
    /// 返回商业字段与当前版本标记。
    ///
    /// # 错误
    /// 无。
    pub fn from_revision(revision: SupplierOfferingRevision, current_id: Option<&str>) -> Self {
        Self {
            is_current: current_id == Some(revision.base.id.as_str()),
            id: revision.base.id,
            revision_no: revision.revision.revision_no,
            created_at: revision.base.created_at,
            dropship_supply_price_gross: Some(revision.dropship_supply_price_gross.to_string()),
            dropship_supply_price_net: Some(revision.dropship_supply_price_net.to_string()),
            bulk_supply_price_gross: Some(revision.bulk_supply_price_gross.to_string()),
            bulk_supply_price_net: Some(revision.bulk_supply_price_net.to_string()),
            input_tax_rate: Some(revision.input_tax_rate.to_string()),
            bulk_minimum_order_quantity: revision.bulk_minimum_order_quantity.to_string(),
            supply_region: revision.supply_region,
            product_capabilities: revision.product_capabilities,
            dropship_express: revision.dropship_express,
            freight_amount: revision.freight_amount.map(|value| value.to_string()),
            service_fee_amount: revision.service_fee_amount.map(|value| value.to_string()),
            valid_from: revision.valid_from.to_string(),
            valid_to: revision.valid_to.map(|value| value.to_string()),
        }
    }

    /// 清除当前调用人无权读取的采购成本字段。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 原地清除价格、税率和费用，保留非成本条款。
    ///
    /// # 错误
    /// 无。
    pub fn redact_costs(&mut self) {
        self.dropship_supply_price_gross = None;
        self.dropship_supply_price_net = None;
        self.bulk_supply_price_gross = None;
        self.bulk_supply_price_net = None;
        self.input_tax_rate = None;
        self.freight_amount = None;
        self.service_fee_amount = None;
    }
}

impl OfferingHistoryPage {
    /// 从仓储返回的探测页构造对外条款页。
    ///
    /// # 参数
    /// * `revisions` - 按版本倒序的至多 21 条记录
    /// * `current_id` - 供给当前版本指针
    ///
    /// # 返回
    /// 至多 20 条与下一页游标；尾页游标为空。
    ///
    /// # 错误
    /// 无。
    pub fn from_revisions(mut revisions: Vec<SupplierOfferingRevision>, current_id: Option<&str>) -> Self {
        let has_more = revisions.len() > 20;
        revisions.truncate(20);
        let next_before_revision_no =
            if has_more { revisions.last().map(|row| row.revision.revision_no) } else { None };
        let items = revisions
            .into_iter()
            .map(|revision| OfferingRevisionView::from_revision(revision, current_id))
            .collect();
        Self { items, next_before_revision_no }
    }
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use erp_core::ids::SupplierOfferingRevisionId;

    use super::*;
    use crate::entity::supplier_offering::SupplierOfferingRevisionData;

    fn revision(no: u32) -> SupplierOfferingRevision {
        let data: SupplierOfferingRevisionData = serde_json::from_value(serde_json::json!({
            "supplier_offering_id": "offering-a", "revision_no": no,
            "dropship_supply_price_gross": "11.30", "dropship_supply_price_net": "9.83",
            "bulk_supply_price_gross": "9.04", "bulk_supply_price_net": "7.86",
            "input_tax_rate": "0.13", "bulk_minimum_order_quantity": "10",
            "freight_amount": "1", "service_fee_amount": "0", "dropship_express": "顺丰",
            "supply_region": ["全国"], "product_capabilities": ["REFUND"],
            "valid_from": "2026-10-01", "valid_to": null,
            "prefill_source_refs": {}
        }))
        .unwrap();
        let mut revision =
            SupplierOfferingRevision::new(SupplierOfferingRevisionId::new(format!("revision-{no}")), data)
                .unwrap();
        revision.base = BaseModel { id: format!("revision-{no}"), ..BaseModel::fake() };
        revision
    }

    #[test]
    fn history_cursor_preserves_revision_order_and_current_pointer() {
        let rows = (1..=21).rev().map(revision).collect();
        let page = OfferingHistoryPage::from_revisions(rows, Some("revision-20"));
        assert_eq!(page.items.len(), 20);
        assert_eq!(page.next_before_revision_no, Some(2));
        assert_eq!(page.items[0].revision_no, 21);
        assert!(!page.items[0].is_current);
        assert!(page.items[1].is_current);
        let tail = OfferingHistoryPage::from_revisions(vec![revision(1)], Some("revision-20"));
        assert_eq!(tail.next_before_revision_no, None);
        assert!(!tail.items[0].is_current);
        assert!(OfferingHistoryPage::from_revisions(vec![], None).items.is_empty());
        assert_eq!(
            OfferingHistoryPage::from_revisions((1..=20).rev().map(revision).collect(), None)
                .next_before_revision_no,
            None
        );
    }

    #[test]
    fn history_redacts_all_cost_fields_without_losing_non_cost_terms() {
        let mut view = OfferingRevisionView::from_revision(revision(1), Some("revision-1"));
        assert!(view.freight_amount.is_some());
        let minimum = view.bulk_minimum_order_quantity.clone();
        view.redact_costs();
        let json = serde_json::to_value(view).unwrap();
        for key in [
            "dropship_supply_price_gross",
            "dropship_supply_price_net",
            "bulk_supply_price_gross",
            "bulk_supply_price_net",
            "input_tax_rate",
            "freight_amount",
            "service_fee_amount",
        ] {
            assert!(json[key].is_null(), "{key}");
        }
        assert_eq!(json["bulk_minimum_order_quantity"], minimum);
        assert_eq!(json["supply_region"], serde_json::json!(["全国"]));
        assert_eq!(json["valid_from"], "2026-10-01");
        assert!(json.get("available_quantity").is_none());
        assert!(json.get("availability_status").is_none());
    }

    #[test]
    fn history_rejects_invalid_or_unknown_cursor_parameters() {
        assert!(OfferingHistoryParams { before_revision_no: Some(0) }.validate().is_err());
        assert!(OfferingHistoryParams { before_revision_no: Some(1) }.validate().is_ok());
        assert!(
            serde_json::from_value::<OfferingHistoryParams>(serde_json::json!({"offering_id": "other"}))
                .is_err()
        );
    }
}
