//! 域 D24 供应商供给服务。
//!
//! 公司 SKU 是唯一商品主数据。服务只编排“公司 SKU → 供应商供给”：新增供给时
//! 原子写入稳定身份、首版商业条款、实时可供投影、审计与幂等结果；改价只追加
//! 商业条款修订；库存与可供状态只更新独立投影。

use std::collections::HashMap;
use std::str::FromStr;

use database::{
    AccessControlExt, CatalogExt, NoTransaction, PublicationExt, SupplierApiExt, SupplierExt,
    SupplierOfferingExt, Transactional, WorkItemExt,
};
use entities::catalog::{Product, ProductKind, Sku, SkuRevision};
use entities::common::time::{BusinessDate, Instant};
use entities::ids::{
    SkuId, SupplierAccountId, SupplierApiConnectionId, SupplierOfferingAvailabilityId, SupplierOfferingId,
    SupplierOfferingRevisionId,
};
use entities::money::{Amount, Quantity, Rate, UnitPrice};
use entities::party::{Party, PartyRevision};
use entities::supplier::{CapabilityCode, SupplierAccount};
use entities::supplier_offering::{
    AvailabilityInterruptionReason, AvailabilityStatus, OfferingRevisionImpact, OfferingStatus,
    PrefillSourceRefs, SupplierOffering, SupplierOfferingAvailability, SupplierOfferingAvailabilityData,
    SupplierOfferingCommand, SupplierOfferingCommandData, SupplierOfferingData, SupplierOfferingRevision,
    SupplierOfferingRevisionData,
};
use id_generator::next_id;
use mongodb::Database;
use serde::{de::DeserializeOwned, Serialize};
use sha2::{Digest, Sha256};
use validator::Validate;

use crate::audit::{AuditActor, CommandReceipt};
use crate::errors::{Error, Result};
use crate::publication::{PublicationService, SystemSafetyPauseTrigger, UnavailableMallConnector};
use crate::query::{normalized_text, page_or_default, page_size_or_default};
use crate::work_item::WorkItemService;
use entities::publication::{
    SafetyPauseCause, SafetyPauseFollowUp, SafetyPauseSourceObjectType, SystemSafetyPauseOperation,
};
use entities::work_item::{WorkItem, WorkItemStatus, WorkItemType};
use std::sync::Arc;

mod dto;

pub use self::dto::{
    CompleteSupplierSupplyExceptionTaskRequest, CompleteSupplierSupplyExceptionTaskResult,
    CreateSupplierOfferingRequest, CreateSupplierOfferingResult, PageView, ReviseSupplierOfferingRequest,
    ReviseSupplierOfferingResult, SupplierOfferingListParams, SupplierOfferingView,
    UpdateSupplierOfferingAvailabilityRequest, UpdateSupplierOfferingAvailabilityResult,
};
use self::dto::{SortDir, SupplierOfferingTermsWrite, OFFERING_SORT_FIELDS};

type SupplierOfferingFilter = <Database as SupplierOfferingExt>::SupplierOfferingFilter;

const SUPPLY_EXCEPTION_COMPLETE_ACTION: &str = "supplier_offering.supply_exception.complete";

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
        let keyword = normalized_text(params.q.as_deref());
        let keyword_sku_ids = match keyword.as_deref() {
            Some(q) => Some(self.resolve_keyword_sku_ids(q).await?),
            None => None,
        };
        let product_no = normalized_text(params.product_no.as_deref());
        let sku_no = normalized_text(params.sku_no.as_deref());
        let sku_ids = self
            .resolve_sku_ids_by_codes(product_no.as_deref(), sku_no.as_deref())
            .await?;
        let filter = SupplierOfferingFilter {
            offering_ids,
            sku_id: typed_id(params.sku_id.as_deref(), SkuId::new),
            supplier_id: typed_id(params.supplier_id.as_deref(), SupplierAccountId::new),
            status: params.status,
            source_type: params.source_type,
            supplier_sku_code: keyword.clone(),
            keyword_sku_ids,
            sku_ids,
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
        let revisions = self.current_revisions(&page.items).await?;
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

    /// 按关键字解析公司 SKU 主键（SKU 编号或当前修订名称）。
    ///
    /// # 参数
    /// * `keyword` - 已去空白的关键字
    ///
    /// # 返回
    /// 返回可能命中的公司 SKU 主键集合（可为空）。
    ///
    /// # 错误
    /// 数据库查询失败时返回错误。
    async fn resolve_keyword_sku_ids(&self, keyword: &str) -> Result<Vec<SkuId>> {
        self.db
            .catalog()
            .resolve_sku_ids_by_keyword(keyword, &mut NoTransaction)
            .await
            .map_err(Into::into)
    }

    /// 按 SPU 编号 / SKU 编号解析公司 SKU 主键（模糊、忽略大小写）。
    ///
    /// 两者同时给出时取交集；任一条件无命中时返回空集合，表示分页查询必然无结果。
    ///
    /// # 参数
    /// * `product_no` - 已去空白的公司商品编号
    /// * `sku_no` - 已去空白的公司 SKU 编号
    ///
    /// # 返回
    /// 返回去重后的命中 SKU 主键集合（可为空）。
    ///
    /// # 错误
    /// 数据库查询失败时返回错误。
    async fn resolve_sku_ids_by_codes(
        &self,
        product_no: Option<&str>,
        sku_no: Option<&str>,
    ) -> Result<Option<Vec<SkuId>>> {
        self.db
            .catalog()
            .resolve_sku_ids_by_codes(product_no, sku_no, &mut NoTransaction)
            .await
            .map_err(Into::into)
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
        let prior_revision = match offering.stable.current_revision_id.as_ref() {
            Some(revision_id) => Some(
                self.db
                    .supplier_offering_revisions()
                    .find_by_id(revision_id, &mut NoTransaction)
                    .await?
                    .ok_or_else(|| {
                        Error::BusinessLogicError(
                            "供给当前修订不存在，禁止形成无法判定影响的新版本".to_string(),
                        )
                    })?,
            ),
            None => None,
        };
        let current_no = self.current_revision_no(&offering).await?;
        let next_no = offering
            .next_revision_no(current_no, req.expected_revision_no)
            .map_err(|_| Error::ConflictError("供给版本已经变化，请刷新后重新保存".to_string()))?;
        let revision = build_revision(
            &SupplierOfferingId::new(offering.base.id.clone()),
            next_no,
            &req.terms,
        )?;
        let next_status = req.status.unwrap_or(offering.stable.status);
        let prior_status = offering.stable.status;
        if next_status == OfferingStatus::Active {
            self.ensure_qualified(&offering.supplier_id, &offering.sku_id, revision.valid_from)
                .await?;
        }
        offering.update_status(next_status, actor.id())?;
        offering.stable.current_revision_id = Some(revision.base.id.clone());
        let expected_version = offering.next_persisted_version()?;
        let audit = actor.clone().resource_log_with_message(
            "supplier_offering.revise",
            "supplier_offering",
            offering.base.id.clone(),
            Some(req.change_reason),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let safety_pause_cause = if next_status == OfferingStatus::Stopped && prior_status != next_status {
            Some(SafetyPauseCause::SupplierStopped)
        } else {
            prior_revision
                .as_ref()
                .and_then(|prior| safety_pause_cause_for_revision(revision.impact_from(prior)))
        };
        let safety_pause = safety_pause_cause.map(|cause| {
            let source_version = if cause == SafetyPauseCause::SupplierStopped {
                format!("offering:{expected_version}")
            } else {
                format!("revision:{}", revision.base.id)
            };
            SystemSafetyPauseTrigger {
                cause,
                source_object_type: SafetyPauseSourceObjectType::SupplierOffering,
                source_object_id: offering.base.id.clone(),
                source_version: source_version.clone(),
                occurred_at: Instant::now(),
                idempotency_key: format!(
                    "w22:offering:{}:{}:{}",
                    offering.base.id,
                    cause.as_str().to_ascii_lowercase(),
                    source_version
                ),
                owner_user_id: actor.id().to_string(),
            }
        });
        let publication = PublicationService::new(db.clone(), Arc::new(UnavailableMallConnector));
        let idempotency_key = req.idempotency_key.clone();
        let fingerprint_for_tx = fingerprint.clone();
        let offering_id = offering.base.id.clone();
        let revision_id = revision.base.id.clone();
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.supplier_offering_repository()
                        .append_revision(&mut offering, &revision, session)
                        .await?;
                    let safety_pause = if let Some(trigger) = safety_pause.as_ref() {
                        publication
                            .system_safety_pause_in_transaction(trigger, session)
                            .await?
                    } else {
                        None
                    };
                    let result = ReviseSupplierOfferingResult {
                        offering_id,
                        revision_id,
                        revision_no: next_no,
                        status: next_status,
                        version: expected_version,
                        safety_pause,
                    };
                    let command =
                        build_command(&idempotency_key, "revise_offering", &fingerprint_for_tx, &result)?;
                    db.supplier_offering_commands().create(&command, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<ReviseSupplierOfferingResult, Error>(result)
                })
            })
            .await;
        self.resolve_written_result(
            transaction_result,
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
        let prior_safety_pause_cause =
            safety_pause_cause_for_availability(availability.interruption_reason());
        if let Some(expected_version) = req.expected_version {
            availability
                .ensure_version(expected_version)
                .map_err(|_| Error::ConflictError("可供状态已经变化，请刷新后重新保存".to_string()))?;
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
        let next_safety_pause_cause = safety_pause_cause_for_availability(availability.interruption_reason());
        let safety_pause_cause = (next_safety_pause_cause != prior_safety_pause_cause)
            .then_some(next_safety_pause_cause)
            .flatten();
        let result_version = availability.next_persisted_version()?;
        let audit = actor.clone().resource_log_with_message(
            "supplier_offering.availability.update",
            "supplier_offering",
            id.to_string(),
            Some(req.change_reason),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let safety_pause = safety_pause_cause.map(|cause| SystemSafetyPauseTrigger {
            cause,
            source_object_type: SafetyPauseSourceObjectType::SupplierOffering,
            source_object_id: offering_id.to_string(),
            source_version: format!("availability:{result_version}"),
            occurred_at: source_updated_at,
            idempotency_key: format!(
                "w22:offering:{}:{}:{}",
                offering_id,
                cause.as_str().to_ascii_lowercase(),
                result_version
            ),
            owner_user_id: actor.id().to_string(),
        });
        let publication = PublicationService::new(db.clone(), Arc::new(UnavailableMallConnector));
        let idempotency_key = req.idempotency_key.clone();
        let fingerprint_for_tx = fingerprint.clone();
        let result_offering_id = offering_id.to_string();
        let result_status = availability.availability_status;
        let result_source_updated_at = availability.source_updated_at.unix_secs();
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.supplier_offering_availabilities()
                        .update(&mut availability, session)
                        .await?;
                    let safety_pause = if let Some(trigger) = safety_pause.as_ref() {
                        publication
                            .system_safety_pause_in_transaction(trigger, session)
                            .await?
                    } else {
                        None
                    };
                    let result = UpdateSupplierOfferingAvailabilityResult {
                        offering_id: result_offering_id,
                        availability_status: result_status,
                        availability_version: result_version,
                        source_updated_at: result_source_updated_at,
                        safety_pause,
                    };
                    let command = build_command(
                        &idempotency_key,
                        "update_offering_availability",
                        &fingerprint_for_tx,
                        &result,
                    )?;
                    db.supplier_offering_commands().create(&command, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<UpdateSupplierOfferingAvailabilityResult, Error>(result)
                })
            })
            .await;
        self.resolve_written_result(
            transaction_result,
            &req.idempotency_key,
            "update_offering_availability",
            &fingerprint,
        )
        .await
    }

    /// 核对供应停止来源与安全暂停影响，并完成其唯一正式后续任务。
    ///
    /// 本命令只关闭人工核对责任；不可变安全暂停证据、暂停修订和发布暂停状态
    /// 均保持不变，不得将“任务完成”解释为恢复供给或恢复发布。
    ///
    /// # 错误
    /// 任务、来源对象、冻结版本、不可变安全暂停操作、当前责任或幂等请求任一
    /// 不一致时失败关闭。
    pub async fn complete_supply_exception_task(
        &self,
        id: &str,
        req: CompleteSupplierSupplyExceptionTaskRequest,
        actor: &AuditActor,
    ) -> Result<CompleteSupplierSupplyExceptionTaskResult> {
        req.validate()?;
        let offering_id = id.trim();
        if offering_id.is_empty() || req.decision.offering_id.trim() != offering_id {
            return Err(Error::ValidationError("路径供给 ID 与任务决定不一致".to_string()));
        }
        let work_item_id = req.work_item_id.trim();
        let subject_version = req.expected_subject_version.trim();
        if work_item_id.is_empty() || subject_version.is_empty() {
            return Err(Error::ValidationError("任务 ID 与来源版本不能为空".to_string()));
        }
        let expected_task_version = crate::work_item::expected_task_version(&req.expected_task_version)?;
        let receipt = CommandReceipt::new(
            "supplier-supply-exception:",
            actor,
            SUPPLY_EXCEPTION_COMPLETE_ACTION,
            "work_item",
            &req.idempotency_key,
            &req,
        )?;
        if let Some(committed_id) = receipt.committed_resource_id(&self.db).await? {
            return self.replay_supply_exception_completion(&committed_id, &req).await;
        }

        let db = self.db.clone();
        let client = db.client().clone();
        let rbac = crate::iam::shared_rbac_service(db.clone());
        let actor_for_tx = actor.clone();
        let req_for_tx = req.clone();
        let receipt_for_tx = receipt.clone();
        let offering_id_for_tx = offering_id.to_string();
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let typed_offering_id = SupplierOfferingId::new(&offering_id_for_tx);
                    db.supplier_offerings()
                        .find_by_id(&typed_offering_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("供应商供给不存在".to_string()))?;
                    let mut work_item = db
                        .work_items()
                        .find_by_id(&req_for_tx.work_item_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("供应停止任务不存在".to_string()))?;
                    ensure_supply_exception_work_item(
                        &work_item,
                        &offering_id_for_tx,
                        expected_task_version,
                        &req_for_tx.expected_subject_version,
                    )?;
                    WorkItemService::new(db.clone(), rbac.clone())
                        .ensure_domain_decision_access(&actor_for_tx, &work_item, session)
                        .await?;
                    let operation = db
                        .system_safety_pause_operations()
                        .find_safety_pause_by_work_item(&work_item.base.id, session)
                        .await?
                        .ok_or_else(|| {
                            Error::BusinessLogicError("供应停止任务缺少不可变安全暂停证据".to_string())
                        })?;
                    ensure_supply_exception_operation(&operation, &work_item)?;

                    let completed_at = Instant::now();
                    work_item.complete_by_domain_command(actor_for_tx.id(), completed_at)?;
                    let decision_audit = actor_for_tx.clone().resource_log_with_message(
                        SUPPLY_EXCEPTION_COMPLETE_ACTION,
                        "supplier_offering",
                        offering_id_for_tx.clone(),
                        Some(format!(
                            "证据引用：{}；核对结论：{}；安全暂停保持生效",
                            req_for_tx.decision.evidence_reference.trim(),
                            req_for_tx.decision.comment.trim()
                        )),
                    )?;
                    let receipt_audit =
                        receipt_for_tx.audit(actor_for_tx.clone(), work_item.base.id.clone())?;
                    db.work_items().update(&mut work_item, session).await?;
                    db.audit_logs().create(&decision_audit, session).await?;
                    db.audit_logs().create(&receipt_audit, session).await?;

                    Ok::<CompleteSupplierSupplyExceptionTaskResult, Error>(
                        supply_exception_completion_result(&operation, &req_for_tx),
                    )
                })
            })
            .await;

        match transaction_result {
            Ok(result) => Ok(result),
            Err(error) => match receipt.committed_resource_id(&self.db).await? {
                Some(committed_id) => self.replay_supply_exception_completion(&committed_id, &req).await,
                None => Err(error),
            },
        }
    }

    async fn replay_supply_exception_completion(
        &self,
        committed_work_item_id: &str,
        req: &CompleteSupplierSupplyExceptionTaskRequest,
    ) -> Result<CompleteSupplierSupplyExceptionTaskResult> {
        if committed_work_item_id != req.work_item_id.trim() {
            return Err(Error::ConflictError(
                "同一操作号已用于其它供应停止任务".to_string(),
            ));
        }
        let work_item = self
            .db
            .work_items()
            .find_by_id(committed_work_item_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("已提交任务结果不存在".to_string()))?;
        if work_item.status != WorkItemStatus::Completed
            || work_item.business_object_id != req.decision.offering_id.trim()
        {
            return Err(Error::Internal("已提交供应停止任务结果不完整".to_string()));
        }
        let operation = self
            .db
            .system_safety_pause_operations()
            .find_safety_pause_by_work_item(committed_work_item_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("已提交任务缺少安全暂停证据".to_string()))?;
        ensure_supply_exception_operation(&operation, &work_item)?;
        Ok(supply_exception_completion_result(&operation, req))
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
        self.db
            .supplier_offering_revisions()
            .current_revision_no(
                &SupplierOfferingId::new(offering.base.id.clone()),
                &mut NoTransaction,
            )
            .await
            .map_err(Into::into)
    }

    async fn current_revisions(
        &self,
        rows: &[database::SupplierOfferingRow],
    ) -> Result<HashMap<String, SupplierOfferingRevision>> {
        self.db
            .supplier_offering_repository()
            .load_current_revisions(rows, &mut NoTransaction)
            .await
            .map_err(Into::into)
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
        let (skus, sku_revisions, products, suppliers, parties, party_revisions) = self
            .db
            .supplier_offering_repository()
            .load_display_entities(rows, &mut NoTransaction)
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

    async fn resolve_written_result<T>(
        &self,
        transaction_result: Result<T>,
        idempotency_key: &str,
        operation: &str,
        fingerprint: &str,
    ) -> Result<T>
    where
        T: DeserializeOwned,
    {
        match transaction_result {
            Ok(result) => Ok(result),
            Err(error) => match self.command_record(idempotency_key).await? {
                Some(command) => replay_command(command, operation, fingerprint),
                None => Err(error),
            },
        }
    }
}

fn ensure_supply_exception_work_item(
    work_item: &WorkItem,
    offering_id: &str,
    expected_task_version: u64,
    expected_subject_version: &str,
) -> Result<()> {
    if work_item.work_item_type != WorkItemType::BusinessException
        || work_item.business_object_type != "SUPPLIER_OFFERING"
        || work_item.business_object_id != offering_id
        || work_item.reason_code.as_deref() != Some("SUPPLIER_STOPPED")
    {
        return Err(Error::BusinessLogicError(
            "当前任务不是已注册的供应停止核对任务".to_string(),
        ));
    }
    if work_item.status != WorkItemStatus::Open {
        return Err(Error::ConflictError("供应停止任务已不再开放".to_string()));
    }
    if work_item.base.version != expected_task_version {
        return Err(Error::ConflictError(
            "任务已被其他请求修改，请刷新后重试".to_string(),
        ));
    }
    if work_item.subject_version != expected_subject_version.trim() {
        return Err(Error::ConflictError(
            "供应停止来源版本已变化，请刷新后重试".to_string(),
        ));
    }
    Ok(())
}

fn ensure_supply_exception_operation(
    operation: &SystemSafetyPauseOperation,
    work_item: &WorkItem,
) -> Result<()> {
    let bound = matches!(
        &operation.follow_up,
        SafetyPauseFollowUp::WorkItem(reference)
            if reference.work_item_id == work_item.base.id
                && reference.business_object_type == work_item.business_object_type
                && reference.business_object_id == work_item.business_object_id
                && reference.subject_version == work_item.subject_version
                && reference.handler_key == "supplier_supply_exception"
    );
    if operation.cause != SafetyPauseCause::SupplierStopped
        || operation.source_object_type != SafetyPauseSourceObjectType::SupplierOffering
        || operation.source_object_id != work_item.business_object_id
        || operation.source_version != work_item.subject_version
        || !bound
    {
        return Err(Error::BusinessLogicError(
            "供应停止任务与不可变安全暂停证据不一致".to_string(),
        ));
    }
    Ok(())
}

fn supply_exception_completion_result(
    operation: &SystemSafetyPauseOperation,
    req: &CompleteSupplierSupplyExceptionTaskRequest,
) -> CompleteSupplierSupplyExceptionTaskResult {
    CompleteSupplierSupplyExceptionTaskResult {
        work_item_id: req.work_item_id.trim().to_string(),
        safety_pause_operation_id: operation.base.id.clone(),
        evidence_reference: req.decision.evidence_reference.trim().to_string(),
        message: "供应停止来源与安全暂停影响已核对；任务已完成，安全暂停继续生效".to_string(),
    }
}

/// 将供给域的可供中断原因映射为发布安全暂停原因。
///
/// 映射属于 D24 供给与 D26 发布之间的集成适配合同；`None` 表示当前可供事实
/// 不要求触发发布安全暂停。新增中断原因时必须在此穷尽声明其发布影响。
fn safety_pause_cause_for_availability(
    reason: Option<AvailabilityInterruptionReason>,
) -> Option<SafetyPauseCause> {
    reason.map(|reason| match reason {
        AvailabilityInterruptionReason::SupplierStopped => SafetyPauseCause::SupplierStopped,
        AvailabilityInterruptionReason::SupplyUnavailable => SafetyPauseCause::SupplyUnavailable,
        AvailabilityInterruptionReason::AvailabilityStale => SafetyPauseCause::AvailabilityStale,
        AvailabilityInterruptionReason::ZeroInventory => SafetyPauseCause::ZeroInventory,
    })
}

/// 将供给商业条款影响映射为发布安全暂停原因。
///
/// 映射属于 D24 供给与 D26 发布之间的集成适配合同；无商业影响时不得生成
/// 暂停原因。新增修订影响时必须在此穷尽声明其发布影响。
fn safety_pause_cause_for_revision(impact: OfferingRevisionImpact) -> Option<SafetyPauseCause> {
    match impact {
        OfferingRevisionImpact::None => None,
        OfferingRevisionImpact::CostChanged => Some(SafetyPauseCause::CostChangeUnconfirmed),
        OfferingRevisionImpact::CriticalSupplyChanged => {
            Some(SafetyPauseCause::CriticalSupplyChangeUnconfirmed)
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
        SupplierOfferingRevisionData::from_gross_prices(
            offering_id.clone(),
            revision_no,
            dropship_gross,
            bulk_gross,
            rate,
            terms.dropship_express.clone(),
            parse_amount(terms.freight_amount.as_deref())?,
            parse_amount(terms.service_fee_amount.as_deref())?,
            Quantity::from_str(terms.bulk_minimum_order_quantity.trim()).map_err(|_| {
                Error::ValidationError(format!("非法集采起订量: {}", terms.bulk_minimum_order_quantity))
            })?,
            terms.supply_region.clone(),
            terms.product_capabilities.clone(),
            parse_business_date(&terms.valid_from)?,
            terms.valid_to.as_deref().map(parse_business_date).transpose()?,
            PrefillSourceRefs::default(),
        ),
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
    Ok(hex::encode(Sha256::digest(payload)))
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
    use super::{
        ensure_supply_exception_operation, ensure_supply_exception_work_item,
        safety_pause_cause_for_availability, safety_pause_cause_for_revision, typed_id,
    };
    use entities::common::time::Instant;
    use entities::ids::{
        ProductPublicationDeliveryId, ProductPublicationId, ProductPublicationRevisionId, SkuId, WorkItemId,
    };
    use entities::publication::{
        SafetyPauseAffectedPublication, SafetyPauseCause, SafetyPauseFollowUp, SafetyPauseSourceObjectType,
        SafetyPauseWorkItemRef, SystemSafetyPauseOperation, SystemSafetyPauseOperationData,
    };
    use entities::supplier_offering::{AvailabilityInterruptionReason, OfferingRevisionImpact};
    use entities::work_item::{AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemType};

    #[test]
    fn blank_filter_ids_are_omitted() {
        assert!(typed_id(Some("  "), SkuId::new).is_none());
        assert_eq!(typed_id(Some(" sku-1 "), SkuId::new).unwrap().as_ref(), "sku-1");
    }

    #[test]
    fn availability_pause_adapter_maps_every_stable_interruption_reason() {
        assert_eq!(safety_pause_cause_for_availability(None), None);
        let cases = [
            (
                AvailabilityInterruptionReason::SupplierStopped,
                SafetyPauseCause::SupplierStopped,
            ),
            (
                AvailabilityInterruptionReason::SupplyUnavailable,
                SafetyPauseCause::SupplyUnavailable,
            ),
            (
                AvailabilityInterruptionReason::AvailabilityStale,
                SafetyPauseCause::AvailabilityStale,
            ),
            (
                AvailabilityInterruptionReason::ZeroInventory,
                SafetyPauseCause::ZeroInventory,
            ),
        ];

        for (reason, expected) in cases {
            assert_eq!(safety_pause_cause_for_availability(Some(reason)), Some(expected));
        }
    }

    #[test]
    fn revision_pause_adapter_maps_every_stable_impact() {
        assert_eq!(
            safety_pause_cause_for_revision(OfferingRevisionImpact::None),
            None
        );
        assert_eq!(
            safety_pause_cause_for_revision(OfferingRevisionImpact::CostChanged),
            Some(SafetyPauseCause::CostChangeUnconfirmed)
        );
        assert_eq!(
            safety_pause_cause_for_revision(OfferingRevisionImpact::CriticalSupplyChanged),
            Some(SafetyPauseCause::CriticalSupplyChangeUnconfirmed)
        );
    }

    #[test]
    fn supply_exception_completion_requires_exact_frozen_task_identity() {
        let task = supply_exception_task();

        ensure_supply_exception_work_item(&task, "offering-1", task.base.version, "offering:2").unwrap();
        assert!(
            ensure_supply_exception_work_item(&task, "offering-2", task.base.version, "offering:2",).is_err()
        );
        assert!(
            ensure_supply_exception_work_item(&task, "offering-1", task.base.version, "offering:3",).is_err()
        );
    }

    #[test]
    fn supply_exception_completion_requires_bound_safety_pause_evidence() {
        let task = supply_exception_task();
        let operation = SystemSafetyPauseOperation::new(
            "pause-1",
            SystemSafetyPauseOperationData {
                cause: SafetyPauseCause::SupplierStopped,
                source_object_type: SafetyPauseSourceObjectType::SupplierOffering,
                source_object_id: task.business_object_id.clone(),
                source_version: task.subject_version.clone(),
                idempotency_key: "pause-key-1".to_string(),
                affected_publications: vec![SafetyPauseAffectedPublication {
                    publication_id: ProductPublicationId::new("publication-1"),
                    pause_revision_id: ProductPublicationRevisionId::new("revision-1"),
                    delivery_id: ProductPublicationDeliveryId::new("delivery-1"),
                }],
                follow_up: SafetyPauseFollowUp::WorkItem(SafetyPauseWorkItemRef {
                    work_item_id: task.base.id.clone(),
                    task_version: task.base.version,
                    business_object_type: task.business_object_type.clone(),
                    business_object_id: task.business_object_id.clone(),
                    subject_version: task.subject_version.clone(),
                    handler_key: "supplier_supply_exception".to_string(),
                }),
                occurred_at: Instant::from_unix_secs(1),
                committed_at: Instant::from_unix_secs(1),
            },
        )
        .unwrap();

        ensure_supply_exception_operation(&operation, &task).unwrap();
    }

    fn supply_exception_task() -> WorkItem {
        WorkItem::new_at(
            WorkItemId::new("work-item-1"),
            WorkItemData {
                work_item_type: WorkItemType::BusinessException,
                business_object_type: "SUPPLIER_OFFERING".to_string(),
                business_object_id: "offering-1".to_string(),
                subject_version: "offering:2".to_string(),
                owner_role: "role-operations".to_string(),
                owner_organization_id: "company".to_string(),
                owner_user_id: "operator-1".to_string(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::High,
                due_at: None,
                reason_code: Some("SUPPLIER_STOPPED".to_string()),
                impact_summary: Some("发布保持安全暂停".to_string()),
            },
            Instant::from_unix_secs(1),
        )
        .unwrap()
    }
}
