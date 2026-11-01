//! 客户模板按勾选顺序整体校验，只暴露已授权的商品资料。

use std::collections::BTreeMap;

use erp_catalog::entity::catalog::ProductKind;
use erp_catalog::repository::SellableSkuRow;

use super::read::InquiryTemplateProduct;
use crate::{Error, Result};

impl InquiryTemplateProduct {
    /// 将当前可选商品转换为完整模板，按勾选顺序定位不可用项。
    /// # 参数
    /// `ids` 为已校验的唯一勾选身份，`rows` 为经过当前授权及可售资格重验的行。
    /// # 返回
    /// 全部商品的客户安全字段，顺序与勾选一致。
    /// # 错误
    /// 不可选、卡券或缺少单位时整批拒绝；不可选项不泄露主数据。
    pub(super) fn from_selection(ids: &[String], rows: Vec<SellableSkuRow>) -> Result<Vec<Self>> {
        let mut rows = rows.into_iter().map(|row| (row.sku_id.clone(), row)).collect::<BTreeMap<_, _>>();
        let mut products = Vec::with_capacity(ids.len());
        let mut issues = Vec::new();
        for (index, id) in ids.iter().enumerate() {
            let Some(row) = rows.remove(id) else {
                issues
                    .push(format!("第 {} 个勾选商品不存在或当前不可选用，请刷新商品池后重新选择", index + 1));
                continue;
            };
            let label = format!("{}（{}）", row.name, row.sku_no);
            if row.product_kind == ProductKind::Voucher {
                issues.push(format!("{label}为卡券，本询价模板不支持卡券，请取消勾选"));
                continue;
            }
            let Some(unit) = row.base_unit_code.filter(|code| !code.trim().is_empty()) else {
                issues.push(format!("{label}缺少有效的计量单位，请维护单位后重新下载"));
                continue;
            };
            products.push(Self {
                sku_id: row.sku_id,
                sku_code: row.sku_no,
                product_name: row.name,
                specification: row.specification,
                unit,
            });
        }
        if !issues.is_empty() {
            let count = issues.len();
            let details = issues.into_iter().take(5).collect::<Vec<_>>().join("；");
            return Err(Error::ValidationError(format!(
                "有 {count} 个商品无法生成询价模板，本次未下载：{details}{}",
                if count > 5 { "；仅显示前 5 项，请处理后重试" } else { "" }
            )));
        }
        Ok(products)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, kind: ProductKind, unit: Option<&str>) -> SellableSkuRow {
        serde_json::from_value(serde_json::json!({
            "sku_id": id, "sku_version": 1, "sku_revision_id": "revision", "sku_revision_no": 1,
            "sku_no": format!("SKU-{id}"), "product_id": "product", "product_no": "P-1",
            "product_kind": kind, "name": format!("商品{id}"), "specification_signature": "",
            "base_unit_id": "unit", "base_unit_code": unit, "sales_visible_price_gross": "10.00",
            "effective_from": "2026-10-01", "supplier_count": 1
        }))
        .unwrap()
    }

    fn ids(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).into()).collect()
    }

    #[test]
    fn batch_template_keeps_all_supported_products_in_selection_order() {
        let result = InquiryTemplateProduct::from_selection(
            &ids(&["service", "physical", "virtual"]),
            vec![
                row("physical", ProductKind::Physical, Some("PCS")),
                row("virtual", ProductKind::Virtual, Some("PCS")),
                row("service", ProductKind::OfflineService, Some("TIME")),
            ],
        )
        .unwrap();
        assert_eq!(
            result.iter().map(|row| row.sku_id.as_str()).collect::<Vec<_>>(),
            ["service", "physical", "virtual"]
        );
        assert_eq!(result[0].unit, "TIME");
    }

    #[test]
    fn mixed_selection_identifies_voucher_and_missing_unit_without_partial_download() {
        let error = InquiryTemplateProduct::from_selection(
            &ids(&["valid", "voucher", "unit", "missing"]),
            vec![
                row("valid", ProductKind::Physical, Some("PCS")),
                row("voucher", ProductKind::Voucher, Some("PCS")),
                row("unit", ProductKind::Physical, None),
            ],
        )
        .unwrap_err();
        let Error::ValidationError(message) = error else { panic!("expected validation error") };
        assert!(message.contains("有 3 个商品"));
        assert!(message.contains("SKU-voucher）为卡券"));
        assert!(message.contains("SKU-unit）缺少有效的计量单位"));
        assert!(message.contains("第 4 个勾选商品不存在或当前不可选用"));
        assert!(!message.contains("SKU-valid"));
        assert!(!message.contains("missing"));
    }

    #[test]
    fn whitespace_unit_is_rejected_and_diagnostics_are_bounded() {
        let requested = (0..6).map(|i| i.to_string()).collect::<Vec<_>>();
        let rows = requested.iter().map(|id| row(id, ProductKind::Physical, Some("  "))).collect();
        let message = InquiryTemplateProduct::from_selection(&requested, rows).unwrap_err().to_string();
        assert!(message.contains("有 6 个商品"));
        assert!(message.contains("SKU-4"));
        assert!(!message.contains("SKU-5"));
        assert!(message.contains("仅显示前 5 项"));
    }
}
