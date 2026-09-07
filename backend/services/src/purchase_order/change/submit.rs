use entities::purchase_order::{
    PurchaseChangeOrder, PurchaseChangeOrderData, PurchaseChangeSubmission, PurchaseOrder,
    PurchaseOrderRevision,
};
use erp_audit::AuditExt;
use erp_core::common::time::Instant;
use erp_core::ids::PurchaseChangeOrderId;
use erp_workflow::entity::document_registry::{BusinessDocument, DocumentType};
use erp_workflow::DocumentRegistryExt;
use id_generator::next_id;
use mongodb::ClientSession;
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;
use {database::PurchaseOrderExt, erp_sales::repository::SalesOrderExt};

use super::super::change_adapter::{
    build_purchase_change_snapshot, execute_purchase_change_domain_action, purchase_change_order_adapter,
    purchase_change_order_object_readable, purchase_change_order_subject_ref,
    purchase_change_responsible_org_id, purchase_change_start_command, require_frozen_binding,
    start_approval_command_kind, start_purchase_change_approval, RECENT_HISTORY_LIMIT,
};
use super::super::change_cancel::{
    build_purchase_change_cancel_input, load_cancel_runtime, persist_purchase_change_cancel,
    PurchaseChangeCancelPersistInput,
};
use super::super::change_start::{
    build_purchase_change_start_input, load_bound_definition_graph, load_start_receipt,
    persist_purchase_change_start, replay_purchase_change_start_with_executor, PurchaseChangeStartInput,
    PurchaseChangeStartPersistInput,
};
use super::super::dto::{
    CancelPurchaseChangeApprovalRequest, PurchaseChangeOrderView, PurchaseChangeSubmitResult,
    StartPurchaseChangeRequest, StartPurchaseChangeResult, SubmitPurchaseChangeRequest,
};
use super::super::line_input::{build_change_submission_lines, to_line_inputs};
use super::super::PurchaseOrderService;
use super::mapping::content_fingerprint;
use crate::errors::{Error, Result};
use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_identity::SharedRbacService;
use erp_workflow::service::approval::binding::{attach_published_binding, BindPublishedDefinitionCommand};
use erp_workflow::service::approval::business_adapter::BindingRevalidationContext;
use erp_workflow::service::approval::execution::{command_recovery_delay, prepare_cancel, prepare_start};
use erp_workflow::service::document_registry::{find_approval_binding, new_registered_document};

impl PurchaseOrderService {
    /// 发起采购变更（基于当前生效版本创建变更单）。
    ///
    /// 新变更单独立绑定已发布定义，不继承原采购单定义。客户端不得选择定义或审批人。
    ///
    /// # 参数
    /// * `id` - 采购单 ID
    /// * `req` - 发起请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回变更单结果。
    ///
    /// # 错误
    /// * `NotFound` - 采购单不存在
    /// * `ConflictError` - 版本不一致、已存在进行中变更或未配置审批流程
    /// * `BusinessLogicError` - 采购单未生效
    pub async fn start_change(
        &self,
        id: &str,
        req: StartPurchaseChangeRequest,
        actor: &AuditActor,
    ) -> Result<StartPurchaseChangeResult> {
        req.validate()?;
        let (order, base_revision) = self.load_changeable_order(id, req.expected_lock_version).await?;
        self.ensure_no_in_progress_change(id).await?;
        let change = PurchaseChangeOrder::new(
            PurchaseChangeOrderId::new(next_id()),
            PurchaseChangeOrderData {
                purchase_order_id: order.base.id.clone().into(),
                base_revision_id: erp_core::ids::PurchaseOrderRevisionId::new(base_revision.base.id.clone()),
                reason: req.reason.clone(),
            },
            actor.id(),
        )?;
        self.persist_started_change(&order, &change, actor).await?;
        Ok(StartPurchaseChangeResult {
            change_id: change.base.id.clone(),
            base_revision_id: base_revision.base.id.clone(),
            base_revision_no: base_revision.revision.revision_no,
            lock_version: order.base.version,
            reference: format!("CHANGE-V{}", base_revision.revision.revision_no),
        })
    }

    /// 提交采购变更并调用统一 `start_approval`。
    ///
    /// 同一事务内：锁定单据、递增 `approval_subject_version`、冻结 `subject_snapshot`、
    /// 从 `BusinessDocument` 读取绑定并启动审批。客户端不得选择定义或审批人。
    ///
    /// # 参数
    /// * `change_id` - 变更单 ID
    /// * `req` - 提交请求（目标完整头、行）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回变更提交结果。
    ///
    /// # 错误
    /// * `NotFound` - 变更单不存在
    /// * `ConflictError` - 版本不一致、无绑定或重复提交
    pub async fn submit_change(
        &self,
        change_id: &str,
        req: SubmitPurchaseChangeRequest,
        actor: &AuditActor,
    ) -> Result<PurchaseChangeSubmitResult> {
        req.validate()?;
        let adapter = purchase_change_order_adapter()?;
        let change = self
            .lock_draft_change(change_id, req.expected_lock_version)
            .await?;
        let result = self
            .start_change_approval(change_id, change, req, actor, adapter)
            .await?;
        Ok(result)
    }

    /// 提交采购变更并返回提交后的完整变更单视图。
    ///
    /// # 参数
    /// * `change_id` - 变更单 ID
    /// * `req` - 期望版本、可选目标内容与幂等键
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回写事务完成后重读的完整变更单视图。
    ///
    /// # 错误
    /// 提交事务或提交后权威视图读取失败时返回错误。
    pub async fn submit_change_view(
        &self,
        change_id: &str,
        req: SubmitPurchaseChangeRequest,
        actor: &AuditActor,
    ) -> Result<PurchaseChangeOrderView> {
        self.submit_change(change_id, req, actor).await?;
        self.change_order_detail(change_id).await
    }

    /// 撤回审批中的采购变更单，回到可修正草稿且 `subject_version` 不回退。
    ///
    /// 作为合同 `cancel_action`，供业务撤回与管理员受阻取消共用。
    ///
    /// # 参数
    /// * `id` - 变更单主键
    /// * `req` - 撤回请求（原因必填）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 撤回成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 非审批中、已最终通过、原因缺失或并发冲突时返回错误。
    pub async fn cancel_change_approval(
        &self,
        id: &str,
        req: CancelPurchaseChangeApprovalRequest,
        actor: &AuditActor,
    ) -> Result<()> {
        req.validate()?;
        let mut change = self
            .db
            .purchase_change_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购变更单不存在".to_string()))?;
        change
            .ensure_expected_version(req.expected_lock_version)
            .map_err(|_| Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()))?;
        self.persist_cancelled_change(id, &mut change, &req, actor).await
    }

    /// 加载可发起变更的采购单及其当前生效版本。
    ///
    /// # 错误
    /// 采购单不存在、版本冲突或未生效时返回错误。
    async fn load_changeable_order(
        &self,
        id: &str,
        expected_lock_version: u64,
    ) -> Result<(PurchaseOrder, PurchaseOrderRevision)> {
        let order = self
            .db
            .purchase_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购单不存在".to_string()))?;
        order
            .ensure_expected_version(expected_lock_version)
            .map_err(|_| Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()))?;
        let base_revision_id = order
            .revision_id_for_change()
            .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
        let base_revision = self
            .db
            .purchase_order_revisions()
            .find_by_id(&base_revision_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("基准版本不存在".to_string()))?;
        Ok((order, base_revision))
    }

    /// 同一采购单是否已有草稿或审批中的变更。
    ///
    /// # 错误
    /// 仓储失败或已存在进行中变更时返回错误。
    async fn ensure_no_in_progress_change(&self, purchase_order_id: &str) -> Result<()> {
        let has_in_progress = self
            .db
            .purchase_order()
            .has_in_progress_change(&purchase_order_id.to_string().into(), &mut NoTransaction)
            .await?;
        if has_in_progress {
            return Err(Error::ConflictError(
                "存在进行中的采购变更，不能重复发起".to_string(),
            ));
        }
        Ok(())
    }

    /// 为新变更单独立绑定已发布定义并写入事务。
    ///
    /// # 错误
    /// 无发布定义、人员重验失败或写入失败时返回错误。
    async fn persist_started_change(
        &self,
        order: &PurchaseOrder,
        change: &PurchaseChangeOrder,
        actor: &AuditActor,
    ) -> Result<()> {
        let sales_order = self
            .db
            .sales_orders()
            .find_by_id(&order.sales_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("来源销售单不存在".to_string()))?;
        let bind_command = BindPublishedDefinitionCommand {
            document_type: DocumentType::PurchaseChangeOrder,
            business_object_id: change.base.id.clone(),
            business_object_version: change.base.version,
            context: BindingRevalidationContext {
                organization_id: purchase_change_responsible_org_id(&sales_order)?,
                creator_id: actor.id().to_string(),
            },
        };
        let document = new_registered_document(&change.base.id, DocumentType::PurchaseChangeOrder, "")
            .map_err(crate::errors::Error::from)?;
        let audit = actor.clone().resource_log(
            "purchase_change_order.create",
            "purchase_change_order",
            change.base.id.clone(),
        )?;
        persist_created_change_order(
            &self.db,
            self.require_rbac()?,
            std::sync::Arc::clone(&self.object_read),
            change.clone(),
            document,
            bind_command,
            audit,
            actor.clone(),
        )
        .await
    }

    /// 锁定草稿变更单。
    ///
    /// # 错误
    /// 不存在、版本冲突或非草稿时返回错误。
    async fn lock_draft_change(
        &self,
        change_id: &str,
        expected_lock_version: u64,
    ) -> Result<PurchaseChangeOrder> {
        let change = self
            .db
            .purchase_change_orders()
            .find_by_id(change_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购变更单不存在".to_string()))?;
        change
            .ensure_expected_version(expected_lock_version)
            .map_err(|_| Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()))?;
        change
            .ensure_draft_for_submission()
            .map_err(|_| Error::ConflictError("变更单已提交，请勿重复提交".to_string()))?;
        Ok(change)
    }

    /// 冻结提交并启动统一审批。
    ///
    /// # 错误
    /// 无绑定、定义缺失、状态不允许或写入失败时返回错误。
    async fn start_change_approval(
        &self,
        id: &str,
        mut change: PurchaseChangeOrder,
        req: SubmitPurchaseChangeRequest,
        actor: &AuditActor,
        adapter: super::change_adapter::PurchaseChangeOrderAdapter,
    ) -> Result<PurchaseChangeSubmitResult> {
        let order = self
            .db
            .purchase_orders()
            .find_by_id(&change.purchase_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("原采购单不存在".to_string()))?;
        let sales_order = self
            .db
            .sales_orders()
            .find_by_id(&order.sales_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("来源销售单不存在".to_string()))?;
        let prepared = self
            .freeze_change_submission(&change, &order, &req, actor)
            .await?;
        start_purchase_change_approval(
            &mut change,
            prepared.submission.base.id.clone().into(),
            prepared.content_hash.clone(),
            actor.id(),
        )?;
        self.dispatch_change_start(
            ChangeStartDispatch {
                id,
                change,
                sales_order,
                prepared,
                adapter,
            },
            actor,
        )
        .await
    }

    /// 构造冻结提交与明细。
    ///
    /// # 错误
    /// 基准版本缺失或行非法时返回错误。
    async fn freeze_change_submission(
        &self,
        change: &PurchaseChangeOrder,
        order: &PurchaseOrder,
        req: &SubmitPurchaseChangeRequest,
        actor: &AuditActor,
    ) -> Result<FrozenChangeSubmission> {
        let base_revision = self
            .db
            .purchase_order_revisions()
            .find_by_id(&change.base_revision_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("基准版本不存在".to_string()))?;
        let mut normalized_request = req.clone();
        if normalized_request.lines.is_empty() {
            normalized_request.lines = self
                .change_lines_from_base_revision(&change.base_revision_id)
                .await?;
        }
        let supplier_names = database::current_legal_names_by_account_ids(
            &self.db,
            std::slice::from_ref(&order.supplier_id),
            &mut NoTransaction,
        )
        .await?;
        let supplier_name = supplier_names
            .get(&order.supplier_id.to_string())
            .cloned()
            .unwrap_or_else(|| order.supplier_id.to_string());
        let submission = self
            .build_change_submission(change, order, &base_revision, &supplier_name, &normalized_request)
            .await?;
        let enriched_lines = self
            .enrich_change_lines_with_current_sales_revision(order, &normalized_request.lines)
            .await?;
        let inputs = to_line_inputs(&enriched_lines)?;
        let lines = build_change_submission_lines(&submission.base.id.clone(), &inputs)?;
        let mut submission_mut = submission.clone();
        submission_mut.submit(Instant::now(), actor.id())?;
        Ok(FrozenChangeSubmission {
            submission: submission_mut,
            lines,
            content_hash: content_fingerprint(&normalized_request.lines),
            idempotency_key: req.idempotency_key.clone(),
        })
    }

    /// 从绑定读取定义并持久化启动事实。
    ///
    /// # 用途
    /// 加载冻结绑定并写入采购变更启动事实。
    ///
    /// # 参数
    /// * `dispatch` - 变更单、原单、提交与适配器
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回提交结果。
    ///
    /// # 错误
    /// 无绑定、定义缺失或写入失败时返回错误。
    ///
    /// # 关键业务约束
    /// 必须使用单据创建时冻结的发布定义。
    async fn dispatch_change_start(
        &self,
        dispatch: ChangeStartDispatch<'_>,
        actor: &AuditActor,
    ) -> Result<PurchaseChangeSubmitResult> {
        let ChangeStartDispatch {
            id,
            change,
            sales_order,
            prepared,
            adapter,
        } = dispatch;
        let subject = purchase_change_order_subject_ref(id)?;
        let binding = find_approval_binding(&self.db, id, &mut NoTransaction)
            .await
            .map_err(crate::errors::Error::from)?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
        let now = Instant::now();
        let snapshot = build_purchase_change_snapshot(
            &change,
            &sales_order,
            &prepared.submission,
            &prepared.lines,
            actor.id(),
            now,
        )?;
        let start = purchase_change_start_command(
            id,
            change.approval_subject_version,
            actor.id(),
            &prepared.idempotency_key,
        );
        let _ = (start_approval_command_kind(&start), RECENT_HISTORY_LIMIT);
        let organization_id = purchase_change_responsible_org_id(&sales_order)?;
        let _ = purchase_change_order_object_readable(&organization_id, actor.id())?;
        let graph = load_bound_definition_graph(&self.db, &binding).await?;
        let existing_receipt = load_start_receipt(
            &self.db,
            &subject,
            change.approval_subject_version,
            &prepared.idempotency_key,
        )
        .await?;
        let start_input = build_purchase_change_start_input(PurchaseChangeStartInput {
            graph,
            binding: &binding,
            subject,
            subject_version: change.approval_subject_version,
            actor_id: actor.id(),
            organization_id: &organization_id,
            idempotency_key: &prepared.idempotency_key,
            receipt: existing_receipt,
            now,
        })?;
        let prepared_exec = prepare_start(start_input)?;
        let audit = actor.clone().resource_log(
            "purchase_change_order.submit",
            "purchase_change_order",
            change.base.id.clone(),
        )?;
        let recovery_subject_version = change.approval_subject_version;
        let recovery_idempotency_key = prepared.idempotency_key.clone();
        let persisted = persist_purchase_change_start(
            &self.db,
            PurchaseChangeStartPersistInput {
                change_order: change.clone(),
                submission: prepared.submission.clone(),
                submission_lines: prepared.lines,
                snapshot_payload: snapshot,
                prepared: prepared_exec,
                owner_role: adapter.owner_role,
                organization_id,
                now,
                audit,
            },
        )
        .await;
        if let Err(error) = persisted {
            if !error.command_may_have_committed() {
                return Err(error);
            }
            return self
                .recover_purchase_change_start(
                    id,
                    recovery_subject_version,
                    &recovery_idempotency_key,
                    actor,
                    error,
                )
                .await;
        }
        Ok(PurchaseChangeSubmitResult {
            change_id: change.base.id.clone(),
            submission_id: prepared.submission.base.id.clone(),
            submission_no: prepared.submission.submission_no.clone(),
            status: change.stable.status.as_str().to_string(),
            lock_version: change.base.version,
            reference: format!("CS-{}", prepared.submission.submission_no),
        })
    }

    /// receipt 唯一竞争、瞬态事务或提交结果未知后，以 fresh session 有界回读。
    async fn recover_purchase_change_start(
        &self,
        change_order_id: &str,
        subject_version: u32,
        idempotency_key: &str,
        actor: &AuditActor,
        original_error: Error,
    ) -> Result<PurchaseChangeSubmitResult> {
        const RECOVERY_ATTEMPTS: usize = 8;
        for attempt in 0..RECOVERY_ATTEMPTS {
            let db = self.db.clone();
            let change_order_id = change_order_id.to_string();
            let idempotency_key = idempotency_key.to_string();
            let actor_id = actor.id().to_string();
            let recovered = self
                .db
                .client()
                .with_transaction(move |session| {
                    Box::pin(async move {
                        let change = db
                            .purchase_change_orders()
                            .find_by_id(&change_order_id, session)
                            .await?
                            .ok_or_else(|| Error::NotFound("采购变更单不存在".to_string()))?;
                        let order = db
                            .purchase_orders()
                            .find_by_id(&change.purchase_order_id, session)
                            .await?
                            .ok_or_else(|| Error::NotFound("原采购单不存在".to_string()))?;
                        let sales_order = db
                            .sales_orders()
                            .find_by_id(&order.sales_order_id, session)
                            .await?
                            .ok_or_else(|| Error::NotFound("来源销售单不存在".to_string()))?;
                        let organization_id = purchase_change_responsible_org_id(&sales_order)?;
                        let _ = purchase_change_order_object_readable(&organization_id, &actor_id)?;
                        let binding = find_approval_binding(&db, &change_order_id, session)
                            .await
                            .map_err(crate::errors::Error::from)?;
                        let binding = require_frozen_binding(binding.as_ref())?;
                        let subject = purchase_change_order_subject_ref(&change_order_id)?;
                        let Some(_) = replay_purchase_change_start_with_executor(
                            &db,
                            &subject,
                            subject_version,
                            &idempotency_key,
                            binding,
                            &actor_id,
                            session,
                        )
                        .await?
                        else {
                            return Ok(None);
                        };
                        if change.approval_subject_version != subject_version {
                            return Err(Error::ConflictError(
                                "采购变更启动结果与业务主题版本不一致".to_string(),
                            ));
                        }
                        let submission_id = change.current_submission_id.as_ref().ok_or_else(|| {
                            Error::ConflictError("采购变更启动结果缺少冻结提交".to_string())
                        })?;
                        let submission = db
                            .purchase_change_submissions()
                            .find_by_id(submission_id, session)
                            .await?
                            .ok_or_else(|| Error::ConflictError("采购变更冻结提交不存在".to_string()))?;
                        if submission.purchase_change_order_id.as_ref() != change_order_id {
                            return Err(Error::ConflictError(
                                "采购变更冻结提交与业务对象不一致".to_string(),
                            ));
                        }
                        Ok(Some(PurchaseChangeSubmitResult {
                            change_id: change.base.id.clone(),
                            submission_id: submission.base.id.clone(),
                            submission_no: submission.submission_no.clone(),
                            status: change.stable.status.as_str().to_string(),
                            lock_version: change.base.version,
                            reference: format!("CS-{}", submission.submission_no),
                        }))
                    })
                })
                .await;
            match recovered {
                Ok(Some(result)) => return Ok(result),
                Ok(None) => {}
                Err(error) if error.command_may_have_committed() => {}
                Err(error) => return Err(error),
            }
            if attempt + 1 < RECOVERY_ATTEMPTS {
                tokio::time::sleep(command_recovery_delay(attempt)).await;
            }
        }
        Err(original_error)
    }

    /// 加载撤回运行事实并写回草稿。
    ///
    /// # 错误
    /// 无绑定、实例终态或写入失败时返回错误。
    async fn persist_cancelled_change(
        &self,
        id: &str,
        change: &mut PurchaseChangeOrder,
        req: &CancelPurchaseChangeApprovalRequest,
        actor: &AuditActor,
    ) -> Result<()> {
        let adapter = purchase_change_order_adapter()?;
        let binding = find_approval_binding(&self.db, id, &mut NoTransaction)
            .await
            .map_err(crate::errors::Error::from)?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
        let subject = purchase_change_order_subject_ref(id)?;
        let runtime =
            load_cancel_runtime(&self.db, &binding, &subject, change.approval_subject_version).await?;
        let now = Instant::now();
        let input = build_purchase_change_cancel_input(
            &runtime,
            &req.reason,
            actor.id(),
            &req.idempotency_key,
            None,
            now,
        )?;
        let prepared = prepare_cancel(input)?;
        execute_purchase_change_domain_action(change, adapter.cancel_action, actor.id())?;
        let audit = actor.clone().resource_log(
            "purchase_change_order.cancel_approval",
            "purchase_change_order",
            id.to_string(),
        )?;
        persist_purchase_change_cancel(
            &self.db,
            PurchaseChangeCancelPersistInput {
                change_order: change.clone(),
                prepared,
                open_tasks: runtime.open_tasks,
                actor_id: actor.id().to_string(),
                reason: req.reason.clone(),
                now,
                audit,
            },
        )
        .await
    }
}

/// 采购变更启动所需的单据、提交与适配器。
///
/// # 用途
/// 将变更单、原采购单、来源销售单、冻结提交与适配器打包。
///
/// # 参数
/// 无
///
/// # 返回
/// 无
///
/// # 错误
/// 无
///
/// # 关键业务约束
/// 变更提交必须已冻结；原采购单仅用于来源复验。
struct ChangeStartDispatch<'a> {
    /// 变更单主键。
    id: &'a str,
    /// 已进入审批中的变更单。
    change: PurchaseChangeOrder,
    /// 来源销售单。
    sales_order: erp_sales::entity::sales_order::SalesOrder,
    /// 冻结提交。
    prepared: FrozenChangeSubmission,
    /// 采购变更审批适配器。
    adapter: super::change_adapter::PurchaseChangeOrderAdapter,
}

/// 已冻结的变更提交与指纹。
struct FrozenChangeSubmission {
    /// 已提交的不可变头。
    submission: PurchaseChangeSubmission,
    /// 提交明细。
    lines: Vec<entities::purchase_order::PurchaseChangeSubmissionLine>,
    /// 目标内容指纹。
    content_hash: String,
    /// 启动幂等键。
    idempotency_key: String,
}

/// 在创建事务内写入变更单、绑定发布定义并登记单据。
///
/// # 错误
/// 无发布定义、人员重验失败或写入失败时返回错误，调用方必须回滚。
#[allow(clippy::too_many_arguments)]
async fn persist_created_change_order(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    object_read: std::sync::Arc<dyn erp_workflow::ApprovalObjectReadPort>,
    change_order: PurchaseChangeOrder,
    mut document: BusinessDocument,
    bind_command: BindPublishedDefinitionCommand,
    audit: erp_audit::AuditLog,
    actor: AuditActor,
) -> Result<()> {
    let db = db.clone();
    let rbac = rbac.clone();
    let object_read = object_read.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                persist_bound_change_document(
                    &db,
                    &rbac,
                    object_read.as_ref(),
                    &mut document,
                    &bind_command,
                    &actor,
                    session,
                )
                .await?;
                db.purchase_change_orders().create(&change_order, session).await?;
                db.audit_logs().create(&audit, session).await?;
                Ok::<(), crate::errors::Error>(())
            })
        })
        .await
}

/// 查询发布定义、写入绑定并持久化注册行。
///
/// # 错误
/// 无发布定义或绑定失败时返回错误。
async fn persist_bound_change_document(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    object_read: &dyn erp_workflow::ApprovalObjectReadPort,
    document: &mut BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    session: &mut ClientSession,
) -> Result<()> {
    let _ = purchase_change_order_object_readable(
        &bind_command.context.organization_id,
        &bind_command.context.creator_id,
    )?;
    let binding = crate::workflow_compose::bind_published_definition_on_document_create(
        db,
        rbac,
        object_read,
        bind_command,
        actor,
        session,
    )
    .await?;
    let binding = binding.ok_or_else(|| Error::Internal("采购变更单必须绑定已发布定义".to_string()))?;
    attach_published_binding(document, binding)?;
    db.business_documents().create(document, session).await?;
    Ok(())
}
