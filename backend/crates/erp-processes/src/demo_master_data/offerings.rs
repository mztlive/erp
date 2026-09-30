//! 演示商品可售准备；供给输入来自 JSON，责任与资格由正式领域入口校验。

use std::collections::{HashMap, HashSet};

use application_core::AuditActor;
use erp_catalog::ports::supply::CatalogSupplyQueryPort;
use erp_catalog::repository::catalog::SkuRepositoryExt;
use erp_catalog::{CatalogExt, ListingStatus, UpdateProductListingRequest};
use erp_core::common::time::BusinessDate;
use erp_core::ids::{ProductId, SupplierOfferingId};
use erp_supply::dto::supplier_offering::CreateSupplierOfferingRequest;
use persistence_core::NoTransaction;
use validator::Validate;

use super::plan::{DemoKind, DemoStep};
use super::record::DemoMasterRecord;
use super::seed::SeedRequest;
use super::{DemoMasterDataService, spec};
use crate::adapters::catalog_supply_query::MongoCatalogSupplyQuery;
use crate::adapters::scoped_offering_process;
use crate::{Error, Result};

/// 在任何主数据写入前校验供给字段、种子引用和每个 SKU 的覆盖。
///
/// # 参数
/// `steps` 为已校验的主数据清单。
/// # 返回
/// 返回正式供给创建请求模板。
/// # 错误
/// 非法条款、重复身份、缺失引用或未覆盖 SKU 时返回错误。
pub(super) fn load(steps: &[DemoStep]) -> Result<Vec<CreateSupplierOfferingRequest>> {
    let rows: Vec<CreateSupplierOfferingRequest> = serde_json::from_str(include_str!("offerings.json"))
        .map_err(|error| Error::ValidationError(format!("演示供给 JSON 无效：{error}")))?;
    validate(&rows, steps)?;
    Ok(rows)
}

/// 校验供给使用领域模型可接受的条款和完整的种子引用。
fn validate(rows: &[CreateSupplierOfferingRequest], steps: &[DemoStep]) -> Result<()> {
    let skus = steps
        .iter()
        .filter_map(|step| match &step.request {
            SeedRequest::Product(product) => Some(&product.skus),
            _ => None,
        })
        .flatten()
        .map(|sku| sku.sku_no.as_str())
        .collect::<HashSet<_>>();
    let suppliers = steps
        .iter()
        .filter(|step| step.kind == DemoKind::Supplier)
        .map(|step| step.key.as_str())
        .collect::<HashSet<_>>();
    let mut covered = HashSet::new();
    let mut keys = HashSet::new();
    for row in rows {
        row.validate().map_err(|error| Error::ValidationError(format!("演示供给字段无效：{error}")))?;
        row.terms.try_into_revision_data(SupplierOfferingId::new("validation"), 1)?;
        if !skus.contains(row.sku_id.as_str())
            || !suppliers.contains(row.supplier_id.as_str())
            || !covered.insert(row.sku_id.as_str())
            || !keys.insert(row.idempotency_key.as_str())
        {
            return Err(Error::ValidationError(format!("演示供给引用缺失或重复：{}", row.sku_id)));
        }
    }
    if covered != skus {
        return Err(Error::ValidationError("每个演示 SKU 必须配置一条初始供给".into()));
    }
    Ok(())
}

impl DemoMasterDataService {
    /// 通过采购身份登记供给并上架已登记商品；失败停止当前批次，可用原游标重试。
    ///
    /// # 参数
    /// `step` 为当前商品，`rows` 为已校验模板，`records` 为最新实际 ID 登记。
    /// # 返回
    /// 当前商品完成供给登记与上架。
    /// # 错误
    /// 岗位、登记、SKU、资格或正式写入失败时返回错误。
    pub(super) async fn ensure_sellable(
        &self,
        step: &DemoStep,
        rows: &[CreateSupplierOfferingRequest],
        records: &HashMap<String, DemoMasterRecord>,
    ) -> Result<()> {
        let product_id = registered_id(records, &step.key, DemoKind::Product)?;
        let login = &spec::foundation_spec().product_maintainer_account;
        let actor = self
            .role_actor(login)
            .await?
            .ok_or_else(|| Error::NotFound(format!("采购账号 {login} 不可用")))?;
        let skus = self
            .db
            .skus()
            .find_by_product_ids(&[ProductId::new(product_id.clone())], &mut NoTransaction)
            .await?;
        let SeedRequest::Product(product) = &step.request else {
            return Err(Error::ValidationError("可售准备只接受商品".into()));
        };
        let mut expected = Vec::new();
        for template in &product.skus {
            let sku = skus
                .iter()
                .find(|sku| sku.sku_no == template.sku_no)
                .ok_or_else(|| Error::NotFound(format!("演示 SKU {} 不存在", template.sku_no)))?;
            let row = rows
                .iter()
                .find(|row| row.sku_id == template.sku_no)
                .ok_or_else(|| Error::ValidationError(format!("演示 SKU {} 缺少供给", template.sku_no)))?;
            self.ensure_offering(row, &sku.base.id, records, &actor).await?;
            expected.push(sku.base.id.clone());
        }
        self.catalog()
            .product_listing_update(
                &product_id,
                UpdateProductListingRequest { listing_status: ListingStatus::Listed },
                &actor,
            )
            .await?;
        self.verify_sellable(&expected).await
    }

    /// 使用销售开单共用的资格查询核对真实 SKU，拒绝已停供或失效的历史回放。
    async fn verify_sellable(&self, ids: &[String]) -> Result<()> {
        let rows = MongoCatalogSupplyQuery::new(self.db.clone())
            .find_sellable_skus_by_ids(ids, BusinessDate::today(), &mut NoTransaction)
            .await?;
        let found = rows.iter().map(|row| row.sku_id.as_str()).collect::<HashSet<_>>();
        if ids.iter().any(|id| !found.contains(id.as_str())) {
            return Err(Error::ValidationError("演示商品未通过当前销售资格，请检查商品启停、价格、供给有效期和可供状态后重试；已有人工供给配置未覆盖".into()));
        }
        Ok(())
    }

    /// 实际 ID 参与命令键，删除重建后不回放旧供给；已有命令保留人工修订。
    async fn ensure_offering(
        &self,
        template: &CreateSupplierOfferingRequest,
        sku_id: &str,
        records: &HashMap<String, DemoMasterRecord>,
        actor: &AuditActor,
    ) -> Result<()> {
        let supplier_id = registered_id(records, &template.supplier_id, DemoKind::Supplier)?;
        let request = resolve(template, sku_id, &supplier_id);
        scoped_offering_process(self.db.clone(), self.rbac.clone()).create(request, actor).await?;
        Ok(())
    }
}

/// 将种子身份解析为真实主键；不认领未登记或已删除记录。
fn registered_id(records: &HashMap<String, DemoMasterRecord>, key: &str, kind: DemoKind) -> Result<String> {
    records
        .get(key)
        .filter(|row| !row.removed && row.kind() == Some(kind))
        .map(|row| row.entity_id.clone())
        .ok_or_else(|| Error::ValidationError(format!("请先完成演示主数据：{key}")))
}

/// 保持声明条款不变，只解析环境引用和命令身份。
fn resolve(
    template: &CreateSupplierOfferingRequest,
    sku: &str,
    supplier: &str,
) -> CreateSupplierOfferingRequest {
    let mut request = template.clone();
    request.sku_id = sku.into();
    request.supplier_id = supplier.into();
    request.idempotency_key = format!("{}-{sku}-{supplier}", template.idempotency_key);
    request
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo_master_data::plan::demo_steps;

    #[test]
    fn every_demo_sku_has_valid_supply_and_real_ids_change_replay_identity() {
        let steps = demo_steps().unwrap();
        let rows = load(&steps).unwrap();
        assert_eq!(rows.len(), 27);
        let first = resolve(&rows[0], "sku-one", "supplier-one");
        let retry = resolve(&rows[0], "sku-one", "supplier-one");
        let rebuilt = resolve(&rows[0], "sku-two", "supplier-two");
        assert_eq!(first.command_fingerprint().unwrap(), retry.command_fingerprint().unwrap());
        assert_ne!(first.idempotency_key, rebuilt.idempotency_key);
        assert_eq!(first.sku_id, "sku-one");
        assert_eq!(first.supplier_id, "supplier-one");
    }

    #[test]
    fn incomplete_duplicate_and_invalid_supply_fail_before_writing() {
        let steps = demo_steps().unwrap();
        let rows = load(&steps).unwrap();
        assert!(validate(&rows[1..], &steps).is_err());
        let mut invalid = rows.clone();
        invalid[0].supplier_id = "missing".into();
        assert!(validate(&invalid, &steps).is_err());
        invalid = rows.clone();
        invalid[0].terms.input_tax_rate = "NaN".into();
        assert!(validate(&invalid, &steps).is_err());
        invalid = rows.clone();
        invalid.push(rows[0].clone());
        assert!(validate(&invalid, &steps).is_err());
        assert!(registered_id(&HashMap::new(), "missing", DemoKind::Supplier).is_err());
    }
}
