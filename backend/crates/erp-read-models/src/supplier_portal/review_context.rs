//! 内部新品核对的明确商品和既有供给候选，不把订货码相似性当作匹配决定。

use application_core::AuditActor;
use erp_catalog::CatalogExt;
use erp_catalog::portal::{
    CatalogPortalExt, CatalogPortalService, CategoryMappingSuggestion, DraftStatus, DuplicateCandidate,
};
use erp_identity::{PortalActor, PortalIdentityService};
use erp_supplier::SupplierExt;
use erp_supplier::portal::CooperationRepository;
use erp_supply::SupplierOfferingExt;
use erp_supply::dto::supplier_offering::SupplierOfferingTermsWrite;
use erp_supply::entity::supplier_offering::{SupplierOffering, SupplierOfferingRevision};
use erp_supply::portal::{PortalOfferingService, PortalSupplyExt};
use persistence_core::Executor;
use serde::Serialize;
use serde_json::{Value, json};

use super::repository::review_context::{existing_offerings, pending_applications};
use super::{PortalApplicationView, SupplierPortalReadService};
use crate::{Error, Result};

/// 本供应商精确订货码下可读的供给身份，审核人仍须明确选择并冻结版本。
#[derive(Debug, Serialize)]
pub struct ExistingOfferingReviewCandidate {
    pub row_id: String,
    pub offering_id: String,
    pub expected_offering_version: u64,
    pub expected_revision_no: u32,
    pub sku_id: String,
    pub supplier_sku_code: String,
    pub name: String,
}

/// 申请授权与商品、供给对象范围共同限制的内部核对上下文。
#[derive(Debug, Serialize)]
pub struct PortalNewProductReviewContext {
    pub duplicates: Vec<DuplicateCandidate>,
    pub existing_offerings: Vec<ExistingOfferingReviewCandidate>,
    pub category_mapping_suggestion: Option<CategoryMappingSuggestion>,
}

impl SupplierPortalReadService {
    /// 读取本供给已提交待采购确认的条款与停供申请。
    ///
    /// # 参数
    /// `actor` 为门户身份，`offering_id` 为精确供给，`executor` 为同一读取执行器。
    /// # 返回
    /// 只含本供应商该供给的待确认允许列表，至多100项。
    /// # 错误
    /// 账号、绑定或供应商失效，供给越界，结果超限或持久化错误时拒绝。
    pub async fn pending_offering_applications(
        &self,
        actor: &PortalActor,
        offering_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PortalApplicationView>> {
        let current = PortalIdentityService::new(self.db.clone())
            .validate_session(
                &actor.account_id,
                &actor.account,
                actor.account_version,
                actor.binding_version,
                executor,
            )
            .await?;
        if current.supplier_id != actor.supplier_id {
            return Err(Error::Unauthenticated("认证已失效".into()));
        }
        self.portal_supplier(&current, executor).await?;
        PortalOfferingService::new(self.db.clone())
            .require_owned_offering(&current.supplier_id, &current.audit_actor(), offering_id, executor)
            .await?;
        pending_applications(&self.db, &current.supplier_id, offering_id, executor)
            .await?
            .iter()
            .map(PortalApplicationView::from_offering)
            .collect()
    }

    /// 读取原冻结目标当前正式事实，供内部审核逐字段比较。
    ///
    /// # 参数
    /// `request_id` 为精确申请，`actor` 为内部账号，`executor` 为本次读取执行器。
    /// # 返回
    /// 供给返回当前正式条款和版本，合作返回当前正式付款条件；新品和首次报价为空。
    /// # 错误
    /// 申请或供给不可见、目标归属异常、正式版本缺失或持久化错误时拒绝。
    pub async fn current_application_facts(
        &self,
        request_id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Value> {
        let authorization = self.internal_authorization(actor)?;
        if !authorization.request_readable(actor, request_id, executor).await? {
            return Err(hidden_target());
        }
        if let Some(app) = self.db.portal_applications().find_by_id(request_id, executor).await? {
            let frozen =
                app.submissions.last().map(|submission| &submission.snapshot).unwrap_or(&app.snapshot);
            let Some((id, expected_version, expected_revision_no)) = frozen.target() else {
                return Ok(Value::Null);
            };
            if !authorization.offering_readable(actor, id, executor).await? {
                return Err(hidden_target());
            }
            let mut facts = self.current_offering_facts(&app.supplier_id, id, executor).await?;
            facts["expected_offering_version"] = json!(expected_version);
            facts["expected_revision_no"] = json!(expected_revision_no);
            return Ok(facts);
        }
        if let Some(app) = CooperationRepository::new(&self.db).find_any(request_id, executor).await? {
            return self.current_cooperation_facts(&app.supplier_id, executor).await;
        }
        if self.db.new_product_drafts().find_by_id(request_id, executor).await?.is_some() {
            return Ok(Value::Null);
        }
        Err(hidden_target())
    }

    /// 读取新品的公司重复候选和精确既有供给引用。
    ///
    /// # 参数
    /// `request_id` 为精确申请，`q` 为可选搜索，`actor` 为内部账号；
    /// `catalog` 必须装配正式商品范围授权，`executor` 为同一读取执行器。
    /// # 返回
    /// 返回可维护商品候选及本供应商、原订货码下可读的供给版本。
    /// # 错误
    /// 申请不可见、授权未装配、候选资料非法或数据库错误时拒绝。
    pub async fn new_product_review_context(
        &self,
        request_id: &str,
        q: Option<&str>,
        actor: &AuditActor,
        catalog: &CatalogPortalService,
        executor: &mut dyn Executor,
    ) -> Result<PortalNewProductReviewContext> {
        let existing_offerings =
            self.existing_offering_review_candidates(request_id, actor, executor).await?;
        let duplicates = match q.map(str::trim).filter(|query| !query.is_empty()) {
            Some(query) => catalog.duplicate_candidates(query, actor, executor).await?,
            None => Vec::new(),
        };
        let draft =
            self.db.new_product_drafts().find_by_id(request_id, executor).await?.ok_or_else(hidden_target)?;
        let input = draft.frozen_input.as_ref().unwrap_or(&draft.draft);
        let category_mapping_suggestion = if input.category.raw_name.trim().is_empty() {
            None
        } else {
            catalog
                .category_mapping_suggestion(
                    &draft.supplier_id,
                    &input.category.raw_name,
                    input.product_kind,
                    executor,
                )
                .await?
        };
        Ok(PortalNewProductReviewContext { duplicates, existing_offerings, category_mapping_suggestion })
    }

    /// 为已授权新品详情提供明确的既有供给版本，不猜测其他订货码。
    ///
    /// # 参数
    /// `request_id` 为新品申请，`actor` 为内部账号，`executor` 为本次执行器。
    /// # 返回
    /// 返回当前待确认原稿每行的精确同供应商、同订货码候选；其他状态为空。
    /// # 错误
    /// 非内部身份或未装配授权时返回 `Forbidden`。申请不存在或越权时返回 `NotFound`。
    /// 供给当前版本缺失、归属异常或 SKU 资料缺失时返回 `ConflictError`。
    /// 待确认原稿关联损坏或仓储读取失败时返回对应错误。
    pub async fn existing_offering_review_candidates(
        &self,
        request_id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ExistingOfferingReviewCandidate>> {
        let authorization = self.internal_authorization(actor)?;
        if !authorization.request_readable(actor, request_id, executor).await? {
            return Err(hidden_target());
        }
        let draft =
            self.db.new_product_drafts().find_by_id(request_id, executor).await?.ok_or_else(hidden_target)?;
        if draft.status != DraftStatus::Pending {
            return Ok(Vec::new());
        }
        let mut result = Vec::new();
        for row in &draft.submitted()?.skus {
            let candidates =
                existing_offerings(&self.db, &draft.supplier_id, row.ordering_code.trim(), executor).await?;
            for offering in candidates {
                if authorization.offering_readable(actor, &offering.base.id, executor).await? {
                    result.push(self.existing_candidate(&row.row_id, offering, executor).await?);
                }
            }
        }
        Ok(result)
    }

    async fn existing_candidate(
        &self,
        row_id: &str,
        offering: SupplierOffering,
        executor: &mut dyn Executor,
    ) -> Result<ExistingOfferingReviewCandidate> {
        let revision_id = offering
            .stable
            .current_revision_id
            .as_ref()
            .ok_or_else(|| Error::ConflictError("既有供给当前版本缺失".into()))?;
        let revision = self
            .db
            .supplier_offering_revisions()
            .find_by_id(revision_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::ConflictError("既有供给当前版本缺失".into()))?;
        if revision.supplier_offering_id.as_ref() != offering.base.id {
            return Err(Error::ConflictError("既有供给当前版本归属异常".into()));
        }
        let sku = self
            .db
            .skus()
            .find_by_id(offering.sku_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::ConflictError("既有供给SKU不存在".into()))?;
        let sku_revision = self
            .db
            .catalog()
            .current_sku_revision(&sku, executor)
            .await?
            .ok_or_else(|| Error::ConflictError("既有供给SKU资料缺失".into()))?;
        Ok(ExistingOfferingReviewCandidate {
            row_id: row_id.into(),
            offering_id: offering.base.id,
            expected_offering_version: offering.base.version,
            expected_revision_no: revision.revision.revision_no,
            sku_id: offering.sku_id.to_string(),
            supplier_sku_code: offering.supplier_sku_code,
            name: sku_revision.name,
        })
    }

    async fn current_offering_facts(
        &self,
        supplier_id: &str,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Value> {
        let offering = self
            .db
            .supplier_offerings()
            .find_by_id(id, executor)
            .await?
            .filter(|offering| offering.supplier_id.as_ref() == supplier_id)
            .ok_or_else(hidden_target)?;
        let pointer = offering
            .stable
            .current_revision_id
            .as_ref()
            .ok_or_else(|| Error::ConflictError("当前正式供给版本缺失".into()))?;
        let revision = self
            .db
            .supplier_offering_revisions()
            .find_by_id(pointer.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::ConflictError("当前正式供给版本缺失".into()))?;
        if revision.supplier_offering_id.as_ref() != offering.base.id {
            return Err(Error::ConflictError("当前正式供给版本归属异常".into()));
        }
        Ok(json!({"offering_id":offering.base.id,"version":offering.base.version,
            "revision_id":revision.base.id,"revision_no":revision.revision.revision_no,
            "status":offering.stable.status,"terms":formal_terms(&revision)}))
    }

    async fn current_cooperation_facts(
        &self,
        supplier_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Value> {
        let supplier =
            self.db.supplier_accounts().find_by_id(supplier_id, executor).await?.ok_or_else(hidden_target)?;
        let pointer = supplier
            .current_commercial_profile_revision_id
            .as_ref()
            .ok_or_else(|| Error::ConflictError("当前正式商务版本缺失".into()))?;
        let profile = self
            .db
            .supplier_commercial_profile_revisions()
            .find_by_id(pointer.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::ConflictError("当前正式商务版本缺失".into()))?;
        if profile.supplier_id.as_ref() != supplier_id {
            return Err(Error::ConflictError("当前正式商务版本归属异常".into()));
        }
        Ok(json!({"supplier_version":supplier.base.version,"current_profile_id":profile.base.id,
            "profile_revision_no":profile.revision.revision_no,"settlement_mode":profile.settlement_mode,
            "reconciliation_cycle":profile.reconciliation_cycle,"payment_term":profile.effective_payment_term_code()}))
    }
}

fn formal_terms(revision: &SupplierOfferingRevision) -> SupplierOfferingTermsWrite {
    SupplierOfferingTermsWrite {
        dropship_supply_price_gross: revision.dropship_supply_price_gross.to_string(),
        bulk_supply_price_gross: revision.bulk_supply_price_gross.to_string(),
        input_tax_rate: revision.input_tax_rate.to_string(),
        bulk_minimum_order_quantity: revision.bulk_minimum_order_quantity.to_string(),
        supply_region: revision.supply_region.clone(),
        product_capabilities: revision.product_capabilities.clone(),
        valid_from: revision.valid_from.to_string(),
        valid_to: revision.valid_to.map(|date| date.to_string()),
        dropship_express: revision.dropship_express.clone(),
        freight_amount: revision.freight_amount.map(|amount| amount.to_string()),
        service_fee_amount: revision.service_fee_amount.map(|amount| amount.to_string()),
    }
}

fn hidden_target() -> Error {
    Error::NotFound("申请不存在或无权访问".into())
}
