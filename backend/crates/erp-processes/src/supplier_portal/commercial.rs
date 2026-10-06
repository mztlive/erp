//! 合作条款按供应商商务修订生效，不影响供给价格和已冻结采购。

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_core::ids::{SupplierCommercialProfileRevisionId, WorkItemId};
use erp_identity::PortalActor;
use erp_supplier::SupplierExt;
use erp_supplier::portal::{CooperationApplication, CooperationRepository, plan_confirmed_cooperation};
use erp_workflow::entity::work_item::SupplierPortalReviewTaskData;
use id_generator::next_id;
use persistence_core::{Executor, NoTransaction};
use serde_json::json;

use super::offerings::review_comment;
use super::{CooperationSave, PortalDecision, PortalReview, PortalTransition, SupplierPortalProcess};
use crate::adapters::workflow::work_item_service;
use crate::{Error, Result};

impl SupplierPortalProcess {
    /// 保存当前供应商付款条件及合作条款草稿。
    /// # 参数
    /// 可选草稿ID、合作资料快照与真实门户身份。
    /// # 返回
    /// 返回未生效的草稿版本。
    /// # 错误
    /// 归属、版本或商务规则不符时拒绝。
    pub async fn cooperation_save(
        &self,
        id: Option<&str>,
        input: CooperationSave,
        actor: &PortalActor,
    ) -> Result<CooperationApplication> {
        let payload = json!({"application_id":id,"input":input});
        let key = input.idempotency_key.clone();
        let id = id.map(str::to_string);
        self.portal_command(
            actor,
            "supplier_portal.commercial_save",
            &key,
            &payload,
            move |this, actor, executor| {
                Box::pin(async move {
                    let repo = CooperationRepository::new(&this.db);
                    let app = if let Some(id) = id {
                        let mut app = repo
                            .get(&id, &actor.supplier_id, executor)
                            .await?
                            .ok_or_else(|| Error::NotFound("合作申请不存在".into()))?;
                        app.edit(
                            &actor.supplier_id,
                            input
                                .expected_version
                                .ok_or_else(|| Error::ValidationError("修改申请必须提供版本".into()))?,
                            input.input,
                            &actor.audit_actor(),
                        )?;
                        repo.update(&mut app, executor).await?;
                        app
                    } else {
                        let app = CooperationApplication::new(
                            next_id(),
                            &actor.supplier_id,
                            input.input,
                            &actor.audit_actor(),
                        )?;
                        repo.create(&app, executor).await?;
                        app
                    };
                    Ok(app)
                })
            },
        )
        .await
    }

    /// 冻结合作条款并登记供应商当前维护人的确认任务。
    /// # 参数
    /// 精确申请、版本命令及真实门户身份。
    /// # 返回
    /// 返回待确认申请。
    /// # 错误
    /// 归属、版本或具体处理资格失效时拒绝。
    pub async fn cooperation_submit(
        &self,
        id: &str,
        input: PortalTransition,
        actor: &PortalActor,
    ) -> Result<CooperationApplication> {
        let payload = json!({"application_id":id,"input":input});
        let key = input.idempotency_key.clone();
        let id = id.to_string();
        self.portal_command(
            actor,
            "supplier_portal.commercial_submit",
            &key,
            &payload,
            move |this, actor, executor| {
                Box::pin(async move {
                    let repo = CooperationRepository::new(&this.db);
                    let mut app = repo
                        .get(&id, &actor.supplier_id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("合作申请不存在".into()))?;
                    let supplier = this.active_supplier(&actor.supplier_id, executor).await?;
                    let task_id = next_id();
                    app.submit(
                        &actor.supplier_id,
                        input.expected_version,
                        &supplier.maintainer_user_id,
                        &task_id,
                        &actor.audit_actor(),
                        now_u64()?,
                    )?;
                    repo.update(&mut app, executor).await?;
                    work_item_service(this.db.clone(), this.rbac.clone())
                        .create_supplier_portal_review(
                            WorkItemId::new(task_id),
                            SupplierPortalReviewTaskData {
                                request_id: id,
                                subject_version: cooperation_subject(&app)?,
                                owner_user_id: supplier.maintainer_user_id,
                                owner_organization_id: supplier.business_org_unit_id,
                                due_at: None,
                                impact_summary: Some("仅更新供应商商务档案，不回改已冻结采购付款条件".into()),
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

    /// 撤回待确认合作申请并关闭其采购任务。
    /// # 参数
    /// 精确申请、版本命令及真实门户身份。
    /// # 返回
    /// 返回已撤回申请。
    /// # 错误
    /// 越界、版本变化或已处理时拒绝。
    pub async fn cooperation_withdraw(
        &self,
        id: &str,
        input: PortalTransition,
        actor: &PortalActor,
    ) -> Result<CooperationApplication> {
        let payload = json!({"application_id":id,"input":input});
        let key = input.idempotency_key.clone();
        let id = id.to_string();
        self.portal_command(
            actor,
            "supplier_portal.commercial_withdraw",
            &key,
            &payload,
            move |this, actor, executor| {
                Box::pin(async move {
                    let repo = CooperationRepository::new(&this.db);
                    let mut app = repo
                        .get(&id, &actor.supplier_id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("合作申请不存在".into()))?;
                    let frozen = app.pending_submission()?;
                    this.withdraw_task(
                        &id,
                        &cooperation_subject(&app)?,
                        &frozen.task_id,
                        &actor.account_id,
                        executor,
                    )
                    .await?;
                    app.withdraw(
                        &actor.supplier_id,
                        input.expected_version,
                        &actor.audit_actor(),
                        now_u64()?,
                    )?;
                    repo.update(&mut app, executor).await?;
                    Ok(app)
                })
            },
        )
        .await
    }

    /// 当前采购确认人通过或退回合作条款。
    /// # 参数
    /// 精确申请、任务和读取版本及内部真实身份。
    /// # 返回
    /// 返回新商务档案结果或原稿退回原因。
    /// # 错误
    /// 资格、原商务档案、任务或申请版本失效时拒绝。
    pub async fn cooperation_review(
        &self,
        id: &str,
        input: PortalReview,
        actor: &AuditActor,
    ) -> Result<CooperationApplication> {
        let current = CooperationRepository::new(&self.db)
            .find_any(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("合作申请不存在".into()))?;
        let payload = json!({"application_id":id,"input":input});
        let key = input.idempotency_key.clone();
        let id = id.to_string();
        let action = match input.decision {
            PortalDecision::Approve => "supplier_portal.commercial_approve",
            PortalDecision::Return => "supplier_portal.commercial_return",
        };
        self.internal_command(
            actor,
            &current.supplier_id,
            action,
            &key,
            &payload,
            move |this, actor, executor| {
                Box::pin(async move { this.review_cooperation(&id, &input, &actor, executor).await })
            },
        )
        .await
    }

    async fn review_cooperation(
        &self,
        id: &str,
        input: &PortalReview,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<CooperationApplication> {
        let repo = CooperationRepository::new(&self.db);
        let mut app =
            repo.find_any(id, executor).await?.ok_or_else(|| Error::NotFound("合作申请不存在".into()))?;
        let subject = cooperation_subject(&app)?;
        self.validate_task(id, &subject, input, actor, executor).await?;
        match input.decision {
            PortalDecision::Return => {
                self.complete_task(id, &subject, input, actor, executor).await?;
                app.return_to_supplier(
                    input.expected_version,
                    actor,
                    review_comment(input)?.to_string(),
                    now_u64()?,
                )?;
            },
            PortalDecision::Approve => {
                self.submission_actor(&app.supplier_id, &app.pending_submission()?.submitted_by, executor)
                    .await?;
                let supplier = self.active_supplier(&app.supplier_id, executor).await?;
                let profile_id = supplier
                    .current_commercial_profile_revision_id
                    .as_ref()
                    .ok_or_else(|| Error::ConflictError("供应商缺少当前商务档案".into()))?;
                let current = self
                    .db
                    .supplier_commercial_profile_revisions()
                    .find_by_id(profile_id.as_ref(), executor)
                    .await?
                    .ok_or_else(|| Error::ConflictError("供应商商务档案不存在".into()))?;
                let mut plan = plan_confirmed_cooperation(
                    &app,
                    &supplier,
                    &current,
                    SupplierCommercialProfileRevisionId::new(next_id()),
                    actor,
                    now_u64()?,
                )?;
                repo.apply_confirmed(&supplier, &mut plan, executor).await?;
                self.complete_task(id, &subject, input, actor, executor).await?;
                app.activate(input.expected_version, actor, plan.result)?;
            },
        }
        repo.update(&mut app, executor).await?;
        Ok(app)
    }
}

pub(super) fn cooperation_subject(app: &CooperationApplication) -> Result<String> {
    app.submissions
        .last()
        .map(|s| format!("cooperation:{}", s.submission_no))
        .ok_or_else(|| Error::ConflictError("合作申请尚未提交".into()))
}
fn now_u64() -> Result<u64> {
    u64::try_from(Instant::now().unix_secs()).map_err(|_| Error::Internal("当前时钟无效".into()))
}
