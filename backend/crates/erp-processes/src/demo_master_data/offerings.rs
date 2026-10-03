//! 演示商品可售准备；供给输入来自 JSON，责任与资格由正式领域入口校验。

use std::collections::{HashMap, HashSet};

use application_core::AuditActor;
use erp_catalog::ports::supply::CatalogSupplyQueryPort;
use erp_catalog::repository::catalog::SkuRepositoryExt;
use erp_catalog::{CatalogExt, ListingStatus, ProductKind, UpdateProductListingRequest};
use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::{
    ProductId, SupplierAccountId, SupplierOfferingAvailabilityId, SupplierOfferingId,
    SupplierOfferingRevisionId, SupplierQualificationId,
};
use erp_supplier::entity::supplier::eligibility::{
    OfferingProductKind, ensure_linked_contracts_qualified, required_offering_capability,
};
use erp_supplier::{
    CapabilityCode, QualificationStatus, QualificationType, SaveSupplierProfileRequest,
    SupplierQualification, SupplierQualificationData,
};
use erp_supply::dto::supplier_offering::CreateSupplierOfferingRequest;
use erp_supply::entity::supplier_offering::{
    SupplierOffering, SupplierOfferingAvailability, SupplierOfferingRevision,
};
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
            SeedRequest::Product(product) => Some(product),
            _ => None,
        })
        .flat_map(|product| product.skus.iter().map(|sku| (sku.sku_no.as_str(), product.product_kind)))
        .collect::<HashMap<_, _>>();
    let suppliers = steps
        .iter()
        .filter_map(|step| match &step.request {
            SeedRequest::Supplier(supplier) => Some((step.key.as_str(), supplier.as_ref())),
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    let mut covered = HashSet::new();
    let mut relationships = HashSet::new();
    let mut keys = HashSet::new();
    for row in rows {
        let revision = validate_request(row)?;
        let kind = skus
            .get(row.sku_id.as_str())
            .ok_or_else(|| Error::ValidationError(format!("演示供给 SKU 引用未定义：{}", row.sku_id)))?;
        let supplier = suppliers.get(row.supplier_id.as_str()).ok_or_else(|| {
            Error::ValidationError(format!("演示供给供应商引用未定义：{}", row.supplier_id))
        })?;
        if !relationships.insert((row.sku_id.as_str(), row.supplier_id.as_str())) {
            return Err(Error::ValidationError(format!(
                "演示供给关系重复：{} / {}",
                row.sku_id, row.supplier_id
            )));
        }
        if !keys.insert(row.idempotency_key.as_str()) {
            return Err(Error::ValidationError(format!("演示供给幂等键重复：{}", row.idempotency_key)));
        }
        validate_qualification(*kind, supplier, revision.valid_from)?;
        covered.insert(row.sku_id.as_str());
    }
    if covered.len() != skus.len() {
        return Err(Error::ValidationError("每个演示 SKU 必须配置至少一条初始供给".into()));
    }
    Ok(())
}

/// 与正式创建入口共用供给、条款与可供实体的不变式，防止字段解析成功后才写入失败。
fn validate_request(row: &CreateSupplierOfferingRequest) -> Result<SupplierOfferingRevision> {
    row.validate().map_err(|error| Error::ValidationError(format!("演示供给字段无效：{error}")))?;
    let offering_id = SupplierOfferingId::new("validation");
    SupplierOffering::new(
        offering_id.clone(),
        row.try_into_offering_data("$supplier_maintainer".into(), "validation".into())?,
        "validation",
    )?;
    let revision = SupplierOfferingRevision::new(
        SupplierOfferingRevisionId::new("validation"),
        row.terms.try_into_revision_data(offering_id.clone(), 1)?,
    )?;
    let at = Instant::from_unix_secs(0);
    SupplierOfferingAvailability::new(
        SupplierOfferingAvailabilityId::new("validation"),
        row.try_into_availability_data(offering_id, at, at, "validation".into())?,
    )?;
    Ok(revision)
}

/// 只检查种子跨文件关联；所需能力和合同有效政策由供应商领域执行。
fn validate_qualification(
    kind: ProductKind,
    supplier: &SaveSupplierProfileRequest,
    valid_from: BusinessDate,
) -> Result<()> {
    let kind = match kind {
        ProductKind::Physical => OfferingProductKind::Physical,
        ProductKind::Virtual => OfferingProductKind::Virtual,
        ProductKind::OfflineService => OfferingProductKind::OfflineService,
        ProductKind::Voucher => OfferingProductKind::Voucher,
    };
    let capability = required_offering_capability(kind);
    if !supplier.capability_codes.contains(&capability) {
        return Err(Error::ValidationError(format!(
            "演示供应商 {} 缺少商品所需供给能力：{}",
            supplier.legal_name,
            capability.as_str()
        )));
    }
    let contracts = linked_contracts(supplier, capability)?;
    ensure_linked_contracts_qualified(&contracts, valid_from)?;
    ensure_linked_contracts_qualified(&contracts, BusinessDate::today())?;
    Ok(())
}

/// 按已声明能力关联还原合同实体，不推断或补造未登记合同。
fn linked_contracts(
    supplier: &SaveSupplierProfileRequest,
    capability: CapabilityCode,
) -> Result<Vec<SupplierQualification>> {
    supplier
        .qualifications
        .iter()
        .filter(|row| {
            row.qualification_type == QualificationType::Contract
                && row.capability_codes.contains(&capability)
        })
        .map(|row| {
            SupplierQualification::new(
                SupplierQualificationId::new("validation"),
                SupplierQualificationData {
                    supplier_id: SupplierAccountId::new(&supplier.idempotency_key),
                    qualification_type: row.qualification_type,
                    certificate_no: row.certificate_no.clone(),
                    issuer: row.issuer.clone(),
                    valid_from: row.valid_from,
                    valid_to: row.valid_to,
                    attachment_id: row.attachment_id.clone(),
                    status: QualificationStatus::Active,
                },
                "validation",
            )
            .map_err(Into::into)
        })
        .collect()
}

/// 返回当前 SKU 的全部声明供给，缺失时拒绝继续上架。
fn sku_offerings<'a>(
    rows: &'a [CreateSupplierOfferingRequest],
    sku_no: &str,
) -> Result<Vec<&'a CreateSupplierOfferingRequest>> {
    let matches = rows.iter().filter(|row| row.sku_id == sku_no).collect::<Vec<_>>();
    if matches.is_empty() {
        return Err(Error::ValidationError(format!("演示 SKU {sku_no} 缺少供给")));
    }
    Ok(matches)
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
            for row in sku_offerings(rows, &template.sku_no)? {
                self.ensure_offering(row, &sku.base.id, records, &actor).await?;
            }
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
        let sku_count = steps
            .iter()
            .filter_map(|step| match &step.request {
                SeedRequest::Product(product) => Some(product.skus.len()),
                _ => None,
            })
            .sum::<usize>();
        assert!(rows.len() >= sku_count);
        let first = resolve(&rows[0], "sku-one", "supplier-one");
        let retry = resolve(&rows[0], "sku-one", "supplier-one");
        let rebuilt = resolve(&rows[0], "sku-two", "supplier-two");
        assert_eq!(first.command_fingerprint().unwrap(), retry.command_fingerprint().unwrap());
        assert_ne!(first.idempotency_key, rebuilt.idempotency_key);
        assert_eq!(first.sku_id, "sku-one");
        assert_eq!(first.supplier_id, "supplier-one");
    }

    /// 专用样本的所有付款条件必须形成可选择供给，成本与公司四价保持独立。
    #[test]
    fn dedicated_demo_offers_every_declared_payment_term_and_separate_costs() {
        let steps = demo_steps().unwrap();
        let rows = load(&steps).unwrap();
        let selected = sku_offerings(&rows, "DEMO-MD-SKU-28-A").unwrap();
        let all_terms = steps
            .iter()
            .filter_map(|step| match &step.request {
                SeedRequest::Supplier(supplier) => Some(supplier.payment_term_snapshot.as_str()),
                _ => None,
            })
            .collect::<HashSet<_>>();
        let offered_terms = selected
            .iter()
            .map(|row| {
                let step = steps.iter().find(|step| step.key == row.supplier_id).unwrap();
                let SeedRequest::Supplier(supplier) = &step.request else { panic!("供应商种子") };
                assert_eq!(row.terms.bulk_minimum_order_quantity, "6");
                assert_ne!(row.terms.dropship_supply_price_gross, "129.00");
                supplier.payment_term_snapshot.as_str()
            })
            .collect::<HashSet<_>>();
        assert_eq!(selected.len(), 11);
        assert_eq!(offered_terms, all_terms);
        assert_eq!(sku_offerings(&rows, "DEMO-MD-SKU-28-B").unwrap().len(), 1);
    }

    #[test]
    fn incomplete_duplicate_and_invalid_supply_fail_before_writing() {
        let steps = demo_steps().unwrap();
        let rows = load(&steps).unwrap();
        let incomplete = rows.iter().filter(|row| row.sku_id != rows[0].sku_id).cloned().collect::<Vec<_>>();
        assert!(
            matches!(validate(&incomplete, &steps), Err(Error::ValidationError(message)) if message.contains("至少一条"))
        );
        let mut invalid = rows.clone();
        invalid[0].supplier_id = "missing".into();
        assert!(
            matches!(validate(&invalid, &steps), Err(Error::ValidationError(message)) if message.contains("供应商引用未定义"))
        );
        invalid = rows.clone();
        invalid[0].sku_id = "missing".into();
        assert!(
            matches!(validate(&invalid, &steps), Err(Error::ValidationError(message)) if message.contains("SKU 引用未定义"))
        );
        invalid = rows.clone();
        invalid[0].terms.input_tax_rate = "NaN".into();
        assert!(validate(&invalid, &steps).is_err());
        invalid = rows.clone();
        invalid.push(rows[0].clone());
        assert!(validate(&invalid, &steps).is_err());
        assert!(registered_id(&HashMap::new(), "missing", DemoKind::Supplier).is_err());
    }

    #[test]
    fn multiple_suppliers_are_valid_and_every_matching_offering_is_selected() {
        let steps = demo_steps().unwrap();
        let mut rows = load(&steps).unwrap();
        let mut alternate = rows[0].clone();
        alternate.supplier_id = "demo-master-supplier-02".into();
        alternate.idempotency_key = "demo-offering-alternate".into();
        rows.push(alternate);
        validate(&rows, &steps).unwrap();
        let selected = sku_offerings(&rows, &rows[0].sku_id).unwrap();
        assert_eq!(selected.len(), 2);
        assert_eq!(selected[0].supplier_id, "demo-master-supplier-01");
        assert_eq!(selected[1].supplier_id, "demo-master-supplier-02");
        assert!(selected.iter().all(|row| row.sku_id == rows[0].sku_id));
        assert!(
            matches!(sku_offerings(&rows, "missing"), Err(Error::ValidationError(message)) if message.contains("缺少供给"))
        );
    }

    #[test]
    fn duplicate_relationship_and_reused_command_key_are_distinct_failures() {
        let steps = demo_steps().unwrap();
        let mut rows = load(&steps).unwrap();
        let mut duplicate = rows[0].clone();
        duplicate.idempotency_key = "new-key-same-relationship".into();
        rows.push(duplicate);
        assert!(
            matches!(validate(&rows, &steps), Err(Error::ValidationError(message)) if message.contains("关系重复"))
        );
        let reused_key = rows[0].idempotency_key.clone();
        let last = rows.last_mut().unwrap();
        last.supplier_id = "demo-master-supplier-02".into();
        last.idempotency_key = reused_key;
        assert!(
            matches!(validate(&rows, &steps), Err(Error::ValidationError(message)) if message.contains("幂等键重复"))
        );
    }

    #[test]
    fn entity_invariants_reject_bad_availability_source_and_terms_before_writing() {
        let steps = demo_steps().unwrap();
        let rows = load(&steps).unwrap();
        let mut invalid = rows.clone();
        invalid[0].available_quantity = Some("-1".into());
        assert!(validate(&invalid, &steps).is_err());
        invalid = rows.clone();
        invalid[0].source_connection_id = Some("unexpected-api-connection".into());
        assert!(validate(&invalid, &steps).is_err());
        invalid = rows;
        invalid[0].terms.supply_region = vec![" ".into()];
        assert!(validate(&invalid, &steps).is_err());
    }

    #[test]
    fn supplier_capability_and_linked_contracts_are_checked_before_writing() {
        let mut steps = demo_steps().unwrap();
        let rows = load(&steps).unwrap();
        let mut mismatch = rows.clone();
        mismatch[0].supplier_id = "demo-master-supplier-17".into();
        assert!(
            matches!(validate(&mismatch, &steps), Err(Error::ValidationError(message)) if message.contains("缺少商品所需供给能力"))
        );
        let supplier = steps.iter_mut().find(|step| step.key == rows[0].supplier_id).unwrap();
        let SeedRequest::Supplier(supplier) = &mut supplier.request else {
            panic!("供给必须引用供应商")
        };
        supplier.qualifications[0].valid_to = None;
        let error = validate(&rows, &steps).unwrap_err().to_string();
        assert!(error.contains("合同有效期未核实"));
        let supplier = steps.iter_mut().find(|step| step.key == rows[0].supplier_id).unwrap();
        let SeedRequest::Supplier(supplier) = &mut supplier.request else {
            panic!("供给必须引用供应商")
        };
        supplier.qualifications.clear();
        validate(&rows, &steps).unwrap();
    }
}
