//! 域 D24 供应商供给服务。
//!
//! 公司 SKU 是唯一商品主数据。服务只编排“公司 SKU → 供应商供给”：新增供给时
//! 原子写入稳定身份、首版商业条款、实时可供投影、审计与幂等结果；改价只追加
//! 商业条款修订；库存与可供状态只更新独立投影。

use std::collections::HashMap;
use std::str::FromStr;

use database::{
    AccessControlExt, CatalogExt, NoTransaction, PartyExt, SupplierApiExt, SupplierExt, SupplierOfferingExt,
    Transactional,
};
use entities::catalog::{Product, ProductKind, Sku, SkuRevision};
use entities::common::time::{BusinessDate, Instant};
use entities::ids::{
    SkuId, SupplierAccountId, SupplierApiConnectionId, SupplierOfferingAvailabilityId, SupplierOfferingId,
    SupplierOfferingRevisionId,
};
use entities::money::{round_to_cent, Amount, Quantity, Rate, UnitPrice};
use entities::party::{Party, PartyRevision};
use entities::supplier::{CapabilityCode, SupplierAccount};
use entities::supplier_offering::{
    AvailabilityStatus, OfferingStatus, PrefillSourceRefs, SupplierOffering, SupplierOfferingAvailability,
    SupplierOfferingAvailabilityData, SupplierOfferingCommand, SupplierOfferingCommandData,
    SupplierOfferingData, SupplierOfferingRevision, SupplierOfferingRevisionData,
};
use id_generator::next_id;
use mongodb::bson::doc;
use mongodb::Database;
use serde::{de::DeserializeOwned, Serialize};
use sha2::{Digest, Sha256};
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::query::{normalized_text, page_or_default, page_size_or_default};

mod dto;

pub use self::dto::{
    CreateSupplierOfferingRequest, CreateSupplierOfferingResult, PageView, ReviseSupplierOfferingRequest,
    ReviseSupplierOfferingResult, SupplierOfferingListParams, SupplierOfferingView,
    UpdateSupplierOfferingAvailabilityRequest, UpdateSupplierOfferingAvailabilityResult,
};
use self::dto::{SortDir, SupplierOfferingTermsWrite, OFFERING_SORT_FIELDS};

type SupplierOfferingFilter = <Database as SupplierOfferingExt>::SupplierOfferingFilter;

/// 供应商供给服务。
pub struct SupplierOfferingService {
    db: Database,
}

#[derive(Default)]
struct OfferingListContext {
    skus: HashMap<String, Sku>,
    sku_revisions: HashMap<String, SkuRevision>,
    products: HashMap<String, Product>,
    suppliers: HashMap<String, SupplierAccount>,
    parties: HashMap<String, Party>,
    party_revisions: HashMap<String, PartyRevision>,
}

impl SupplierOfferingService {
    /// 创建供应商供给服务。
    ///
    /// # 参数
    /// * `db` - 数据库
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 分页查询供应商供给。
    ///
    /// # 参数
    /// * `params` - 筛选与分页参数
    ///
    /// # 返回
    /// 返回包含公司 SKU、供应商、当前商业条款和实时可供状态的列表。
    ///
    /// # 错误
    /// 参数或数据库查询失败时返回错误。
    pub async fn list(&self, params: &SupplierOfferingListParams) -> Result<PageView<SupplierOfferingView>> {
        params.validate()?;
        let (sort_by, sort_dir) =
            dto::normalize_sort(&params.sort_by, &params.sort_dir, OFFERING_SORT_FIELDS)?;
        let offering_ids = self
            .offering_ids_by_availability(params.availability_status)
            .await?;
        let filter = SupplierOfferingFilter {
            offering_ids,
            sku_id: typed_id(params.sku_id.as_deref(), SkuId::new),
            supplier_id: typed_id(params.supplier_id.as_deref(), SupplierAccountId::new),
            status: params.status,
            source_type: params.source_type,
            supplier_sku_code: normalized_text(params.q.as_deref()),
            page: page_or_default(params.page),
            page_size: page_size_or_default(params.page_size),
            sort_by: Some(sort_by.to_string()),
            sort_ascending: sort_dir == SortDir::Asc,
        };
        let page = self
            .db
            .supplier_offerings()
            .search_supplier_offerings(&filter, &mut NoTransaction)
            .await?;
        let offering_ids = page
            .items
            .iter()
            .map(|row| SupplierOfferingId::new(row.id.clone()))
            .collect::<Vec<_>>();
        let revisions = self.current_revisions(&offering_ids).await?;
        let availabilities = self.current_availabilities(&offering_ids).await?;
        let context = self.list_context(&page.items).await?;
        let items = page
            .items
            .into_iter()
            .map(|row| {
                let id = row.id.clone();
                build_view(
                    row,
                    revisions.get(&id).cloned(),
                    availabilities.get(&id).cloned(),
                    &context,
                )
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 将当前可供状态转换为供给主键候选集，确保关联条件在分页前生效。
    async fn offering_ids_by_availability(
        &self,
        status: Option<AvailabilityStatus>,
    ) -> Result<Option<Vec<SupplierOfferingId>>> {
        let Some(status) = status else {
            return Ok(None);
        };
        let offering_ids = self
            .db
            .supplier_offering_availabilities()
            .find_offering_ids_by_status(status, &mut NoTransaction)
            .await?;
        Ok(Some(offering_ids))
    }

    /// 新增公司 SKU 的供应商供给。
    ///
    /// # 参数
    /// * `req` - 供给身份、首版条款和初始可供状态
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回供给、修订和可供投影主键。
    ///
    /// # 错误
    /// 公司 SKU/供应商/连接无效、资质不满足、字段非法或身份重复时返回错误。
    pub async fn create(
        &self,
        req: CreateSupplierOfferingRequest,
        actor: &AuditActor,
    ) -> Result<CreateSupplierOfferingResult> {
        req.validate()?;
        let fingerprint = command_fingerprint("create_offering", &req)?;
        if let Some(command) = self.command_record(&req.idempotency_key).await? {
            return replay_command(command, "create_offering", &fingerprint);
        }
        let offering_id = SupplierOfferingId::new(next_id());
        let mut offering = SupplierOffering::new(
            offering_id.clone(),
            SupplierOfferingData {
                sku_id: SkuId::new(req.sku_id.trim()),
                supplier_id: SupplierAccountId::new(req.supplier_id.trim()),
                supplier_product_code: req.supplier_product_code.clone(),
                supplier_sku_code: req.supplier_sku_code.clone(),
                source_type: req.source_type,
                source_connection_id: req
                    .source_connection_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(SupplierApiConnectionId::new),
            },
            actor.id(),
        )?;
        self.ensure_identity_available(&offering).await?;
        self.ensure_source_connection(&offering).await?;
        let revision = build_revision(&offering_id, 1, &req.terms)?;
        self.ensure_qualified(&offering.supplier_id, &offering.sku_id, revision.valid_from)
            .await?;
        let availability = build_availability(
            &offering_id,
            req.availability_status,
            req.available_quantity.as_deref(),
            req.source_updated_at,
            req.source_revision_token,
            actor.id(),
        )?;
        offering.stable.current_revision_id = Some(revision.base.id.clone());
        let result = CreateSupplierOfferingResult {
            offering_id: offering.base.id.clone(),
            revision_id: revision.base.id.clone(),
            availability_id: availability.base.id.clone(),
            revision_no: 1,
            status: offering.stable.status,
        };
        let command = build_command(&req.idempotency_key, "create_offering", &fingerprint, &result)?;
        let audit = actor.clone().resource_log_with_message(
            "supplier_offering.create",
            "supplier_offering",
            offering.base.id.clone(),
            Some(req.change_reason),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.supplier_offering_repository()
                        .create_with_revision_and_availability(&offering, &revision, &availability, session)
                        .await?;
                    db.supplier_offering_commands().create(&command, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), Error>(())
                })
            })
            .await;
        self.resolve_command_result(
            transaction_result,
            result,
            &req.idempotency_key,
            "create_offering",
            &fingerprint,
        )
        .await
    }

    /// 追加新的供给商业条款修订。
    ///
    /// # 参数
    /// * `id` - 供给主键
    /// * `req` - 新条款与期望版本
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回新修订号和供给状态。
    ///
    /// # 错误
    /// 供给不存在、版本冲突、资质不满足或条款非法时返回错误。
    pub async fn revise(
        &self,
        id: &str,
        req: ReviseSupplierOfferingRequest,
        actor: &AuditActor,
    ) -> Result<ReviseSupplierOfferingResult> {
        req.validate()?;
        let fingerprint = command_fingerprint("revise_offering", &(id, &req))?;
        if let Some(command) = self.command_record(&req.idempotency_key).await? {
            return replay_command(command, "revise_offering", &fingerprint);
        }
        let mut offering = self
            .db
            .supplier_offerings()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供给不存在".to_string()))?;
        let current_no = self.current_revision_no(&offering).await?;
        if current_no != req.expected_revision_no {
            return Err(Error::ConflictError(
                "供给版本已经变化，请刷新后重新保存".to_string(),
            ));
        }
        let next_no = current_no + 1;
        let revision = build_revision(
            &SupplierOfferingId::new(offering.base.id.clone()),
            next_no,
            &req.terms,
        )?;
        let next_status = req.status.unwrap_or(offering.stable.status);
        if next_status == OfferingStatus::Active {
            self.ensure_qualified(&offering.supplier_id, &offering.sku_id, revision.valid_from)
                .await?;
        }
        offering.update_status(next_status, actor.id())?;
        offering.stable.current_revision_id = Some(revision.base.id.clone());
        let result = ReviseSupplierOfferingResult {
            offering_id: offering.base.id.clone(),
            revision_id: revision.base.id.clone(),
            revision_no: next_no,
            status: next_status,
            version: offering.base.version + 1,
        };
        let command = build_command(&req.idempotency_key, "revise_offering", &fingerprint, &result)?;
        let audit = actor.clone().resource_log_with_message(
            "supplier_offering.revise",
            "supplier_offering",
            offering.base.id.clone(),
            Some(req.change_reason),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.supplier_offering_repository()
                        .append_revision(&mut offering, &revision, session)
                        .await?;
                    db.supplier_offering_commands().create(&command, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), Error>(())
                })
            })
            .await;
        self.resolve_command_result(
            transaction_result,
            result,
            &req.idempotency_key,
            "revise_offering",
            &fingerprint,
        )
        .await
    }

    /// 更新供给的实时可供状态与数量。
    ///
    /// # 参数
    /// * `id` - 供给主键
    /// * `req` - 新可供事实
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回更新后的状态、版本和来源时间。
    ///
    /// # 错误
    /// 供给/投影不存在、版本冲突、来源时间倒退或数量非法时返回错误。
    pub async fn update_availability(
        &self,
        id: &str,
        req: UpdateSupplierOfferingAvailabilityRequest,
        actor: &AuditActor,
    ) -> Result<UpdateSupplierOfferingAvailabilityResult> {
        req.validate()?;
        let fingerprint = command_fingerprint("update_offering_availability", &(id, &req))?;
        if let Some(command) = self.command_record(&req.idempotency_key).await? {
            return replay_command(command, "update_offering_availability", &fingerprint);
        }
        let offering_id = SupplierOfferingId::new(id.trim());
        self.db
            .supplier_offerings()
            .find_by_id(&offering_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供给不存在".to_string()))?;
        let mut availability = self
            .db
            .supplier_offering_availabilities()
            .find_by_offering_id(&offering_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供给可供状态不存在".to_string()))?;
        if req
            .expected_version
            .is_some_and(|version| version != availability.base.version)
        {
            return Err(Error::ConflictError(
                "可供状态已经变化，请刷新后重新保存".to_string(),
            ));
        }
        let source_updated_at = req
            .source_updated_at
            .map(Instant::from_unix_secs)
            .unwrap_or_else(Instant::now);
        availability.apply(SupplierOfferingAvailabilityData {
            supplier_offering_id: offering_id.clone(),
            availability_status: req.availability_status,
            available_quantity: parse_quantity(req.available_quantity.as_deref())?,
            source_updated_at,
            received_at: Instant::now(),
            source_revision_token: req.source_revision_token,
            updated_by: actor.id().to_string(),
        })?;
        let result = UpdateSupplierOfferingAvailabilityResult {
            offering_id: offering_id.to_string(),
            availability_status: availability.availability_status,
            availability_version: availability.base.version + 1,
            source_updated_at: availability.source_updated_at.unix_secs(),
        };
        let command = build_command(
            &req.idempotency_key,
            "update_offering_availability",
            &fingerprint,
            &result,
        )?;
        let audit = actor.clone().resource_log_with_message(
            "supplier_offering.availability.update",
            "supplier_offering",
            id.to_string(),
            Some(req.change_reason),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.supplier_offering_availabilities()
                        .update(&mut availability, session)
                        .await?;
                    db.supplier_offering_commands().create(&command, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), Error>(())
                })
            })
            .await;
        self.resolve_command_result(
            transaction_result,
            result,
            &req.idempotency_key,
            "update_offering_availability",
            &fingerprint,
        )
        .await
    }

    async fn ensure_identity_available(&self, offering: &SupplierOffering) -> Result<()> {
        let existing = self
            .db
            .supplier_offerings()
            .find_by_supplier_identity(
                &offering.supplier_id,
                &offering.supplier_sku_code,
                &mut NoTransaction,
            )
            .await?;
        if existing.is_some() {
            return Err(Error::ConflictError("该供应商 SKU 已登记供给".to_string()));
        }
        Ok(())
    }

    async fn ensure_source_connection(&self, offering: &SupplierOffering) -> Result<()> {
        let Some(connection_id) = offering.source_connection_id.as_ref() else {
            return Ok(());
        };
        let connection = self
            .db
            .supplier_api_connections()
            .find_by_id(connection_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商 API 连接不存在".to_string()))?;
        if connection.supplier_id != offering.supplier_id || !connection.is_active() {
            return Err(Error::BusinessLogicError(
                "供应商 API 连接不属于该供应商或未启用".to_string(),
            ));
        }
        Ok(())
    }

    async fn ensure_qualified(
        &self,
        supplier_id: &SupplierAccountId,
        sku_id: &SkuId,
        on_date: BusinessDate,
    ) -> Result<()> {
        let sku = self
            .db
            .skus()
            .find_by_id(sku_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("公司 SKU 不存在".to_string()))?;
        if !sku.is_active() {
            return Err(Error::BusinessLogicError("公司 SKU 未启用".to_string()));
        }
        let product = self
            .db
            .products()
            .find_by_id(&sku.product_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("公司商品不存在".to_string()))?;
        let code = match product.product_kind {
            ProductKind::Physical => CapabilityCode::Physical,
            ProductKind::Virtual | ProductKind::Voucher => CapabilityCode::Virtual,
            ProductKind::OfflineService => CapabilityCode::OfflineService,
        };
        let capability = self
            .db
            .supplier_capabilities()
            .find_by_supplier_and_code(supplier_id, code, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::BusinessLogicError("供应商未启用该商品类型所需能力".to_string()))?;
        let revision_id = capability
            .stable
            .current_revision_id
            .as_deref()
            .map(entities::ids::SupplierCapabilityRevisionId::new)
            .ok_or_else(|| Error::BusinessLogicError("供应商能力缺少当前版本".to_string()))?;
        crate::supplier::eligibility::ensure_capability_qualified(
            &self.db,
            supplier_id,
            &revision_id,
            on_date,
        )
        .await
    }

    async fn current_revision_no(&self, offering: &SupplierOffering) -> Result<u32> {
        let revisions = self
            .db
            .supplier_offering_revisions()
            .find_revisions_by_offering_ids(
                &[SupplierOfferingId::new(offering.base.id.clone())],
                &mut NoTransaction,
            )
            .await?;
        Ok(revisions
            .into_iter()
            .map(|revision| revision.revision.revision_no)
            .max()
            .unwrap_or(0))
    }

    async fn current_revisions(
        &self,
        offering_ids: &[SupplierOfferingId],
    ) -> Result<HashMap<String, SupplierOfferingRevision>> {
        let offerings = self
            .db
            .supplier_offerings()
            .find_many(
                doc! { "id": { "$in": offering_ids.iter().map(ToString::to_string).collect::<Vec<_>>() } },
                &mut NoTransaction,
            )
            .await?;
        let current = offerings
            .into_iter()
            .filter_map(|offering| {
                offering
                    .stable
                    .current_revision_id
                    .map(|revision_id| (revision_id, offering.base.id))
            })
            .collect::<HashMap<_, _>>();
        let revisions = self
            .db
            .supplier_offering_revisions()
            .find_revisions_by_offering_ids(offering_ids, &mut NoTransaction)
            .await?;
        Ok(revisions
            .into_iter()
            .filter_map(|revision| {
                current
                    .get(&revision.base.id)
                    .cloned()
                    .map(|offering_id| (offering_id, revision))
            })
            .collect())
    }

    async fn current_availabilities(
        &self,
        offering_ids: &[SupplierOfferingId],
    ) -> Result<HashMap<String, SupplierOfferingAvailability>> {
        Ok(self
            .db
            .supplier_offering_availabilities()
            .find_by_offering_ids(offering_ids, &mut NoTransaction)
            .await?
            .into_iter()
            .map(|value| (value.supplier_offering_id.to_string(), value))
            .collect())
    }

    async fn list_context(&self, rows: &[database::SupplierOfferingRow]) -> Result<OfferingListContext> {
        let sku_ids = rows.iter().map(|row| row.sku_id.to_string()).collect::<Vec<_>>();
        let skus = self
            .db
            .skus()
            .find_many(doc! { "id": { "$in": &sku_ids } }, &mut NoTransaction)
            .await?;
        let sku_revision_ids = skus
            .iter()
            .filter_map(|sku| sku.stable.current_revision_id.clone())
            .collect::<Vec<_>>();
        let product_ids = skus
            .iter()
            .map(|sku| sku.product_id.to_string())
            .collect::<Vec<_>>();
        let sku_revisions = self
            .db
            .sku_revisions()
            .find_many(doc! { "id": { "$in": sku_revision_ids } }, &mut NoTransaction)
            .await?;
        let products = self
            .db
            .products()
            .find_many(doc! { "id": { "$in": product_ids } }, &mut NoTransaction)
            .await?;
        let supplier_ids = rows
            .iter()
            .map(|row| row.supplier_id.to_string())
            .collect::<Vec<_>>();
        let suppliers = self
            .db
            .supplier_accounts()
            .find_many(doc! { "id": { "$in": supplier_ids } }, &mut NoTransaction)
            .await?;
        let party_ids = suppliers
            .iter()
            .map(|supplier| supplier.party_id.to_string())
            .collect::<Vec<_>>();
        let parties = self
            .db
            .parties()
            .find_many(doc! { "id": { "$in": party_ids } }, &mut NoTransaction)
            .await?;
        let party_revision_ids = parties
            .iter()
            .filter_map(|party| party.stable.current_revision_id.clone())
            .collect::<Vec<_>>();
        let party_revisions = self
            .db
            .party_revisions()
            .find_many(doc! { "id": { "$in": party_revision_ids } }, &mut NoTransaction)
            .await?;
        Ok(OfferingListContext {
            skus: by_id(skus),
            sku_revisions: by_id(sku_revisions),
            products: by_id(products),
            suppliers: by_id(suppliers),
            parties: by_id(parties),
            party_revisions: by_id(party_revisions),
        })
    }

    async fn command_record(&self, idempotency_key: &str) -> Result<Option<SupplierOfferingCommand>> {
        self.db
            .supplier_offering_commands()
            .find_by_idempotency_key(idempotency_key, &mut NoTransaction)
            .await
            .map_err(Into::into)
    }

    async fn resolve_command_result<T>(
        &self,
        transaction_result: Result<()>,
        intended_result: T,
        idempotency_key: &str,
        operation: &str,
        fingerprint: &str,
    ) -> Result<T>
    where
        T: DeserializeOwned,
    {
        match transaction_result {
            Ok(()) => Ok(intended_result),
            Err(error) => match self.command_record(idempotency_key).await? {
                Some(command) => replay_command(command, operation, fingerprint),
                None => Err(error),
            },
        }
    }
}

fn typed_id<T>(value: Option<&str>, constructor: impl Fn(String) -> T) -> Option<T> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| constructor(value.to_string()))
}

fn build_revision(
    offering_id: &SupplierOfferingId,
    revision_no: u32,
    terms: &SupplierOfferingTermsWrite,
) -> Result<SupplierOfferingRevision> {
    let rate = Rate::from_str(terms.input_tax_rate.trim())
        .map_err(|_| Error::ValidationError(format!("非法进项税率: {}", terms.input_tax_rate)))?;
    let dropship_gross = parse_unit_price(&terms.dropship_supply_price_gross, "一件代发供给价")?;
    let bulk_gross = parse_unit_price(&terms.bulk_supply_price_gross, "集采供给价")?;
    SupplierOfferingRevision::new(
        SupplierOfferingRevisionId::new(next_id()),
        SupplierOfferingRevisionData {
            supplier_offering_id: offering_id.clone(),
            revision_no,
            dropship_supply_price_gross: dropship_gross,
            dropship_supply_price_net: price_net(dropship_gross, rate),
            bulk_supply_price_gross: bulk_gross,
            bulk_supply_price_net: price_net(bulk_gross, rate),
            input_tax_rate: rate,
            dropship_express: terms.dropship_express.clone(),
            freight_amount: parse_amount(terms.freight_amount.as_deref())?,
            service_fee_amount: parse_amount(terms.service_fee_amount.as_deref())?,
            bulk_minimum_order_quantity: Quantity::from_str(terms.bulk_minimum_order_quantity.trim())
                .map_err(|_| {
                    Error::ValidationError(format!("非法集采起订量: {}", terms.bulk_minimum_order_quantity))
                })?,
            supply_region: terms.supply_region.clone(),
            product_capabilities: terms.product_capabilities.clone(),
            valid_from: parse_business_date(&terms.valid_from)?,
            valid_to: terms.valid_to.as_deref().map(parse_business_date).transpose()?,
            prefill_source_refs: PrefillSourceRefs::default(),
        },
    )
    .map_err(Into::into)
}

fn build_availability(
    offering_id: &SupplierOfferingId,
    status: entities::supplier_offering::AvailabilityStatus,
    quantity: Option<&str>,
    source_updated_at: Option<i64>,
    source_revision_token: Option<String>,
    updated_by: &str,
) -> Result<SupplierOfferingAvailability> {
    let source_updated_at = source_updated_at
        .map(Instant::from_unix_secs)
        .unwrap_or_else(Instant::now);
    SupplierOfferingAvailability::new(
        SupplierOfferingAvailabilityId::new(next_id()),
        SupplierOfferingAvailabilityData {
            supplier_offering_id: offering_id.clone(),
            availability_status: status,
            available_quantity: parse_quantity(quantity)?,
            source_updated_at,
            received_at: Instant::now(),
            source_revision_token,
            updated_by: updated_by.to_string(),
        },
    )
    .map_err(Into::into)
}

fn parse_unit_price(value: &str, label: &str) -> Result<UnitPrice> {
    UnitPrice::from_str(value.trim()).map_err(|_| Error::ValidationError(format!("非法{label}: {value}")))
}

fn parse_amount(value: Option<&str>) -> Result<Option<Amount>> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            Amount::from_str(value).map_err(|_| Error::ValidationError(format!("非法金额: {value}")))
        })
        .transpose()
}

fn parse_quantity(value: Option<&str>) -> Result<Option<Quantity>> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            Quantity::from_str(value).map_err(|_| Error::ValidationError(format!("非法数量: {value}")))
        })
        .transpose()
}

fn parse_business_date(value: &str) -> Result<BusinessDate> {
    BusinessDate::from_str(value.trim()).map_err(|_| Error::ValidationError(format!("非法业务日期: {value}")))
}

fn price_net(gross: UnitPrice, rate: Rate) -> UnitPrice {
    UnitPrice::try_from(gross.to_decimal() - round_to_cent(gross.to_decimal() * rate.to_decimal()))
        .expect("合法含税价与税率必须生成合法不含税价")
}

fn build_view(
    row: database::SupplierOfferingRow,
    revision: Option<SupplierOfferingRevision>,
    availability: Option<SupplierOfferingAvailability>,
    context: &OfferingListContext,
) -> SupplierOfferingView {
    let sku = context.skus.get(row.sku_id.as_ref());
    let sku_revision = sku
        .and_then(|sku| sku.stable.current_revision_id.as_deref())
        .and_then(|id| context.sku_revisions.get(id));
    let product = sku.and_then(|sku| context.products.get(sku.product_id.as_ref()));
    let supplier = context.suppliers.get(row.supplier_id.as_ref());
    let party = supplier.and_then(|supplier| context.parties.get(supplier.party_id.as_ref()));
    let party_revision = party
        .and_then(|party| party.stable.current_revision_id.as_deref())
        .and_then(|id| context.party_revisions.get(id));
    SupplierOfferingView {
        id: row.id,
        sku_id: row.sku_id.to_string(),
        sku_no: sku.map(|value| value.sku_no.clone()),
        product_no: product.map(|value| value.product_no.clone()),
        sku_name: sku_revision.map(|value| value.name.clone()),
        specification: sku_revision.and_then(|value| value.specification.clone()),
        supplier_id: row.supplier_id.to_string(),
        supplier_no: supplier.map(|value| value.supplier_no.clone()),
        supplier_name: party_revision.map(|value| value.legal_name.clone()),
        supplier_product_code: row.supplier_product_code,
        supplier_sku_code: row.supplier_sku_code,
        source_type: row.source_type,
        source_connection_id: row.source_connection_id.map(|value| value.to_string()),
        status: row.status,
        current_revision_id: row.current_revision_id,
        current_revision_no: revision.as_ref().map(|value| value.revision.revision_no),
        dropship_supply_price_gross: revision
            .as_ref()
            .map(|value| value.dropship_supply_price_gross.to_string()),
        dropship_supply_price_net: revision
            .as_ref()
            .map(|value| value.dropship_supply_price_net.to_string()),
        bulk_supply_price_gross: revision
            .as_ref()
            .map(|value| value.bulk_supply_price_gross.to_string()),
        bulk_supply_price_net: revision
            .as_ref()
            .map(|value| value.bulk_supply_price_net.to_string()),
        input_tax_rate: revision.as_ref().map(|value| value.input_tax_rate.to_string()),
        bulk_minimum_order_quantity: revision
            .as_ref()
            .map(|value| value.bulk_minimum_order_quantity.to_string()),
        supply_region: revision
            .as_ref()
            .map(|value| value.supply_region.clone())
            .unwrap_or_default(),
        product_capabilities: revision
            .as_ref()
            .map(|value| value.product_capabilities.clone())
            .unwrap_or_default(),
        dropship_express: revision.as_ref().and_then(|value| value.dropship_express.clone()),
        freight_amount: revision
            .as_ref()
            .and_then(|value| value.freight_amount.map(|amount| amount.to_string())),
        service_fee_amount: revision
            .as_ref()
            .and_then(|value| value.service_fee_amount.map(|amount| amount.to_string())),
        valid_from: revision.as_ref().map(|value| value.valid_from.to_string()),
        valid_to: revision
            .as_ref()
            .and_then(|value| value.valid_to.map(|date| date.to_string())),
        availability_status: availability.as_ref().map(|value| value.availability_status),
        available_quantity: availability
            .as_ref()
            .and_then(|value| value.available_quantity.map(|quantity| quantity.to_string())),
        availability_source_updated_at: availability
            .as_ref()
            .map(|value| value.source_updated_at.unix_secs()),
        availability_version: availability.as_ref().map(|value| value.base.version),
        version: row.version,
        created_at: row.created_at,
    }
}

trait HasId {
    fn id(&self) -> &str;
}

impl HasId for Sku {
    fn id(&self) -> &str {
        &self.base.id
    }
}
impl HasId for SkuRevision {
    fn id(&self) -> &str {
        &self.base.id
    }
}
impl HasId for Product {
    fn id(&self) -> &str {
        &self.base.id
    }
}
impl HasId for SupplierAccount {
    fn id(&self) -> &str {
        &self.base.id
    }
}
impl HasId for Party {
    fn id(&self) -> &str {
        &self.base.id
    }
}
impl HasId for PartyRevision {
    fn id(&self) -> &str {
        &self.base.id
    }
}

fn by_id<T: HasId>(values: Vec<T>) -> HashMap<String, T> {
    values
        .into_iter()
        .map(|value| (value.id().to_string(), value))
        .collect()
}

fn command_fingerprint<T: Serialize>(operation: &str, request: &T) -> Result<String> {
    let payload = serde_json::to_vec(&(operation, request))
        .map_err(|error| Error::Internal(format!("序列化命令指纹失败: {error}")))?;
    Ok(format!("{:x}", Sha256::digest(payload)))
}

fn build_command<T: Serialize>(
    idempotency_key: &str,
    operation: &str,
    fingerprint: &str,
    result: &T,
) -> Result<SupplierOfferingCommand> {
    SupplierOfferingCommand::new(
        next_id(),
        SupplierOfferingCommandData {
            idempotency_key: idempotency_key.to_string(),
            operation: operation.to_string(),
            request_fingerprint: fingerprint.to_string(),
            result_json: serde_json::to_string(result)
                .map_err(|error| Error::Internal(format!("序列化命令结果失败: {error}")))?,
        },
    )
    .map_err(Into::into)
}

fn replay_command<T: DeserializeOwned>(
    command: SupplierOfferingCommand,
    operation: &str,
    fingerprint: &str,
) -> Result<T> {
    if command.operation != operation || command.request_fingerprint != fingerprint {
        return Err(Error::ConflictError("幂等键已被其他请求使用".to_string()));
    }
    serde_json::from_str(&command.result_json)
        .map_err(|error| Error::Internal(format!("反序列化幂等结果失败: {error}")))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::{price_net, typed_id};
    use entities::ids::SkuId;
    use entities::money::{Rate, UnitPrice};

    #[test]
    fn blank_filter_ids_are_omitted() {
        assert!(typed_id(Some("  "), SkuId::new).is_none());
        assert_eq!(typed_id(Some(" sku-1 "), SkuId::new).unwrap().as_ref(), "sku-1");
    }

    #[test]
    fn net_price_uses_contract_rounding() {
        let gross = UnitPrice::from_str("11.30").unwrap();
        let rate = Rate::from_str("0.13").unwrap();
        assert_eq!(price_net(gross, rate).to_string(), "9.83");
    }
}
