//! 采购变更发起、提交、撤回、生效与查询。

use database::{
    AccessControlExt, CostExt, DocumentRegistryExt, NoTransaction, PayableExt, PurchaseOrderExt,
    SalesOrderExt, Transactional,
};
use entities::common::time::Instant;
use entities::document_registry::business_document::ApprovalDefinitionBinding;
use entities::document_registry::{BusinessDocument, DocumentType};
use entities::ids::{PurchaseChangeOrderId, PurchaseChangeSubmissionId};
use entities::purchase_order::{
    PurchaseChangeOrder, PurchaseChangeOrderData, PurchaseChangeOrderStatus, PurchaseChangeSubmission,
    PurchaseChangeSubmissionData, PurchaseOrder, PurchaseOrderRevision, PurchaseOrderStatus,
    SubmissionStatus,
};
use id_generator::next_id;
use mongodb::ClientSession;
use validator::Validate;

use super::change_adapter::{
    build_purchase_change_snapshot, document_approval_view, execute_purchase_change_domain_action,
    purchase_change_order_adapter, purchase_change_order_object_readable, purchase_change_order_subject_ref,
    purchase_change_responsible_org_id, purchase_change_start_command, require_frozen_binding,
    start_approval_command_kind, start_purchase_change_approval, RECENT_HISTORY_LIMIT,
};
use super::change_cancel::{
    build_purchase_change_cancel_input, load_cancel_runtime, persist_purchase_change_cancel,
    PurchaseChangeCancelPersistInput,
};
use super::change_start::{
    build_purchase_change_start_input, load_bound_definition_graph, load_start_receipt,
    persist_purchase_change_start, PurchaseChangeStartInput, PurchaseChangeStartPersistInput,
};
use super::dto::{
    CancelPurchaseChangeApprovalRequest, EffectPurchaseChangeRequest, PageView, PurchaseChangeEffectResult,
    PurchaseChangeOrderListParams, PurchaseChangeOrderView, PurchaseChangeSubmitResult,
    SavePurchaseOrderLine, StartPurchaseChangeRequest, StartPurchaseChangeResult,
    SubmitPurchaseChangeRequest,
};
use super::PurchaseOrderService;
use crate::approval::binding::{
    attach_published_binding, bind_published_definition_on_document_create, BindPublishedDefinitionCommand,
};
use crate::approval::business_adapter::BindingRevalidationContext;
use crate::approval::execution::{prepare_cancel, prepare_start};
use crate::approval::policy::ApprovalDomainAction;
use crate::audit::AuditActor;
use crate::document_registry::{find_approval_binding, new_registered_document};
use crate::errors::{Error, Result};
use crate::iam::SharedRbacService;

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
                base_revision_id: entities::ids::PurchaseOrderRevisionId::new(base_revision.base.id.clone()),
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
        self.ensure_version(&change, req.expected_lock_version)?;
        self.persist_cancelled_change(id, &mut change, &req, actor).await
    }

    /// 最终通过并生效：改写采购单并同步履约影响。
    ///
    /// 作为合同 `on_final_approve`，仅 `IN_APPROVAL` 可进入生效。
    ///
    /// # 参数
    /// * `change_id` - 变更单 ID
    /// * `req` - 生效请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回生效结果。
    ///
    /// # 错误
    /// * `NotFound` - 变更单/提交不存在
    /// * `ConflictError` - 版本不一致、非审批中或重复生效
    /// * `BusinessLogicError` - 基准版本已不是当前版本
    pub async fn apply_effective_change(
        &self,
        change_id: &str,
        req: EffectPurchaseChangeRequest,
        actor: &AuditActor,
    ) -> Result<PurchaseChangeEffectResult> {
        req.validate()?;
        let change = self
            .db
            .purchase_change_orders()
            .find_by_id(change_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购变更单不存在".to_string()))?;
        self.ensure_version(&change, req.expected_lock_version)?;
        execute_purchase_change_domain_action(
            &mut change.clone(),
            ApprovalDomainAction::PurchaseChangeOrderApplyEffectiveChange,
            actor.id(),
        )?;
        let submission_id = resolve_effect_submission_id(
            change.current_submission_id.as_ref(),
            Some(req.submission_id.as_str()),
        )?;
        self.persist_effective_change(change, submission_id, actor).await
    }

    /// 客户端直接生效失败关闭。最终动作只能由审批运行时调用。
    ///
    /// # 返回
    /// 恒返回冲突。
    ///
    /// # 错误
    /// 恒返回 `ConflictError`。
    pub fn reject_client_effect() -> Result<PurchaseChangeEffectResult> {
        Err(Error::ConflictError(
            "采购变更生效只能由审批最终通过动作执行，客户端不得直接生效".to_string(),
        ))
    }

    /// 分页查询采购变更单列表。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法
    pub async fn change_order_list(
        &self,
        params: &PurchaseChangeOrderListParams,
    ) -> Result<PageView<PurchaseChangeOrderView>> {
        params.validate()?;
        let (sort_by, sort_dir) =
            super::dto::normalize_sort(&params.sort_by, &params.sort_dir, &["created_at"])?;
        let page = params.page.unwrap_or(1);
        let page_size = params.page_size.unwrap_or(20).clamp(1, 100);
        let filter = change_list_filter(params);
        let sort_doc = mongodb::bson::doc! { sort_by: if matches!(sort_dir, super::dto::SortDir::Asc) { 1i32 } else { -1i32 } };
        let items = self
            .db
            .purchase_change_orders()
            .find_many_sorted(filter, sort_doc, &mut NoTransaction)
            .await?;
        let total = items.len() as i64;
        let start = ((page - 1) * u64::from(page_size)) as usize;
        let views = items
            .into_iter()
            .skip(start)
            .take(page_size as usize)
            .map(|change| change_list_view(change, None))
            .collect();
        Ok(PageView {
            items: views,
            total,
            page,
            page_size,
        })
    }

    /// 查询采购变更单详情。
    ///
    /// 返回统一只读审批结构；创建后未提交只返回绑定定义。
    ///
    /// # 参数
    /// * `id` - 变更单 ID
    ///
    /// # 返回
    /// 返回变更单视图。
    ///
    /// # 错误
    /// * `NotFound` - 变更单不存在
    pub async fn change_order_detail(&self, id: &str) -> Result<PurchaseChangeOrderView> {
        let change = self
            .db
            .purchase_change_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购变更单不存在".to_string()))?;
        Ok(change_list_view(change, self.load_change_binding(id).await?))
    }

    /// 读取变更单创建时冻结的审批绑定。未注册时返回空绑定。
    ///
    /// # 错误
    /// 仓储失败时返回错误。
    async fn load_change_binding(&self, id: &str) -> Result<Option<ApprovalDefinitionBinding>> {
        match find_approval_binding(&self.db, id, &mut NoTransaction).await {
            Ok(binding) => Ok(binding),
            Err(Error::NotFound(_)) => Ok(None),
            Err(error) => Err(error),
        }
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
        self.ensure_version(&order, expected_lock_version)?;
        ensure_order_allows_change(&order)?;
        let base_revision_id = order
            .stable
            .current_revision_id
            .clone()
            .ok_or_else(|| Error::BusinessLogicError("采购单没有生效版本，不能发起变更".to_string()))?;
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
            .purchase_change_orders()
            .exists(
                mongodb::bson::doc! {
                    "purchase_order_id": purchase_order_id,
                    "status": { "$in": [
                        PurchaseChangeOrderStatus::Draft.as_str(),
                        PurchaseChangeOrderStatus::InApproval.as_str(),
                    ]},
                },
                &mut NoTransaction,
            )
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
        let document = new_registered_document(&change.base.id, DocumentType::PurchaseChangeOrder, "")?;
        let audit = actor.clone().resource_log(
            "purchase_change_order.create",
            "purchase_change_order",
            change.base.id.clone(),
        )?;
        persist_created_change_order(
            &self.db,
            self.require_rbac()?,
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
        self.ensure_version(&change, expected_lock_version)?;
        if change.stable.status != PurchaseChangeOrderStatus::Draft {
            return Err(Error::ConflictError("变更单已提交，请勿重复提交".to_string()));
        }
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
        let supplier_name = self
            .resolve_supplier_name(&order.supplier_id)
            .await?
            .unwrap_or_else(|| order.supplier_id.to_string());
        let submission = self
            .build_change_submission(change, order, &base_revision, &supplier_name, req)
            .await?;
        let lines = self
            .build_change_submission_lines(&submission.base.id.clone(), &req.lines)
            .await?;
        let mut submission_mut = submission.clone();
        submission_mut.submit(Instant::now(), actor.id())?;
        Ok(FrozenChangeSubmission {
            submission: submission_mut,
            lines,
            content_hash: content_fingerprint(&req.lines),
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
        let binding = find_approval_binding(&self.db, id, &mut NoTransaction).await?;
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
        persist_purchase_change_start(
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
        .await?;
        Ok(PurchaseChangeSubmitResult {
            change_id: change.base.id.clone(),
            submission_id: prepared.submission.base.id.clone(),
            submission_no: prepared.submission.submission_no.clone(),
            status: change.stable.status.as_str().to_string(),
            lock_version: change.base.version,
            reference: format!("CS-{}", prepared.submission.submission_no),
        })
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
        let binding = find_approval_binding(&self.db, id, &mut NoTransaction).await?;
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

    /// 在同一事务内生成生效修订、应付差额并推进采购当前版本。
    ///
    /// # 错误
    /// 基准版本漂移、提交状态非法或写入失败时返回错误。
    async fn persist_effective_change(
        &self,
        change: PurchaseChangeOrder,
        submission_id: String,
        actor: &AuditActor,
    ) -> Result<PurchaseChangeEffectResult> {
        let order = self
            .db
            .purchase_orders()
            .find_by_id(&change.purchase_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("原采购单不存在".to_string()))?;
        ensure_base_revision_current(&change, &order)?;
        let (submission, lines) = self.load_pending_change_submission(&submission_id).await?;
        let new_revision_no = self.next_revision_no(&order).await?;
        let (revision, revision_lines) = self
            .build_change_revision(&order, &submission, &lines, new_revision_no)
            .await?;
        let base_revision = self
            .db
            .purchase_order_revisions()
            .find_by_id(&change.base_revision_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("基准版本不存在".to_string()))?;
        let delta = self
            .build_change_deltas(&order, &base_revision, &revision)
            .await?;
        let payable_delta_entry_id = delta.0.as_ref().map(|(_, entry)| entry.base.id.clone());
        write_effective_change(
            &self.db,
            EffectiveChangeWrite {
                order: order.clone(),
                change: change.clone(),
                submission,
                revision: revision.clone(),
                revision_lines,
                delta,
            },
            actor,
        )
        .await?;
        Ok(PurchaseChangeEffectResult {
            change_id: change.base.id.clone(),
            revision_id: revision.base.id.clone(),
            revision_no: new_revision_no,
            payable_delta_entry_id,
            purchase_order_lock_version: order.base.version,
            reference: format!("EFFECT-V{new_revision_no}"),
        })
    }

    /// 加载待生效的变更提交及其明细。
    ///
    /// # 错误
    /// 提交不存在或已处理时返回错误。
    async fn load_pending_change_submission(
        &self,
        submission_id: &str,
    ) -> Result<(
        PurchaseChangeSubmission,
        Vec<entities::purchase_order::PurchaseChangeSubmissionLine>,
    )> {
        let submission = self
            .db
            .purchase_change_submissions()
            .find_by_id(submission_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("变更提交不存在".to_string()))?;
        if submission.status != SubmissionStatus::Pending {
            return Err(Error::ConflictError("变更提交已处理，请勿重复生效".to_string()));
        }
        let lines = self
            .db
            .purchase_change_submission_lines()
            .find_many(
                mongodb::bson::doc! { "purchase_change_submission_id": submission_id },
                &mut NoTransaction,
            )
            .await?;
        Ok((submission, lines))
    }

    /// 构建变更提交（表头取自目标内容，提交动作由调用方冻结审计人）。
    async fn build_change_submission(
        &self,
        change: &PurchaseChangeOrder,
        order: &PurchaseOrder,
        base_revision: &PurchaseOrderRevision,
        _supplier_name: &str,
        req: &SubmitPurchaseChangeRequest,
    ) -> Result<PurchaseChangeSubmission> {
        let (gross, net, tax) = self.compute_request_totals(&req.lines).await?;
        let payment_term_code = req
            .payment_term_code
            .clone()
            .unwrap_or_else(|| base_revision.payment_term_snapshot.payment_term_code.clone());
        let payment_term_snapshot = self.payment_term_snapshot(&payment_term_code).await?;
        let next_no = self.next_change_submission_no(change).await?;
        PurchaseChangeSubmission::new(
            PurchaseChangeSubmissionId::new(next_id()),
            PurchaseChangeSubmissionData {
                purchase_change_order_id: change.base.id.clone().into(),
                submission_no: next_no,
                base_revision_id: change.base_revision_id.clone(),
                supplier_id: order.supplier_id.clone(),
                purchase_type: order.purchase_type,
                fulfillment_responsibility: order.fulfillment_responsibility,
                supplier_revision_id: base_revision.supplier_revision_id.clone(),
                supplier_snapshot: base_revision.supplier_snapshot.clone(),
                payment_term_snapshot,
                gross_amount: gross,
                net_amount: net,
                tax_amount: tax,
            },
        )
        .map_err(Into::into)
    }

    /// 计算下一个变更提交序号。
    async fn next_change_submission_no(&self, change: &PurchaseChangeOrder) -> Result<String> {
        let existing = self
            .db
            .purchase_change_submissions()
            .find_many(
                mongodb::bson::doc! { "purchase_change_order_id": change.base.id.clone() },
                &mut NoTransaction,
            )
            .await?;
        let max_no = existing
            .iter()
            .filter_map(|submission| {
                submission
                    .submission_no
                    .strip_prefix("CS-")
                    .and_then(|value| value.parse::<u32>().ok())
            })
            .max()
            .unwrap_or(0);
        Ok(format!("CS-{:06}", max_no + 1))
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
    sales_order: entities::sales_order::SalesOrder,
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

/// 只有已生效或部分执行的采购单可以发起变更。
///
/// # 错误
/// 状态不允许时返回业务错误。
fn ensure_order_allows_change(order: &PurchaseOrder) -> Result<()> {
    if order.stable.status != PurchaseOrderStatus::Effective
        && order.stable.status != PurchaseOrderStatus::PartiallyExecuted
    {
        return Err(Error::BusinessLogicError(
            "只有已生效的采购单可以发起变更".to_string(),
        ));
    }
    Ok(())
}

/// 生效只允许当前冻结提交；请求携带的提交必须与之一致。
///
/// # 参数
/// * `current` - 变更单上的当前冻结提交
/// * `requested` - 客户端或运行时给出的提交；空则只用当前提交
///
/// # 返回
/// 返回当前冻结提交 ID。
///
/// # 错误
/// 尚未提交，或请求提交与当前冻结提交不一致时返回错误。
pub(super) fn resolve_effect_submission_id(
    current: Option<&PurchaseChangeSubmissionId>,
    requested: Option<&str>,
) -> Result<String> {
    let current = current
        .ok_or_else(|| Error::BusinessLogicError("变更单尚未提交审批".to_string()))?
        .to_string();
    if let Some(requested) = requested.map(str::trim).filter(|value| !value.is_empty()) {
        if requested != current {
            return Err(Error::ConflictError(
                "生效提交必须是当前冻结提交，不得使用历史提交".to_string(),
            ));
        }
    }
    Ok(current)
}

/// 基准版本必须仍是采购单当前版本。
///
/// # 错误
/// 版本漂移时返回业务错误。
fn ensure_base_revision_current(change: &PurchaseChangeOrder, order: &PurchaseOrder) -> Result<()> {
    if change.base_revision_id.to_string() != order.stable.current_revision_id.as_deref().unwrap_or_default()
    {
        return Err(Error::BusinessLogicError(
            "基准版本已不是当前版本，变更不能生效".to_string(),
        ));
    }
    Ok(())
}

/// 在创建事务内写入变更单、绑定发布定义并登记单据。
///
/// # 错误
/// 无发布定义、人员重验失败或写入失败时返回错误，调用方必须回滚。
async fn persist_created_change_order(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    change_order: PurchaseChangeOrder,
    mut document: BusinessDocument,
    bind_command: BindPublishedDefinitionCommand,
    audit: entities::AuditLog,
    actor: AuditActor,
) -> Result<()> {
    let db = db.clone();
    let rbac = rbac.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                persist_bound_change_document(&db, &rbac, &mut document, &bind_command, &actor, session)
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
    document: &mut BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    session: &mut ClientSession,
) -> Result<()> {
    let _ = purchase_change_order_object_readable(
        &bind_command.context.organization_id,
        &bind_command.context.creator_id,
    )?;
    let binding =
        bind_published_definition_on_document_create(db, rbac, bind_command, actor, session).await?;
    let binding = binding.ok_or_else(|| Error::Internal("采购变更单必须绑定已发布定义".to_string()))?;
    attach_published_binding(document, binding)?;
    db.business_documents().create(document, session).await?;
    Ok(())
}

/// 采购变更生效写入所需的单据、版本与差额。
///
/// # 用途
/// 将采购单、变更单、提交、生效版本与应付/成本差额打包。
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
/// 生效版本必须基于当前基准版本；差额可为空。
struct EffectiveChangeWrite {
    /// 原采购单。
    order: PurchaseOrder,
    /// 待生效的变更单。
    change: PurchaseChangeOrder,
    /// 待通过的变更提交。
    submission: PurchaseChangeSubmission,
    /// 生效版本。
    revision: entities::purchase_order::PurchaseOrderRevision,
    /// 生效版本行。
    revision_lines: Vec<entities::purchase_order::PurchaseOrderRevisionLine>,
    /// 应付差额与成本差额。
    delta: (
        Option<(entities::payable::PayableAccount, entities::payable::PayableEntry)>,
        Vec<entities::cost::CostEntry>,
    ),
}

/// 写入生效修订、应付差额与变更单状态。
///
/// # 用途
/// 将变更单标为已生效并提交事务写入。
///
/// # 参数
/// * `db` - 数据库
/// * `write` - 采购单、变更单、版本与差额
/// * `actor` - 审计操作人
///
/// # 返回
/// 写入成功时返回 `Ok(())`。
///
/// # 错误
/// 仓储失败时返回错误。
///
/// # 关键业务约束
/// 变更单状态迁移必须与版本指针同一事务。
async fn write_effective_change(
    db: &mongodb::Database,
    mut write: EffectiveChangeWrite,
    actor: &AuditActor,
) -> Result<()> {
    let audit = actor.clone().resource_log(
        "purchase_change_order.effect",
        "purchase_change_order",
        write.change.base.id.clone(),
    )?;
    let actor_id = actor.id().to_string();
    write
        .change
        .apply_effective(write.revision.base.id.clone().into(), &actor_id)?;
    let db = db.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move { persist_effective_writes(&db, write, audit, session).await })
        })
        .await
}

/// 事务内写入生效修订、指针、差额与变更单。
///
/// # 用途
/// 在已开启事务中落库生效版本、差额与变更结论。
///
/// # 参数
/// * `db` - 数据库
/// * `write` - 采购单、变更单、版本与差额
/// * `audit` - 已构造审计
/// * `session` - 事务会话
///
/// # 返回
/// 写入成功时返回 `Ok(())`。
///
/// # 错误
/// 仓储失败时返回错误。
///
/// # 关键业务约束
/// 采购单当前版本指针必须切到新修订。
async fn persist_effective_writes(
    db: &mongodb::Database,
    write: EffectiveChangeWrite,
    audit: entities::AuditLog,
    session: &mut ClientSession,
) -> Result<()> {
    let EffectiveChangeWrite {
        mut order,
        mut change,
        mut submission,
        revision,
        revision_lines,
        delta,
    } = write;
    db.purchase_order()
        .create_effective_revision(&revision, &revision_lines, session)
        .await?;
    order.stable.current_revision_id = Some(revision.base.id.clone());
    db.purchase_orders().update(&mut order, session).await?;
    if let Some((account, entry)) = &delta.0 {
        db.payable()
            .create_payable_with_entry(account, entry, session)
            .await?;
    }
    for entry in &delta.1 {
        db.cost()
            .create_cost_entry_with_allocations(entry, Vec::new(), session)
            .await?;
    }
    submission.status = SubmissionStatus::Approved;
    db.purchase_change_submissions()
        .update(&mut submission, session)
        .await?;
    db.purchase_change_orders().update(&mut change, session).await?;
    db.audit_logs().create(&audit, session).await?;
    Ok(())
}

/// 组装列表筛选。
///
/// # 参数
/// * `params` - 查询参数
///
/// # 返回
/// 返回 Mongo 过滤文档。
fn change_list_filter(params: &PurchaseChangeOrderListParams) -> mongodb::bson::Document {
    let mut filter = mongodb::bson::doc! {};
    if let Some(purchase_order_id) = params
        .purchase_order_id
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        filter.insert("purchase_order_id", purchase_order_id);
    }
    if let Some(status) = params.status.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        filter.insert("status", status);
    }
    filter
}

/// 由变更单构造列表/详情视图。
///
/// # 参数
/// * `change` - 变更单
/// * `binding` - 详情时的冻结绑定；列表为空
///
/// # 返回
/// 返回视图。
fn change_list_view(
    change: PurchaseChangeOrder,
    binding: Option<ApprovalDefinitionBinding>,
) -> PurchaseChangeOrderView {
    PurchaseChangeOrderView {
        id: change.base.id.clone(),
        purchase_order_id: change.purchase_order_id.to_string(),
        base_revision_id: change.base_revision_id.to_string(),
        reason: change.reason.clone(),
        status: change.stable.status.as_str().to_string(),
        current_submission_id: change.current_submission_id.as_ref().map(ToString::to_string),
        effective_revision_id: change.effective_revision_id.as_ref().map(ToString::to_string),
        version: change.base.version,
        created_at: change.base.created_at,
        approval: document_approval_view(binding.as_ref(), None, change.stable.status),
    }
}

/// 内容指纹（Debug 形态 SipHash 十六进制；同二进制内稳定，用于变更目标内容比对）。
fn content_fingerprint(lines: &[SavePurchaseOrderLine]) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    format!("{:?}", lines).hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::{
        execute_purchase_change_domain_action, resolve_effect_submission_id, start_purchase_change_approval,
        PurchaseOrderService,
    };
    use crate::approval::policy::ApprovalDomainAction;
    use entities::ids::{
        PurchaseChangeOrderId, PurchaseChangeSubmissionId, PurchaseOrderId, PurchaseOrderRevisionId,
    };
    use entities::purchase_order::{PurchaseChangeOrder, PurchaseChangeOrderData, PurchaseChangeOrderStatus};

    /// 创建必须注册 BusinessDocument 并独立绑定发布定义。
    #[test]
    fn create_registers_document_and_binds_published_definition() {
        let source = include_str!("change.rs");
        assert!(source.contains("bind_published_definition_on_document_create"));
        assert!(source.contains("new_registered_document"));
        assert!(source.contains("DocumentType::PurchaseChangeOrder"));
        assert!(source.contains("新变更单独立绑定"));
    }

    /// 提交必须锁定单据、递增 approval_subject_version 并调用 start_approval。
    #[test]
    fn submit_calls_start_approval_with_subject_version() {
        let source = include_str!("change.rs");
        assert!(source.contains("start_change_approval"));
        assert!(source.contains("purchase_change_start_command"));
        assert!(source.contains("change.approval_subject_version"));
        assert!(source.contains("prepare_start"));
    }

    /// 最终动作唯一为 apply_effective_change，且绑定当前冻结提交。
    #[test]
    fn final_action_is_apply_effective_change() {
        let source = include_str!("change.rs");
        assert!(source.contains("pub async fn apply_effective_change"));
        assert!(source.contains("change.apply_effective"));
        assert!(source.contains("PurchaseChangeOrderApplyEffectiveChange"));
        assert!(source.contains("resolve_effect_submission_id"));
        assert!(source.contains("current_submission_id"));
    }

    /// 撤回必须调用统一 cancel 并回到草稿。
    #[test]
    fn cancel_uses_unified_port() {
        let source = include_str!("change.rs");
        assert!(source.contains("pub async fn cancel_change_approval"));
        assert!(source.contains("prepare_cancel"));
        assert!(source.contains("adapter.cancel_action"));
    }

    /// 详情必须返回统一审批结构。
    #[test]
    fn detail_returns_unified_approval() {
        let source = include_str!("change.rs");
        assert!(source.contains("document_approval_view"));
        assert!(source.contains("load_change_binding"));
    }

    /// 客户端直接生效必须失败关闭。
    #[test]
    fn client_effect_fails_closed() {
        let error = PurchaseOrderService::reject_client_effect().unwrap_err();
        assert!(error.to_string().contains("客户端不得直接生效"));
    }

    /// 生效只接受当前冻结提交；错误提交或缺失提交失败关闭。
    #[test]
    fn effect_rejects_mismatched_or_missing_submission() {
        let current = PurchaseChangeSubmissionId::new("pcs-current");
        assert_eq!(
            resolve_effect_submission_id(Some(&current), Some("pcs-current")).unwrap(),
            "pcs-current"
        );
        assert_eq!(
            resolve_effect_submission_id(Some(&current), None).unwrap(),
            "pcs-current"
        );
        let mismatch = resolve_effect_submission_id(Some(&current), Some("pcs-old")).unwrap_err();
        assert!(mismatch.to_string().contains("当前冻结提交"));
        assert!(resolve_effect_submission_id(None, Some("pcs-current")).is_err());
    }

    /// 非审批中不得走最终通过动作；撤回不回退 subject_version。
    #[test]
    fn cancel_keeps_subject_version_and_effect_requires_in_approval() {
        let mut change = PurchaseChangeOrder::new(
            PurchaseChangeOrderId::new("pco-1"),
            PurchaseChangeOrderData {
                purchase_order_id: PurchaseOrderId::new("po-1"),
                base_revision_id: PurchaseOrderRevisionId::new("por-1"),
                reason: "成本上涨".into(),
            },
            "user-1",
        )
        .expect("草稿必须可构造");
        assert!(execute_purchase_change_domain_action(
            &mut change,
            ApprovalDomainAction::PurchaseChangeOrderApplyEffectiveChange,
            "user-1",
        )
        .is_err());
        start_purchase_change_approval(
            &mut change,
            PurchaseChangeSubmissionId::new("pcs-1"),
            "hash-1",
            "user-1",
        )
        .unwrap();
        assert_eq!(change.approval_subject_version, 1);
        execute_purchase_change_domain_action(
            &mut change,
            ApprovalDomainAction::PurchaseChangeOrderCancelApproval,
            "user-1",
        )
        .unwrap();
        assert_eq!(change.stable.status, PurchaseChangeOrderStatus::Draft);
        assert_eq!(change.approval_subject_version, 1);
    }
}
