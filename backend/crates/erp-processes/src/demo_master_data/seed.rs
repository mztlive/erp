//! JSON 种子复用领域创建输入；运行时 ID 在写入前解析。

use std::collections::{HashMap, HashSet};

use erp_catalog::{
    CreateProductBrandRequest, CreateProductCategoryRequest, CreateProductRequest,
    CreateUnitOfMeasureRequest, ProductKind,
};
use erp_customer::SaveCustomerProfileRequest;
use erp_supplier::SaveSupplierProfileRequest;
use erp_warehouse::CreateWarehouseRequest;
use serde::Deserialize;
use validator::Validate;

use super::plan::{DemoKind, DemoStep};
use crate::{Error, Result};

/// 各类固定内容直接使用领域 DTO。
#[derive(Clone, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub(super) enum SeedRequest {
    Unit(CreateUnitOfMeasureRequest),
    Brand(CreateProductBrandRequest),
    Category(CreateProductCategoryRequest),
    Warehouse(CreateWarehouseRequest),
    Customer(SaveCustomerProfileRequest),
    Supplier(Box<SaveSupplierProfileRequest>),
    Product(CreateProductRequest),
}

impl SeedRequest {
    /// 返回种类及跨版本稳定的种子键。
    ///
    /// # 参数
    /// 无；读取当前种子。
    ///
    /// # 返回
    /// 返回种类和稳定种子键。
    ///
    /// # 错误
    /// 无。
    pub(super) fn identity(&self) -> (DemoKind, &str) {
        match self {
            Self::Unit(row) => (DemoKind::Unit, &row.unit_code),
            Self::Brand(row) => (DemoKind::Brand, &row.brand_code),
            Self::Category(row) => (DemoKind::Category, &row.category_code),
            Self::Warehouse(row) => (DemoKind::Warehouse, &row.warehouse_code),
            Self::Customer(row) => (DemoKind::Customer, &row.idempotency_key),
            Self::Supplier(row) => (DemoKind::Supplier, &row.idempotency_key),
            Self::Product(row) => (DemoKind::Product, &row.product_no),
        }
    }

    /// 返回领域输入中的显示名称。
    ///
    /// # 参数
    /// 无；读取当前种子。
    ///
    /// # 返回
    /// 返回领域创建请求中的显示名称。
    ///
    /// # 错误
    /// 无。
    pub(super) fn label(&self) -> &str {
        match self {
            Self::Unit(row) => &row.name,
            Self::Brand(row) => &row.name,
            Self::Category(row) => &row.name,
            Self::Warehouse(row) => &row.name,
            Self::Customer(row) => &row.legal_name,
            Self::Supplier(row) => &row.legal_name,
            Self::Product(row) => &row.name,
        }
    }

    /// 在任何写入前验证领域输入及引用顺序。
    fn validate(&self, previous: &HashMap<String, DemoKind>) -> Result<()> {
        let validation = match self {
            Self::Unit(row) => row.validate(),
            Self::Brand(row) => row.validate(),
            Self::Category(row) => row.validate(),
            Self::Warehouse(row) => row.validate(),
            Self::Customer(row) => row.validate(),
            Self::Supplier(row) => row.validate(),
            Self::Product(row) => row.validate(),
        };
        validation.map_err(|error| Error::ValidationError(format!("演示种子字段无效：{error}")))?;
        match self {
            Self::Product(row) => validate_product(row, previous)?,
            Self::Category(row) if row.parent_category_id.is_some() => {
                return Err(Error::ValidationError("演示分类必须为根分类".into()));
            },
            Self::Supplier(row) => validate_supplier(row)?,
            Self::Warehouse(row) => {
                placeholder(&row.inbound_handler_user_id, "$warehouse_handler")?;
                placeholder(&row.outbound_handler_user_id, "$warehouse_handler")?;
            },
            _ => {},
        }
        Ok(())
    }
}

/// 解析全量清单，拒绝重复身份、非法字段和前置引用缺失。
///
/// # 参数
/// `json` - 完整种子 JSON 文本。
///
/// # 返回
/// 返回按依赖顺序校验完成的种子。
///
/// # 错误
/// 解析失败、字段非法、身份重复或引用缺失时返回错误。
pub(super) fn load(json: &str) -> Result<Vec<DemoStep>> {
    let requests: Vec<SeedRequest> = serde_json::from_str(json)
        .map_err(|error| Error::ValidationError(format!("演示种子 JSON 无效：{error}")))?;
    let mut known = HashMap::new();
    let mut sku_numbers = HashSet::new();
    let mut steps = Vec::new();
    for request in requests {
        request.validate(&known)?;
        let (kind, key) = request.identity();
        let key = key.to_string();
        if key.trim().is_empty() || known.insert(key.clone(), kind).is_some() {
            return Err(Error::ValidationError(format!("演示种子身份为空或重复：{key}")));
        }
        if let SeedRequest::Product(product) = &request {
            for sku in &product.skus {
                if !sku_numbers.insert(sku.sku_no.trim().to_string()) {
                    return Err(Error::ValidationError(format!("演示 SKU 编号重复：{}", sku.sku_no)));
                }
            }
        }
        steps.push(DemoStep { kind, key, request });
    }
    Ok(steps)
}

/// 商品只允许引用前面已声明的字典种子。
fn validate_product(row: &CreateProductRequest, previous: &HashMap<String, DemoKind>) -> Result<()> {
    if row.product_kind == ProductKind::Voucher {
        super::voucher::voucher_request(row)?;
    }
    reference(previous, row.brand_id.as_ref(), DemoKind::Brand)?;
    reference(previous, row.category_id.as_ref(), DemoKind::Category)?;
    for sku in &row.skus {
        reference(previous, sku.base_unit_id.as_ref(), DemoKind::Unit)?;
        if sku.sku_id.is_some() || sku.expected_sku_revision_id.is_some() || sku.reenable {
            return Err(Error::ValidationError("演示种子不能引用已有 SKU ID".into()));
        }
    }
    Ok(())
}

/// 校验供应商占位引用及嵌套输入合同。
fn validate_supplier(row: &SaveSupplierProfileRequest) -> Result<()> {
    row.validate_contract()?;
    placeholder(row.signing_entity_party_id.as_ref(), "$company")?;
    placeholder(row.payment_entity_party_id.as_ref(), "$company")?;
    placeholder(row.maintainer_user_id.as_deref().unwrap_or_default(), "$supplier_maintainer")?;
    for owner in &row.capability_owners {
        placeholder(&owner.owner_user_id, "$supplier_maintainer")?;
    }
    Ok(())
}

/// 拒绝意外写入某个环境的真实账号或公司 ID。
fn placeholder(value: &str, expected: &str) -> Result<()> {
    if value != expected {
        return Err(Error::ValidationError(format!("演示种子运行时引用必须为 {expected}")));
    }
    Ok(())
}

/// 引用必须存在、种类匹配且先于使用方出现。
fn reference(previous: &HashMap<String, DemoKind>, key: &str, kind: DemoKind) -> Result<()> {
    if previous.get(key) != Some(&kind) {
        return Err(Error::ValidationError(format!("演示种子引用未定义或种类不匹配：{key}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use erp_catalog::entity::catalog::SkuSalesPrices;
    use erp_core::ids::{SupplierAccountId, SupplierCommercialProfileRevisionId};
    use erp_core::money::Quantity;
    use erp_supplier::entity::supplier::{
        SupplierCommercialProfileRevision, SupplierCommercialProfileRevisionData,
    };
    use serde_json::{Value, json};

    use super::{SeedRequest, load};
    use crate::demo_master_data::plan::{apply_window, count_records, planned_counts};

    #[test]
    fn embedded_seed_preserves_catalog_and_business_values() {
        let steps = load(include_str!("master-data.json")).unwrap();
        let counts = planned_counts(&steps);
        assert_eq!((counts.unit, counts.brand, counts.category, counts.warehouse), (6, 8, 13, 6));
        assert_eq!((counts.customer, counts.supplier, counts.product), (24, 28, 28));
        let SeedRequest::Product(product) = &steps.last().unwrap().request else {
            panic!("商品必须最后生成")
        };
        assert_eq!(product.skus[0].sales_visible_price_gross.unwrap().to_string(), "100.00");
        assert!(product.skus[0].factory_price_gross.is_none());
        assert!(product.skus[0].bulk_price_gross.is_none());
        assert!(product.skus[0].bulk_min_quantity.is_none());
        assert_eq!(product.skus[0].sku_no, "DEMO-MD-P-27");
        assert!(steps.iter().all(|step| !step.request.label().is_empty()));
    }

    /// 用声明的公司价执行正式数量取价规则，并保留缺少可选价格的对照规格。
    #[test]
    fn demo_multispec_prices_cover_bulk_threshold_and_fallback() {
        let steps = load(include_str!("master-data.json")).unwrap();
        let SeedRequest::Product(product) =
            &steps.iter().find(|step| step.key == "DEMO-MD-P-28").unwrap().request
        else {
            panic!("多规格演示商品")
        };
        assert_eq!(product.skus.len(), 2);
        assert_ne!(
            serde_json::to_value(&product.skus[0].spec_entries).unwrap(),
            serde_json::to_value(&product.skus[1].spec_entries).unwrap()
        );
        let standard = &product.skus[0];
        assert_eq!(standard.factory_price_gross.unwrap().to_string(), "77.00");
        assert_eq!(standard.market_price.unwrap().to_string(), "159.00");
        let prices = SkuSalesPrices {
            sales_visible_price_gross: standard.sales_visible_price_gross,
            bulk_price_gross: standard.bulk_price_gross,
            bulk_min_quantity: standard.bulk_min_quantity,
        };
        assert_eq!(prices.reference_price("9".parse::<Quantity>().unwrap()).unwrap().to_string(), "129.00");
        assert_eq!(prices.reference_price("10".parse::<Quantity>().unwrap()).unwrap().to_string(), "119.00");
        let light = &product.skus[1];
        assert!(light.factory_price_gross.is_none() && light.market_price.is_none());
        let fallback = SkuSalesPrices {
            sales_visible_price_gross: light.sales_visible_price_gross,
            bulk_price_gross: light.bulk_price_gross,
            bulk_min_quantity: light.bulk_min_quantity,
        };
        assert_eq!(
            fallback.reference_price("100".parse::<Quantity>().unwrap()).unwrap().to_string(),
            "99.00"
        );
    }

    /// 每条供应商输入均须通过真实商务版本工厂，并覆盖全部固定条件与自然周期。
    #[test]
    fn demo_suppliers_cover_payment_terms_with_valid_commercial_revisions() {
        let steps = load(include_str!("master-data.json")).unwrap();
        let mut terms = HashSet::new();
        for step in &steps {
            let SeedRequest::Supplier(row) = &step.request else { continue };
            let revision = SupplierCommercialProfileRevision::new(
                SupplierCommercialProfileRevisionId::new("validation"),
                SupplierCommercialProfileRevisionData {
                    supplier_id: SupplierAccountId::new(&step.key),
                    revision_no: 1,
                    settlement_mode: row.settlement_mode,
                    reconciliation_cycle: row.reconciliation_cycle,
                    payment_term_snapshot: row.payment_term_snapshot.clone(),
                    business_category: row.business_category.clone(),
                    invoice_type: row.invoice_type,
                    invoice_tax_rate: row.invoice_tax_rate,
                    invoice_tax_rates: row.invoice_tax_rates.clone(),
                    signing_entity_party_id: row.signing_entity_party_id.clone(),
                    payment_entity_party_id: row.payment_entity_party_id.clone(),
                    change_reason: row.change_reason.clone(),
                },
            )
            .unwrap();
            terms.insert(revision.effective_payment_term_code());
        }
        assert_eq!(
            terms,
            [
                "PREPAY_100",
                "PREPAY_50",
                "PREPAY_30",
                "CASH_ON_APPROVAL",
                "POSTPAY_NET15",
                "POSTPAY_NET30",
                "PERIOD_WEEK_7",
                "PERIOD_MONTH_15",
                "PERIOD_QUARTER_15",
                "PERIOD_HALF_YEAR_30",
                "PERIOD_YEAR_30",
            ]
            .map(str::to_string)
            .into_iter()
            .collect()
        );
    }

    #[test]
    fn invalid_seed_fails_before_execution() {
        let original: Value = serde_json::from_str(include_str!("master-data.json")).unwrap();
        let mut duplicated = original.clone();
        duplicated.as_array_mut().unwrap().push(original[0].clone());
        assert!(load(&duplicated.to_string()).is_err());
        let mut bad_reference = original.clone();
        bad_reference.as_array_mut().unwrap().last_mut().unwrap()["data"]["brand_id"] = json!("missing");
        assert!(load(&bad_reference.to_string()).is_err());
        let mut bad_price = original;
        bad_price.as_array_mut().unwrap().last_mut().unwrap()["data"]["skus"][0]["sales_visible_price_gross"] =
            json!("NaN");
        assert!(load(&bad_price.to_string()).is_err());
        assert!(load("not json").is_err());
        assert!(load("[]").unwrap().is_empty());
    }

    /// SKU 编号跨商品唯一，不能因供给集合按编号归组而漏验重复规格。
    #[test]
    fn duplicate_sku_number_across_products_fails_before_execution() {
        let mut rows: Value = serde_json::from_str(include_str!("master-data.json")).unwrap();
        let mut duplicate = rows
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["data"]["product_no"] == "DEMO-MD-P-01")
            .unwrap()
            .clone();
        duplicate["data"]["product_no"] = json!("DEMO-MD-P-DUPLICATE");
        rows.as_array_mut().unwrap().push(duplicate);
        assert!(
            matches!(load(&rows.to_string()), Err(crate::Error::ValidationError(message)) if message.contains("SKU 编号重复"))
        );
    }

    #[test]
    fn generation_chunk_boundaries_and_counts() {
        let steps = load(include_str!("master-data.json")).unwrap();
        assert_eq!(apply_window(0, steps.len()), 0..8);
        assert_eq!(apply_window(steps.len(), steps.len()), steps.len()..steps.len());
        let (active, removed) = count_records(&[(steps[0].kind, false), (steps[0].kind, true)]);
        assert_eq!((active.unit, removed.unit), (1, 1));
    }
}
