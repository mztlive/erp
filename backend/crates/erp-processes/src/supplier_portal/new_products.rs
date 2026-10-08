//! 新品原稿、字典映射与正式商品和供给一次落地。

use application_core::AuditActor;
use async_trait::async_trait;
use entity_core::BaseModel;
use erp_catalog::portal::{
    CatalogDraftEffectiveCommand, CatalogDraftResult, CatalogDraftSubmitCommand, CatalogMaterializeCommand,
    CatalogPortalService, DraftSku, NewProductDraft, NewProductInput, NormalizedProduct, PackagingInput,
    PortalSupplyTermsPort,
};
use erp_catalog::{Error as CatalogError, Result as CatalogResult};
use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::{SkuId, WorkItemId};
use erp_identity::PortalActor;
use erp_supply::dto::supplier_offering::SupplierOfferingTermsWrite;
use erp_supply::portal::{
    ApplicationStatus, ConfirmedOfferingInput, FrozenOfferingSubmission, OfferingApplication,
    OfferingApplicationSnapshot, PortalAvailabilityStatus, PortalOfferingService,
    validate_portal_packaging_price, validate_portal_reported_at, validate_portal_terms,
};
use erp_supply::ports::offering_qualification::PortalQuoteQualificationPort;
use erp_workflow::entity::work_item::SupplierPortalReviewTaskData;
use id_generator::next_id;
use persistence_core::{Executor, NoTransaction};
use serde_json::{Value, json};

use super::offerings::review_comment;
use super::{NewProductSave, PortalDecision, PortalReview, PortalTransition, SupplierPortalProcess};
use crate::adapters::workflow::work_item_service;
use crate::adapters::{offering_access, scoped_catalog_service};
use crate::supply_governance::offering::MongoOfferingQualification;
use crate::{Error, Result};

pub(super) struct SupplyTerms;
#[async_trait]
impl PortalSupplyTermsPort for SupplyTerms {
    async fn validate(&self, value: &Value, _: &mut dyn Executor) -> CatalogResult<()> {
        let terms: SupplierOfferingTermsWrite = serde_json::from_value(value.clone())
            .map_err(|error| CatalogError::ValidationError(format!("供货条款无效: {error}")))?;
        validate_portal_terms(&terms, BusinessDate::today())
            .map_err(|error| CatalogError::ValidationError(error.to_string()))
    }
    async fn validate_packaging(
        &self,
        packaging: &PackagingInput,
        _: &mut dyn Executor,
    ) -> CatalogResult<()> {
        validate_portal_packaging_price(&packaging.original_unit_price)
            .map_err(|error| CatalogError::ValidationError(error.to_string()))
    }
}

struct SupplyContext<'a> {
    draft: &'a NewProductDraft,
    review: &'a PortalReview,
    actor: &'a AuditActor,
    owner: &'a str,
    org: &'a str,
}

impl SupplierPortalProcess {
    /// 组装绑定当前数据库和目录范围的新品门户服务。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回未额外缓存的 `CatalogPortalService`。
    ///
    /// # 错误
    /// 不返回错误。
    pub(super) fn catalog_portal(&self) -> CatalogPortalService {
        CatalogPortalService::new(self.db.clone(), scoped_catalog_service(self.db.clone(), self.rbac.clone()))
    }

    /// 保存供应商新品原稿，不建立正式商品或供给。
    /// # 参数
    /// 可选草稿ID、原始商品/SKU/图片及当前门户身份。
    /// # 返回
    /// 返回保存后草稿和版本。
    /// # 错误
    /// 越界、附件归属、版本或输入非法时拒绝。
    pub async fn new_product_save(
        &self,
        id: Option<&str>,
        input: NewProductSave,
        actor: &PortalActor,
    ) -> Result<NewProductDraft> {
        let payload = json!({"application_id":id,"input":input});
        let key = input.idempotency_key.clone();
        let id = id.map(str::to_string);
        self.portal_command(
            actor,
            "supplier_portal.new_product_save",
            &key,
            &payload,
            move |this, actor, executor| {
                Box::pin(async move {
                    this.validate_assets(&actor.supplier_id, id.as_deref(), &input.input, true, executor)
                        .await?;
                    let service = this.catalog_portal();
                    Ok(match id {
                        None => {
                            service
                                .create(next_id(), actor.supplier_id, actor.account_id, input.input, executor)
                                .await?
                        },
                        Some(id) => {
                            service
                                .update(
                                    &id,
                                    &actor.supplier_id,
                                    input.expected_version.ok_or_else(|| {
                                        Error::ValidationError("修改草稿必须提供版本".into())
                                    })?,
                                    input.input,
                                    executor,
                                )
                                .await?
                        },
                    })
                })
            },
        )
        .await
    }

    /// 冻结完整新品原稿并登记供应商维护人的采购确认任务。
    /// # 参数
    /// 申请ID、版本命令及当前门户身份。
    /// # 返回
    /// 返回带提交历史和具体任务的待确认申请。
    /// # 错误
    /// 资料、条款、文件或处理资格不合法时拒绝。
    pub async fn new_product_submit(
        &self,
        id: &str,
        input: PortalTransition,
        actor: &PortalActor,
    ) -> Result<NewProductDraft> {
        let payload = json!({"application_id":id,"input":input});
        let key = input.idempotency_key.clone();
        let id = id.to_string();
        self.portal_command(
            actor,
            "supplier_portal.new_product_submit",
            &key,
            &payload,
            move |this, actor, executor| {
                Box::pin(async move { this.submit_new_product(&id, &input, &actor, executor).await })
            },
        )
        .await
    }

    /// 在调用方事务内冻结新品原稿并登记采购确认任务。
    ///
    /// # 参数
    /// * `id` - 新品草稿 ID。
    /// * `input` - 期望版本与幂等键。
    /// * `actor` - 当前门户身份。
    /// * `executor` - 调用方事务执行器。
    ///
    /// # 返回
    /// 返回已提交、带确认任务的新品草稿。
    ///
    /// # 错误
    /// 草稿不可见、图片或附件不合法、确认人资格不足、提交版本冲突，或任务创建失败时返回对应错误。
    pub(super) async fn submit_new_product(
        &self,
        id: &str,
        input: &PortalTransition,
        actor: &PortalActor,
        executor: &mut dyn Executor,
    ) -> Result<NewProductDraft> {
        let service = self.catalog_portal();
        let draft = service.detail(id, &actor.supplier_id, executor).await?;
        draft.draft.ensure_submission_images()?;
        self.validate_assets(&actor.supplier_id, Some(id), &draft.draft, true, executor).await?;
        self.batch_new_product_reviewer(&draft, executor).await?;
        let supplier = self.active_supplier(&actor.supplier_id, executor).await?;
        let task_id = next_id();
        let draft = service
            .submit(
                draft,
                CatalogDraftSubmitCommand {
                    expected_version: input.expected_version,
                    submission_id: next_id(),
                    task_id: task_id.clone(),
                    actor_id: actor.account_id.clone(),
                },
                &SupplyTerms,
                executor,
            )
            .await?;
        work_item_service(self.db.clone(), self.rbac.clone())
            .create_supplier_portal_review(
                WorkItemId::new(task_id),
                SupplierPortalReviewTaskData {
                    request_id: id.to_string(),
                    subject_version: new_product_subject(&draft)?,
                    owner_user_id: supplier.maintainer_user_id,
                    owner_organization_id: supplier.business_org_unit_id,
                    due_at: None,
                    impact_summary: Some(
                        "新品入库后新建SKU保持未上架；复用已在售SKU的新增供给可能恢复选源".into(),
                    ),
                },
                executor,
            )
            .await?;
        Ok(draft)
    }

    /// 撤回待确认新品并关闭对应任务，保留全部原稿。
    /// # 参数
    /// 申请ID、版本命令及门户身份。
    /// # 返回
    /// 返回已撤回申请。
    /// # 错误
    /// 越界、版本漂移或已确认时拒绝。
    pub async fn new_product_withdraw(
        &self,
        id: &str,
        input: PortalTransition,
        actor: &PortalActor,
    ) -> Result<NewProductDraft> {
        let payload = json!({"application_id":id,"input":input});
        let key = input.idempotency_key.clone();
        let id = id.to_string();
        self.portal_command(
            actor,
            "supplier_portal.new_product_withdraw",
            &key,
            &payload,
            move |this, actor, executor| {
                Box::pin(async move {
                    let service = this.catalog_portal();
                    let draft = service.detail(&id, &actor.supplier_id, executor).await?;
                    let task_id = draft
                        .task_id
                        .as_deref()
                        .ok_or_else(|| Error::ConflictError("申请尚未提交".into()))?;
                    this.withdraw_task(
                        &id,
                        &new_product_subject(&draft)?,
                        task_id,
                        &actor.account_id,
                        executor,
                    )
                    .await?;
                    Ok(service
                        .withdraw(
                            &id,
                            &actor.supplier_id,
                            input.expected_version,
                            &actor.account_id,
                            executor,
                        )
                        .await?)
                })
            },
        )
        .await
    }

    /// 当前采购人一次确认商品、全部SKU、供给和正式素材引用。
    /// # 参数
    /// 精确申请/任务版本、字典映射及明确复用身份。
    /// # 返回
    /// 返回每个SKU及供给的实际建档结果。
    /// # 错误
    /// 资格、映射、资料含义或任一版本失效时整单回滚。
    pub async fn new_product_review(
        &self,
        id: &str,
        input: PortalReview,
        actor: &AuditActor,
    ) -> Result<NewProductDraft> {
        let current = self.catalog_portal().load(id, &mut NoTransaction).await?;
        let payload = json!({"application_id":id,"input":input});
        let key = input.idempotency_key.clone();
        let id = id.to_string();
        let action = match input.decision {
            PortalDecision::Approve => "supplier_portal.new_product_approve",
            PortalDecision::Return => "supplier_portal.new_product_return",
        };
        self.internal_command(
            actor,
            &current.supplier_id,
            action,
            &key,
            &payload,
            move |this, actor, executor| {
                Box::pin(async move { this.review_new_product(&id, &input, &actor, executor).await })
            },
        )
        .await
    }

    async fn review_new_product(
        &self,
        id: &str,
        input: &PortalReview,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<NewProductDraft> {
        let service = self.catalog_portal();
        let draft = service.load(id, executor).await?;
        let subject = new_product_subject(&draft)?;
        self.validate_task(id, &subject, input, actor, executor).await?;
        if matches!(input.decision, PortalDecision::Return) {
            self.complete_task(id, &subject, input, actor, executor).await?;
            return Ok(service
                .return_to_supplier(id, input.expected_version, review_comment(input)?, actor.id(), executor)
                .await?);
        }
        validate_reported_times(draft.submitted()?, input.availability_reported_at_confirmed)?;
        input
            .comment
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| Error::ValidationError("新品审核必须填写真实核对或匹配理由".into()))?;
        let (normalized, owner, org) =
            self.prepare_new_product_review(&draft, input, actor, executor).await?;
        self.validate_assets(&draft.supplier_id, Some(id), draft.submitted()?, true, executor).await?;
        let mut result = service
            .materialize(
                &CatalogMaterializeCommand {
                    draft_id: id.to_string(),
                    expected_version: input.expected_version,
                    normalized: normalized.clone(),
                    product_target: input.existing_product.clone(),
                    maintainer_user_id: owner.clone(),
                    business_org_unit_id: org.clone(),
                    actor: actor.clone(),
                },
                executor,
            )
            .await?;
        self.materialize_supply(
            SupplyContext { draft: &draft, review: input, actor, owner: &owner, org: &org },
            &mut result,
            executor,
        )
        .await?;
        self.finish_new_product(
            SupplyContext { draft: &draft, review: input, actor, owner: &owner, org: &org },
            normalized,
            result,
            executor,
        )
        .await
    }

    async fn finish_new_product(
        &self,
        context: SupplyContext<'_>,
        normalized: NormalizedProduct,
        result: CatalogDraftResult,
        executor: &mut dyn Executor,
    ) -> Result<NewProductDraft> {
        let id = &context.draft.base.id;
        let reason = context
            .review
            .comment
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| Error::ValidationError("新品审核必须填写真实核对或匹配理由".into()))?;
        self.complete_task(id, &new_product_subject(context.draft)?, context.review, context.actor, executor)
            .await?;
        Ok(self
            .catalog_portal()
            .mark_effective(
                CatalogDraftEffectiveCommand {
                    draft_id: id.to_string(),
                    expected_version: context.review.expected_version,
                    normalized,
                    result,
                    actor_id: context.actor.id().to_string(),
                    reason: reason.to_string(),
                },
                executor,
            )
            .await?)
    }

    async fn prepare_new_product_review(
        &self,
        draft: &NewProductDraft,
        input: &PortalReview,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(NormalizedProduct, String, String)> {
        let normalized = input
            .normalized_product
            .clone()
            .or(draft.normalized_product.clone())
            .ok_or_else(|| Error::ValidationError("请完成品牌分类单位及SKU映射".into()))?;
        self.submission_actor(
            &draft.supplier_id,
            &draft.submissions.last().ok_or_else(|| Error::Internal("缺少冻结提交".into()))?.submitted_by,
            executor,
        )
        .await?;
        let owner = input
            .maintainer_user_id
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| Error::ValidationError("必须明确商品及供给内部维护人".into()))?;
        let (owner, org) = self.new_product_maintainer(owner, actor, executor).await?;
        Ok((normalized, owner, org))
    }

    async fn materialize_supply(
        &self,
        context: SupplyContext<'_>,
        result: &mut CatalogDraftResult,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let service = PortalOfferingService::new(self.db.clone());
        for resolved in &mut result.skus {
            let row = context
                .draft
                .submitted()?
                .skus
                .iter()
                .find(|r| r.row_id == resolved.row_id)
                .ok_or_else(|| Error::Internal("正式SKU缺少原始提报行".into()))?;
            let (snapshot, target_owner, target_org) =
                self.new_product_supply_snapshot(&context, row, &resolved.sku_id, executor).await?;
            let app = supply_application(context.draft, snapshot)?;
            let confirmed = service
                .create_for_new_product(
                    ConfirmedOfferingInput {
                        application: &app,
                        actor: context.actor,
                        maintainer_user_id: &target_owner,
                        business_org_unit_id: &target_org,
                        on_date: BusinessDate::today(),
                    },
                    &MongoOfferingQualification::new(self.db.clone()),
                    executor,
                )
                .await?;
            resolved.offering_id = Some(confirmed.result.offering_id);
        }
        Ok(())
    }

    async fn new_product_supply_snapshot(
        &self,
        context: &SupplyContext<'_>,
        row: &DraftSku,
        sku_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<(OfferingApplicationSnapshot, String, String)> {
        let terms: SupplierOfferingTermsWrite = serde_json::from_value(row.supply_terms.clone())
            .map_err(|e| Error::ValidationError(e.to_string()))?;
        let quantity = row.available_quantity.map(|q| q.to_string());
        self.validate_supply_quantity(sku_id, &terms, quantity.as_deref(), executor).await?;
        let service = PortalOfferingService::new(self.db.clone());
        let access = offering_access(self.db.clone(), self.rbac.clone());
        if let Some(existing) = service
            .ordering_identity(&context.draft.supplier_id, sku_id, &row.ordering_code, executor)
            .await?
        {
            let frozen = context
                .review
                .existing_offerings
                .iter()
                .find(|r| r.row_id == row.row_id && r.offering_id == existing.base.id)
                .ok_or_else(|| Error::ConflictError("已存在供给，请明确核对其版本并转条款修订".into()))?;
            let existing =
                access.require_offering(context.actor, "update", &existing.base.id, executor).await?;
            return Ok((
                OfferingApplicationSnapshot::TermsChange {
                    offering_id: existing.base.id,
                    expected_offering_version: frozen.expected_offering_version,
                    expected_revision_no: frozen.expected_revision_no,
                    terms,
                },
                existing.maintainer_user_id,
                existing.business_org_unit_id,
            ));
        }
        access.ensure_writable(context.actor, "create", context.owner, context.org, executor).await?;
        Ok((
            OfferingApplicationSnapshot::ExistingQuote {
                sku_id: sku_id.to_string(),
                target_version: Box::new(
                    MongoOfferingQualification::new(self.db.clone())
                        .quote_target(&SkuId::new(sku_id), executor)
                        .await?,
                ),
                supplier_sku_code: row.ordering_code.clone(),
                supplier_product_code: None,
                terms,
                availability_status: PortalAvailabilityStatus::Available,
                available_quantity: row.available_quantity.map(|q| q.to_string()),
                availability_reported_at: row.reported_at,
            },
            context.owner.to_string(),
            context.org.to_string(),
        ))
    }
}

fn validate_reported_times(input: &NewProductInput, confirmed: bool) -> Result<()> {
    if !confirmed {
        return Err(Error::ValidationError("请人工核对每行供应商实际填报时间后确认".into()));
    }
    let received_at = Instant::now();
    for row in &input.skus {
        validate_portal_reported_at(row.reported_at, received_at)?;
    }
    Ok(())
}

/// 用当前提交编号构造新品任务主题。
///
/// # 参数
/// * `draft` - 已加载的新品草稿。
///
/// # 返回
/// 返回 `new_product:` 加当前提交编号。
///
/// # 错误
/// 尚未提交时返回 `ConflictError`。
pub(super) fn new_product_subject(draft: &NewProductDraft) -> Result<String> {
    draft
        .current_submission_id
        .as_ref()
        .map(|id| format!("new_product:{id}"))
        .ok_or_else(|| Error::ConflictError("新品尚未提交".into()))
}
fn supply_application(
    draft: &NewProductDraft,
    snapshot: OfferingApplicationSnapshot,
) -> Result<OfferingApplication> {
    let submitted = draft.submissions.last().ok_or_else(|| Error::Internal("新品缺少冻结提交".into()))?;
    Ok(OfferingApplication {
        base: BaseModel::new(draft.base.id.clone()),
        supplier_id: draft.supplier_id.clone(),
        created_by: draft.created_by.clone(),
        kind: snapshot.kind(),
        status: ApplicationStatus::Submitted,
        snapshot: snapshot.clone(),
        reason: "新品审核供给".into(),
        submissions: vec![FrozenOfferingSubmission {
            submission_no: 1,
            submitted_by: submitted.submitted_by.clone(),
            submitted_at: submitted.submitted_at,
            snapshot,
            reason: "新品原稿".into(),
            handler_id: String::new(),
            work_item_id: submitted.task_id.clone(),
            work_item_version: 1,
        }],
        decisions: Vec::new(),
        result: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn submitted_input(reported_at: Instant) -> NewProductInput {
        let dictionary = json!({"raw_name":"供应商原名"});
        serde_json::from_value(json!({"name":"新品","product_kind":erp_catalog::ProductKind::Physical,"brand":dictionary,"category":dictionary,"image_asset_ids":[],"file_asset_ids":[],"skus":[{"row_id":"sku-row","name":"新品SKU","spec_entries":[],"unit":dictionary,"ordering_code":"CODE","supply_terms":{},"available_quantity":null,"reported_at":reported_at}]})).unwrap()
    }

    #[test]
    fn approval_requires_explicit_report_time_review_without_rewriting_old_facts() {
        let input = submitted_input(Instant::from_unix_secs(10));
        let original = serde_json::to_value(&input).unwrap();
        assert!(validate_reported_times(&input, false).is_err());
        validate_reported_times(&input, true).unwrap();
        assert_eq!(serde_json::to_value(&input).unwrap(), original);
        let future = submitted_input(Instant::from_unix_secs(Instant::now().unix_secs() + 3600));
        assert!(validate_reported_times(&future, true).is_err());
    }
}
