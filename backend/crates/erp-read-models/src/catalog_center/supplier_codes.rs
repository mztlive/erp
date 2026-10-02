//! 公司商品池仅补齐当前有效供给的供应商业务编号。

use std::collections::{BTreeSet, HashMap};

use erp_catalog::SellableSkuView;
use erp_core::ids::SupplierAccountId;
use erp_supplier::repository::{SupplierAccountRepositoryExt, SupplierExt};
use persistence_core::NoTransaction;

use super::CatalogCenterReadService;
use crate::Result;

impl CatalogCenterReadService {
    /// 按当前商品池页内的有效供应商身份批量读取编号。
    ///
    /// # 参数
    /// * `row_ids` - 每行当前有效供给的供应商身份
    ///
    /// # 返回
    /// 返回未删除供应商的编号映射，不读取完整供应商资料。
    ///
    /// # 错误
    /// 编号仓储读取失败时拒绝整页，避免展示不完整的编号结论。
    pub(super) async fn supplier_numbers(&self, row_ids: &[Vec<String>]) -> Result<HashMap<String, String>> {
        let ids = row_ids.iter().flatten().cloned().collect::<BTreeSet<_>>();
        let Some(db) = self.db.as_ref().filter(|_| !ids.is_empty()) else {
            return Ok(HashMap::new());
        };
        let ids = ids.into_iter().map(SupplierAccountId::from).collect::<Vec<_>>();
        Ok(db.supplier_accounts().supplier_numbers_by_ids(&ids, &mut NoTransaction).await?)
    }
}

/// 为每行补齐去重、排序后的供应商业务编号，不返回内部身份。
///
/// # 参数
/// * `rows` - 授权商品池页
/// * `row_ids` - 同序页行的有效供给供应商身份
/// * `numbers` - 供应商业务编号映射
///
/// # 返回
/// 原地补齐可读取的编号；缺失或已删除的供应商不补编号。
pub(super) fn apply_supplier_codes(
    rows: &mut [SellableSkuView],
    row_ids: &[Vec<String>],
    numbers: &HashMap<String, String>,
) {
    for (row, ids) in rows.iter_mut().zip(row_ids) {
        row.supplier_codes = codes_for(ids, numbers);
    }
}

/// 根据有效供给身份选择业务编号，保持稳定排序并保留编号前导零。
fn codes_for(ids: &[String], numbers: &HashMap<String, String>) -> Vec<String> {
    ids.iter().filter_map(|id| numbers.get(id)).cloned().collect::<BTreeSet<_>>().into_iter().collect()
}

#[cfg(test)]
mod tests {
    use erp_catalog::repository::SellableSkuRow;
    use erp_catalog::service::catalog::{prepare_sellable_sku_list, sellable_sku_page_view};
    use persistence_core::PageResult;
    use serde_json::json;

    use super::*;

    #[test]
    fn codes_include_only_effective_suppliers_and_preserve_leading_zeroes() {
        let numbers = [
            ("supplier-a".into(), "0002".into()),
            ("supplier-b".into(), "0001".into()),
            ("unrelated".into(), "0003".into()),
        ]
        .into();
        let ids = vec!["supplier-a".into(), "supplier-b".into(), "supplier-a".into(), "deleted".into()];
        assert_eq!(codes_for(&ids, &numbers), vec!["0001", "0002"]);
        assert!(codes_for(&[], &numbers).is_empty());
    }

    #[test]
    fn enrichment_preserves_price_facts_and_row_association_without_internal_ids() {
        let rows = ["sku-a", "sku-b"]
            .map(|id| {
                serde_json::from_value::<SellableSkuRow>(json!({
            "sku_id": id, "sku_version": 1, "sku_revision_id": format!("{id}-revision"),
            "sku_revision_no": 1, "sku_no": id, "product_id": "product", "product_no": "P-1",
            "product_kind": "PHYSICAL", "name": "礼盒", "specification_signature": "",
            "base_unit_id": "unit", "factory_price_gross": "6.00",
            "sales_visible_price_gross": "10.00", "bulk_price_gross": "8.00", "bulk_min_quantity": "10",
            "market_price": "12.00", "effective_from": "2026-01-01", "supplier_count": 1
        })).unwrap()
            })
            .to_vec();
        let params = serde_json::from_value(json!({"eligibility_as_of": "2026-10-03"})).unwrap();
        let filter = prepare_sellable_sku_list(&params).unwrap();
        let mut view = sellable_sku_page_view(PageResult { items: rows, total: 2 }, &filter).unwrap();
        let ids = vec![vec!["supplier-a".into()], vec!["supplier-b".into()]];
        let numbers = [("supplier-a".into(), "0002".into()), ("supplier-b".into(), "0001".into())].into();
        apply_supplier_codes(&mut view.items, &ids, &numbers);
        assert_eq!(view.items[0].supplier_codes, vec!["0002"]);
        assert_eq!(view.items[1].supplier_codes, vec!["0001"]);
        let value = serde_json::to_value(&view.items[0]).unwrap();
        assert_eq!(value["factory_price_gross"], "6.00");
        assert_eq!(value["bulk_price_gross"], "8.00");
        assert!(value.get("supplier_ids").is_none());
    }
}
