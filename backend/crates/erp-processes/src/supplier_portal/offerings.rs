//! 可供直接维护与供给申请提交、撤回、采购决定。

use application_core::AuditActor;
use erp_catalog::CatalogExt;
use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::{SkuId, SupplierAccountId, WorkItemId};
use erp_identity::PortalActor;
use erp_supply::SupplierOfferingExt;
use erp_supply::dto::supplier_offering::SupplierOfferingTermsWrite;
use erp_supply::portal::{
    ConfirmedOfferingInput, OfferingApplication, OfferingApplicationResult, OfferingApplicationSnapshot,
    PortalAvailabilityInput, PortalAvailabilityUpdateResult, PortalOfferingService, PortalQuoteInput,
    validate_portal_available_quantity, validate_portal_quantities, validate_portal_reported_at,
    validate_portal_terms,
};
use erp_supply::ports::offering_qualification::{PortalQuoteQualificationPort, QualificationPort};
use erp_workflow::entity::work_item::SupplierPortalReviewTaskData;
use id_generator::next_id;
use persistence_core::{Executor, NoTransaction};
use serde_json::json;

use super::{PortalDecision, PortalReview, PortalTransition, SupplierPortalProcess};
use crate::adapters::offering_access;
use crate::adapters::workflow::work_item_service;
use crate::supply_governance::offering::MongoOfferingQualification;
use crate::{Error, Result};

impl SupplierPortalProcess {
    /// 即时更新本供应商可供事实，不改变商务和内部管控。
    /// # 参数
    /// 精确供给、读取版本、数量状态及真实门户身份。
    /// # 返回
    /// 返回已提交可供版本。
    /// # 错误
    /// 越权、API来源、版本或数量非法时拒绝。
    pub async fn availability_update(
        &self,
        id: &str,
        input: PortalAvailabilityInput,
        actor: &PortalActor,
    ) -> Result<PortalAvailabilityUpdateResult> {
        let payload = json!({"offering_id":id,"input":input});
        let id = id.to_string();
        let key = input.idempotency_key.clone();
        self.portal_command(
            actor,
            "supplier_portal.availability_update",
            &key,
            &payload,
            move |this, actor, executor| {
                Box::pin(async move {
                    this.validate_availability_unit(&actor, &id, &input, executor).await?;
                    Ok(PortalOfferingService::new(this.db)
                        .update_availability_with_change(
                            &actor.supplier_id,
                            &actor.audit_actor(),
                            &id,
                            &input,
                            executor,
                        )
                        .await?)
                })
            },
        )
        .await
    }

    /// 保存首次报价、调价或停止供应草稿。
    /// # 参数
    /// 可选草稿ID、门户允许字段及当前身份。
    /// # 返回
    /// 返回保存后版本，不创建正式供给。
    /// # 错误
    /// 归属、开放资格、原供给或草稿版本失效时拒绝。
    pub async fn application_save(
        &self,
        id: Option<&str>,
        input: PortalQuoteInput,
        actor: &PortalActor,
    ) -> Result<OfferingApplication> {
        let payload = json!({"application_id":id,"input":input});
        let id = id.map(str::to_string);
        let key = input.idempotency_key.clone();
        self.portal_command(
            actor,
            "supplier_portal.application_save",
            &key,
            &payload,
            move |this, actor, executor| {
                Box::pin(async move {
                    let service = PortalOfferingService::new(this.db.clone());
                    this.validate_snapshot(&actor, &input.snapshot, executor).await?;
                    let snapshot = service
                        .prepare_snapshot(
                            &actor.supplier_id,
                            &actor.audit_actor(),
                            input.snapshot,
                            BusinessDate::today(),
                            executor,
                        )
                        .await?;
                    let is_new = id.is_none();
                    let mut app = match id {
                        None => OfferingApplication::new(
                            next_id(),
                            &actor.supplier_id,
                            &actor.audit_actor(),
                            snapshot,
                            &input.reason,
                        )?,
                        Some(id) => {
                            let mut app = service.load_application(&id, executor).await?;
                            app.ensure_owned(
                                &actor.supplier_id,
                                &actor.audit_actor(),
                                input
                                    .expected_version
                                    .ok_or_else(|| Error::ValidationError("修改草稿必须提供版本".into()))?,
                            )?;
                            app.edit(snapshot, &input.reason)?;
                            app
                        },
                    };
                    service.save_application(&mut app, is_new, executor).await?;
                    Ok(app)
                })
            },
        )
        .await
    }

    /// 冻结原稿并创建当前具体采购人的确认任务。
    /// # 参数
    /// 精确草稿、版本命令及门户身份。
    /// # 返回
    /// 返回待确认申请和任务引用。
    /// # 错误
    /// 负责人无资格、范围或申请版本失效时不创建任务。
    pub async fn application_submit(
        &self,
        id: &str,
        input: PortalTransition,
        actor: &PortalActor,
    ) -> Result<OfferingApplication> {
        let payload = json!({"application_id":id,"input":input});
        let id = id.to_string();
        let key = input.idempotency_key.clone();
        self.portal_command(
            actor,
            "supplier_portal.application_submit",
            &key,
            &payload,
            move |this, actor, executor| {
                Box::pin(async move {
                    let service = PortalOfferingService::new(this.db.clone());
                    let mut app = service.load_application(&id, executor).await?;
                    app.ensure_owned(&actor.supplier_id, &actor.audit_actor(), input.expected_version)?;
                    this.validate_snapshot(&actor, &app.snapshot, executor).await?;
                    let (owner, org) = this.application_owner(&app, &actor, executor).await?;
                    let task_id = next_id();
                    app.submit(&actor.audit_actor(), &owner, &task_id, 1, Instant::now())?;
                    service.save_application(&mut app, false, executor).await?;
                    work_item_service(this.db.clone(), this.rbac.clone())
                        .create_supplier_portal_review(
                            WorkItemId::new(task_id),
                            SupplierPortalReviewTaskData {
                                request_id: id,
                                subject_version: offering_subject(&app)?,
                                owner_user_id: owner,
                                owner_organization_id: org,
                                due_at: None,
                                impact_summary: None,
                            },
                            executor,
                        )
                        .await?;
                    Ok(app)
                })
            },
        )
        .await
    }

    /// 撤回待确认申请，同事务关闭具体确认任务。
    /// # 参数
    /// 精确申请、版本命令及门户身份。
    /// # 返回
    /// 返回保留提交历史的已撤回申请。
    /// # 错误
    /// 已决定、过期版本或任务并发变更时拒绝。
    pub async fn application_withdraw(
        &self,
        id: &str,
        input: PortalTransition,
        actor: &PortalActor,
    ) -> Result<OfferingApplication> {
        let payload = json!({"application_id":id,"input":input});
        let id = id.to_string();
        let key = input.idempotency_key.clone();
        self.portal_command(
            actor,
            "supplier_portal.application_withdraw",
            &key,
            &payload,
            move |this, actor, executor| {
                Box::pin(async move {
                    let service = PortalOfferingService::new(this.db.clone());
                    let mut app = service.load_application(&id, executor).await?;
                    app.ensure_owned(&actor.supplier_id, &actor.audit_actor(), input.expected_version)?;
                    let task =
                        app.submissions.last().ok_or_else(|| Error::ConflictError("申请尚未提交".into()))?;
                    let subject = offering_subject(&app)?;
                    this.withdraw_task(&id, &subject, &task.work_item_id, &actor.account_id, executor)
                        .await?;
                    app.withdraw(&actor.audit_actor())?;
                    service.save_application(&mut app, false, executor).await?;
                    Ok(app)
                })
            },
        )
        .await
    }

    /// 由当前具体采购负责人通过或退回原提交。
    /// # 参数
    /// 申请、任务、各自版本以及内部核对命令。
    /// # 返回
    /// 返回正式供给结果或退回原因。
    /// # 错误
    /// 任一身份、资格、对象范围或版本失效时整单回滚。
    pub async fn application_review(
        &self,
        id: &str,
        input: PortalReview,
        actor: &AuditActor,
    ) -> Result<OfferingApplication> {
        let current =
            PortalOfferingService::new(self.db.clone()).load_application(id, &mut NoTransaction).await?;
        let payload = json!({"application_id":id,"input":input});
        let id = id.to_string();
        let key = input.idempotency_key.clone();
        let action = review_action(&input.decision);
        self.internal_command(
            actor,
            &current.supplier_id,
            action,
            &key,
            &payload,
            move |this, actor, executor| {
                Box::pin(async move { this.review_offering(&id, &input, &actor, executor).await })
            },
        )
        .await
    }

    async fn review_offering(
        &self,
        id: &str,
        input: &PortalReview,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<OfferingApplication> {
        let service = PortalOfferingService::new(self.db.clone());
        let mut app = service.load_application(id, executor).await?;
        app.decision_submission(actor, input.expected_version, &input.work_item_id, input.work_item_version)?;
        let subject = offering_subject(&app)?;
        self.validate_task(id, &subject, input, actor, executor).await?;
        if let Some((offering_id, _, _)) = app.snapshot.target() {
            offering_access(self.db.clone(), self.rbac.clone())
                .require_offering(actor, "update", offering_id, executor)
                .await?;
        }
        let result = match input.decision {
            PortalDecision::Return => None,
            PortalDecision::Approve => Some(self.approve_offering(&app, input, actor, executor).await?),
        };
        self.complete_task(id, &subject, input, actor, executor).await?;
        app.decide(actor, result, review_comment(input)?, Instant::now())?;
        service.save_application(&mut app, false, executor).await?;
        Ok(app)
    }

    async fn approve_offering(
        &self,
        app: &OfferingApplication,
        input: &PortalReview,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<OfferingApplicationResult> {
        let service = PortalOfferingService::new(self.db.clone());
        validate_quote_reported_time(&app.snapshot, input.availability_reported_at_confirmed)?;
        self.validate_snapshot_units(&app.snapshot, executor).await?;
        self.submission_actor(
            &app.supplier_id,
            &app.submissions.last().ok_or_else(|| Error::Internal("缺少冻结提交".into()))?.submitted_by,
            executor,
        )
        .await?;
        let (owner, org) = self.confirmed_offering_owner(app, input, actor, executor).await?;
        offering_access(self.db.clone(), self.rbac.clone())
            .ensure_writable(
                actor,
                if app.snapshot.target().is_some() { "update" } else { "create" },
                &owner,
                &org,
                executor,
            )
            .await?;
        Ok(service
            .apply_application(
                ConfirmedOfferingInput {
                    application: app,
                    actor,
                    maintainer_user_id: &owner,
                    business_org_unit_id: &org,
                    on_date: BusinessDate::today(),
                },
                &MongoOfferingQualification::new(self.db.clone()),
                executor,
            )
            .await?
            .result)
    }

    async fn confirmed_offering_owner(
        &self,
        app: &OfferingApplication,
        input: &PortalReview,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(String, String)> {
        let access = offering_access(self.db.clone(), self.rbac.clone());
        if let Some((id, _, _)) = app.snapshot.target() {
            let existing = access.require_offering(actor, "update", id, executor).await?;
            return Ok((existing.maintainer_user_id, existing.business_org_unit_id));
        }
        let owner = input
            .maintainer_user_id
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| Error::ValidationError("首次供给必须明确内部维护人".into()))?;
        self.offering_maintainer(owner, actor, executor).await
    }

    pub(super) async fn validate_snapshot(
        &self,
        actor: &PortalActor,
        snapshot: &OfferingApplicationSnapshot,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let service = PortalOfferingService::new(self.db.clone());
        match snapshot {
            OfferingApplicationSnapshot::ExistingQuote {
                sku_id,
                target_version,
                supplier_sku_code,
                terms,
                ..
            } => {
                let qualification = MongoOfferingQualification::new(self.db.clone());
                if service
                    .ordering_identity(&actor.supplier_id, sku_id, supplier_sku_code, executor)
                    .await?
                    .is_none()
                {
                    service.ensure_quote_access(&actor.supplier_id, sku_id, executor).await?;
                }
                qualification.ensure_quote_target(&SkuId::new(sku_id), target_version, executor).await?;
                validate_portal_terms(terms, BusinessDate::today())?;
                qualification
                    .ensure_qualified(
                        &SupplierAccountId::new(&actor.supplier_id),
                        &SkuId::new(sku_id),
                        BusinessDate::today(),
                        executor,
                    )
                    .await?;
            },
            OfferingApplicationSnapshot::TermsChange { offering_id, terms, .. } => {
                service
                    .require_owned_offering(&actor.supplier_id, &actor.audit_actor(), offering_id, executor)
                    .await?;
                validate_portal_terms(terms, BusinessDate::today())?;
            },
            OfferingApplicationSnapshot::StopSupply { offering_id, .. } => {
                service
                    .require_owned_offering(&actor.supplier_id, &actor.audit_actor(), offering_id, executor)
                    .await?;
            },
        }
        self.validate_snapshot_units(snapshot, executor).await?;
        Ok(())
    }

    pub(super) async fn validate_snapshot_units(
        &self,
        snapshot: &OfferingApplicationSnapshot,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        match snapshot {
            OfferingApplicationSnapshot::ExistingQuote { sku_id, terms, available_quantity, .. } => {
                self.validate_supply_quantity(sku_id, terms, available_quantity.as_deref(), executor).await?
            },
            OfferingApplicationSnapshot::TermsChange { offering_id, terms, .. } => {
                let offering = self
                    .db
                    .supplier_offerings()
                    .find_by_id(offering_id, executor)
                    .await?
                    .ok_or_else(|| Error::NotFound("供给不存在".into()))?;
                self.validate_supply_quantity(offering.sku_id.as_ref(), terms, None, executor).await?;
            },
            OfferingApplicationSnapshot::StopSupply { .. } => {},
        }
        Ok(())
    }

    pub(super) async fn validate_supply_quantity(
        &self,
        sku_id: &str,
        terms: &SupplierOfferingTermsWrite,
        quantity: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let sku = self
            .db
            .skus()
            .find_by_id(sku_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("公司SKU不存在".into()))?;
        let unit = self
            .db
            .unit_of_measures()
            .find_by_id(sku.base_unit_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::BusinessLogicError("SKU基础单位不存在".into()))?;
        if !unit.is_active() {
            return Err(Error::BusinessLogicError("SKU基础单位已停用".into()));
        }
        validate_portal_quantities(terms, quantity, unit.quantity_scale)?;
        Ok(())
    }

    pub(super) async fn validate_availability_unit(
        &self,
        actor: &PortalActor,
        id: &str,
        input: &PortalAvailabilityInput,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let offering = PortalOfferingService::new(self.db.clone())
            .require_owned_offering(&actor.supplier_id, &actor.audit_actor(), id, executor)
            .await?;
        let sku = self
            .db
            .skus()
            .find_by_id(offering.sku_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::NotFound("公司SKU不存在".into()))?;
        let unit = self
            .db
            .unit_of_measures()
            .find_by_id(sku.base_unit_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::BusinessLogicError("SKU基础单位不存在".into()))?;
        if !unit.is_active() {
            return Err(Error::BusinessLogicError("SKU基础单位已停用".into()));
        }
        validate_portal_available_quantity(input.available_quantity.as_deref(), unit.quantity_scale)?;
        Ok(())
    }

    pub(super) async fn application_owner(
        &self,
        app: &OfferingApplication,
        actor: &PortalActor,
        executor: &mut dyn Executor,
    ) -> Result<(String, String)> {
        if let Some((id, _, _)) = app.snapshot.target() {
            let item = PortalOfferingService::new(self.db.clone())
                .require_owned_offering(&actor.supplier_id, &actor.audit_actor(), id, executor)
                .await?;
            return Ok((item.maintainer_user_id, item.business_org_unit_id));
        }
        let supplier = self.active_supplier(&app.supplier_id, executor).await?;
        Ok((supplier.maintainer_user_id, supplier.business_org_unit_id))
    }
}

pub(super) fn offering_subject(app: &OfferingApplication) -> Result<String> {
    app.submissions
        .last()
        .map(|s| format!("offering:{}", s.submission_no))
        .ok_or_else(|| Error::ConflictError("申请尚未提交".into()))
}
fn review_action(decision: &PortalDecision) -> &'static str {
    match decision {
        PortalDecision::Approve => "supplier_portal.application_approve",
        PortalDecision::Return => "supplier_portal.application_return",
    }
}
fn validate_quote_reported_time(snapshot: &OfferingApplicationSnapshot, confirmed: bool) -> Result<()> {
    if let OfferingApplicationSnapshot::ExistingQuote { availability_reported_at, .. } = snapshot {
        if !confirmed {
            return Err(Error::ValidationError("请人工核对供应商实际填报时间后确认首次报价".into()));
        }
        validate_portal_reported_at(*availability_reported_at, Instant::now())?;
    }
    Ok(())
}
pub(super) fn review_comment(input: &PortalReview) -> Result<&str> {
    match input.comment.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(value) => Ok(value),
        None if matches!(input.decision, PortalDecision::Approve) => Ok("采购确认通过"),
        None => Err(Error::ValidationError("退回必须填写原因".into())),
    }
}
