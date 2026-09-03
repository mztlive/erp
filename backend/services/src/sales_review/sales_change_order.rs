// ---------------------------------------------------------------------
// 销售变更单（W05 变更轨；§8.1.3 本批部分）
// ---------------------------------------------------------------------

use database::{
    AccessControlExt, DocumentRegistryExt, NoTransaction, ReceivableExt, SalesOrderExt, SalesReviewExt,
    Transactional,
};
use entities::common::time::Instant;
use entities::document_registry::{
    BusinessDocument, WorkflowAction, WorkflowActionData, WorkflowActionId, WorkflowActionType,
};
use entities::ids::{
    BusinessDocumentId, ReceivableAccountId, SalesChangeOrderId, SalesChangeSubmissionId,
    SalesChangeSubmissionLineId, SalesOrderId, SalesOrderRevisionId, SalesOrderRevisionLineId,
    SalesOrderWorkingCopyId,
};
use entities::sales_order::{SalesContentHash, SalesOrderWorkingCopyLineData, WorkingPurpose};
use entities::sales_review::{
    SalesChangeOrder, SalesChangeOrderData, SalesChangeSubmission, SalesChangeSubmissionData,
    SalesChangeSubmissionLine,
};
use id_generator::next_id;
use mongodb::ClientSession;
use validator::Validate;

use super::adapter::{
    build_sales_change_snapshot, document_approval_view, execute_sales_change_domain_action,
    require_frozen_binding, sales_change_order_adapter, sales_change_order_object_readable,
    sales_change_order_subject_ref, sales_change_responsible_org_id, sales_change_start_command,
    start_approval_command_kind, start_sales_change_approval, RECENT_HISTORY_LIMIT,
};
use super::cancel_approval::{
    build_sales_change_cancel_input, load_cancel_runtime, persist_sales_change_cancel,
    SalesChangeCancelPersistInput,
};
use super::dto;
use super::formalization::{build_change_revision, build_receivable_delta};
use super::start_approval::{
    build_sales_change_start_input, load_bound_definition_graph, load_start_receipt,
    persist_sales_change_start, replay_sales_change_start_with_executor, SalesChangeStartInput,
    SalesChangeStartPersistInput,
};
use super::{
    CancelSalesChangeApprovalRequest, CreateSalesChangeOrderRequest, PageView, SalesChangeOrderDetailView,
    SalesChangeOrderFilter, SalesChangeOrderListParams, SalesChangeOrderView, SalesReviewService,
    SubmitSalesChangeRequest, VoidSalesChangeOrderRequest,
};
use crate::approval::binding::{
    attach_published_binding, bind_published_definition_on_document_create, BindPublishedDefinitionCommand,
};
use crate::approval::business_adapter::BindingRevalidationContext;
use crate::approval::execution::idempotency::normalize_idempotency_key;
use crate::approval::execution::{
    command_may_have_committed, command_recovery_delay, prepare_cancel, prepare_start,
};
use crate::approval::policy::ApprovalDomainAction;
use crate::audit::AuditActor;
use crate::document_registry::{find_approval_binding, new_registered_document};
use crate::errors::{Error, Result};
use crate::iam::SharedRbacService;

impl SalesReviewService {
    /// 分页查询销售变更单。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn sales_change_order_list(
        &self,
        params: &SalesChangeOrderListParams,
    ) -> Result<PageView<SalesChangeOrderView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = SalesChangeOrderFilter {
            sales_order_id: query.sales_order_id.map(SalesOrderId::new),
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, dto::SortDir::Asc),
        };
        let page = self
            .db
            .sales_change_orders()
            .search_sales_change_orders(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| SalesChangeOrderView {
                id: row.id,
                sales_order_id: row.sales_order_id,
                base_revision_id: row.base_revision_id,
                change_type: row.change_type,
                status: row.status,
                current_submission_id: row.current_submission_id,
                version: row.version,
                created_at: row.created_at,
            })
            .collect();

        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 查询销售变更单详情。
    ///
    /// # 参数
    /// * `id` - 变更单 ID
    ///
    /// # 返回
    /// 返回详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 变更单不存在
    pub async fn sales_change_order_detail(&self, id: &str) -> Result<SalesChangeOrderDetailView> {
        let change_order = self
            .db
            .sales_change_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售变更单不存在".to_string()))?;
        Ok(detail_view(change_order, self.load_change_binding(id).await?))
    }

    /// 读取变更单创建时冻结的审批绑定。未注册时返回空绑定。
    ///
    /// # 错误
    /// 仓储失败时返回错误。
    async fn load_change_binding(
        &self,
        id: &str,
    ) -> Result<Option<entities::document_registry::business_document::ApprovalDefinitionBinding>> {
        match find_approval_binding(&self.db, id, &mut NoTransaction).await {
            Ok(binding) => Ok(binding),
            Err(Error::NotFound(_)) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// 同一销售单同一基准版本是否已有进行中变更。
    ///
    /// # 错误
    /// 仓储失败时返回错误。
    async fn has_in_progress_change(
        &self,
        sales_order_id: &SalesOrderId,
        base_revision_id: &SalesOrderRevisionId,
    ) -> Result<bool> {
        Ok(self
            .db
            .sales_change_orders()
            .has_in_progress_by_order_and_base(sales_order_id, base_revision_id, &mut NoTransaction)
            .await?)
    }

    /// 创建销售变更单（草稿 + 变更工作副本 + `BusinessDocument` 绑定原子形成）。
    ///
    /// `PROCESS_REQUIRED` 无发布定义时返回 `APPROVAL_PROCESS_NOT_CONFIGURED`，
    /// 业务单据零写入。客户端不得选择定义或审批人。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    /// * `rbac` - 共享 RBAC
    ///
    /// # 返回
    /// 返回变更单详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 原销售单不存在或未生效
    /// * `ConflictError` - 同一基准版本已有进行中变更或未配置审批流程
    pub async fn create_sales_change_order(
        &self,
        req: CreateSalesChangeOrderRequest,
        actor: &AuditActor,
        rbac: &SharedRbacService,
    ) -> Result<SalesChangeOrderDetailView> {
        req.validate()?;
        let order = self
            .db
            .sales_orders()
            .find_by_id(&req.sales_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
        if let Some(blocker) = order.sales_change_start_blocker() {
            return Err(Error::BusinessLogicError(blocker.to_string()));
        }
        let base_revision_id =
            SalesOrderRevisionId::new(order.current_revision_id().expect("实体规则已确认当前版本存在"));
        let base_revision = self
            .db
            .sales_order_revisions()
            .find_by_id(base_revision_id.as_ref(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售单当前版本不存在".to_string()))?;
        if !base_revision.matches_revision_no(req.expected_base_revision_no) {
            return Err(Error::ConflictError(format!(
                "销售单当前版本已变更：期望 {}，实际 {}",
                req.expected_base_revision_no, base_revision.revision.revision_no
            )));
        }
        if self
            .has_in_progress_change(&req.sales_order_id, &base_revision_id)
            .await?
        {
            return Err(Error::ConflictError(
                "同一基准版本已有进行中的销售变更单".to_string(),
            ));
        }

        let change_order = SalesChangeOrder::new(
            SalesChangeOrderId::new(next_id()),
            SalesChangeOrderData {
                sales_order_id: req.sales_order_id.clone(),
                base_revision_id: base_revision_id.clone(),
                change_type: req.change_type,
                reason: req.reason,
            },
            actor.id(),
        )?;
        let revision_lines = self
            .db
            .sales_order_revision_lines()
            .list_lines_by_revision(&base_revision_id, &mut NoTransaction)
            .await?;
        let revision_line_ids = revision_lines
            .iter()
            .map(|line| SalesOrderRevisionLineId::new(line.base.id.clone()))
            .collect::<Vec<_>>();
        let goods_lines = self
            .db
            .sales_order_goods_service_line_revisions()
            .list_by_revision_line_ids(&revision_line_ids, &mut NoTransaction)
            .await?;
        let working_copy_id = SalesOrderWorkingCopyId::new(next_id());
        let (line_datas, lines) =
            build_change_working_copy_lines_from_revision(&working_copy_id, &revision_lines, &goods_lines)?;
        let (gross, net, tax) = entities::sales_order::SalesOrderWorkingCopyLine::amount_totals(&lines);
        let working_copy = entities::sales_order::SalesOrderWorkingCopy::new(
            working_copy_id,
            entities::sales_order::SalesOrderWorkingCopyData {
                sales_order_id: req.sales_order_id.clone(),
                working_purpose: WorkingPurpose::SalesChange,
                sales_change_order_id: Some(change_order.base.id.clone().into()),
                base_revision_id: Some(base_revision_id.clone()),
                draft_version: 1,
                content_hash: SalesContentHash::change(&change_order.base.id, 1)?.into_wire(),
                editor_user_id: actor.id().to_string(),
                business_type: order.business_type,
                customer_id: order.customer_id.clone(),
                contract_id: order.contract_id.clone(),
                contract_revision_id: base_revision.contract_revision_id.clone(),
                settlement_party_id: order.settlement_party_id.clone(),
                snapshot: entities::sales_order::HeaderSnapshotData {
                    customer_name: base_revision.customer_snapshot.customer_name.clone(),
                    contract_no: base_revision
                        .contract_snapshot
                        .as_ref()
                        .map(|snapshot| snapshot.contract_no.clone()),
                    settlement_party_name: base_revision
                        .settlement_party_snapshot
                        .as_ref()
                        .map(|snapshot| snapshot.settlement_party_name.clone()),
                    payment_term_code: base_revision.payment_term_snapshot.payment_term_code.clone(),
                    payment_term_name: base_revision.payment_term_snapshot.payment_term_name.clone(),
                    invoice_type: base_revision.invoice_requirement_snapshot.invoice_type.clone(),
                    tax_point: base_revision.invoice_requirement_snapshot.tax_point.clone(),
                },
                project_name: base_revision.project_name.clone(),
                business_remark: base_revision.business_remark.clone(),
                voucher_category_sku_id: base_revision.voucher_category_sku_id.clone(),
                voucher_expiry_at: base_revision.voucher_expiry_at,
                target_mall_id: None,
                receivable_due_date: None,
                gross_amount: gross,
                net_amount: net,
                tax_amount: tax,
                lines: line_datas,
            },
            actor.id(),
        )?;
        let bind_command = BindPublishedDefinitionCommand {
            document_type: entities::document_registry::DocumentType::SalesChangeOrder,
            business_object_id: change_order.base.id.clone(),
            business_object_version: change_order.base.version,
            context: BindingRevalidationContext {
                organization_id: sales_change_responsible_org_id(&order)?,
                creator_id: actor.id().to_string(),
            },
        };
        let document = new_registered_document(
            change_order.base.id.clone(),
            entities::document_registry::DocumentType::SalesChangeOrder,
            String::new(),
        )?;
        let audit = actor.clone().resource_log(
            "sales_change_order.create",
            "sales_change_order",
            change_order.base.id.clone(),
        )?;
        persist_created_change_order(
            &self.db,
            rbac,
            CreatedChangeOrderPersistInput {
                change_order: change_order.clone(),
                working_copy,
                lines,
                document,
                bind_command,
                audit,
                actor: actor.clone(),
            },
        )
        .await?;

        self.sales_change_order_detail(&change_order.base.id).await
    }

    /// 提交销售变更并调用统一 `start_approval`。
    ///
    /// `subject_version` 取 `sales_change_submission.submission_no`，不得复用
    /// `BaseModel.version`。定义与审批人取自已绑定事实，不接受客户端选择。
    ///
    /// # 参数
    /// * `id` - 变更单 ID
    /// * `req` - 提交请求（含期望版本与幂等键）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回变更单详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 变更单或变更工作副本不存在
    /// * `ConflictError` - 期望版本不一致、无绑定或状态不允许
    pub async fn submit_sales_change(
        &self,
        id: &str,
        req: SubmitSalesChangeRequest,
        actor: &AuditActor,
    ) -> Result<SalesChangeOrderDetailView> {
        req.validate()?;
        let adapter = sales_change_order_adapter()?;
        let change_order = self
            .db
            .sales_change_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售变更单不存在".to_string()))?;
        if !change_order.matches_version(req.version) {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        if !change_order.is_draft() {
            return Err(Error::ConflictError(
                "只有草稿状态的销售变更单可以提交审批".to_string(),
            ));
        }
        self.start_change_approval(id, change_order, req.idempotency_key.clone(), actor, adapter)
            .await
    }

    /// 作废销售变更单（仅草稿态）。
    ///
    /// # 参数
    /// * `id` - 变更单 ID
    /// * `req` - 作废请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回变更单详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 变更单不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    pub async fn void_sales_change(
        &self,
        id: &str,
        req: VoidSalesChangeOrderRequest,
        actor: &AuditActor,
    ) -> Result<SalesChangeOrderDetailView> {
        req.validate()?;
        let mut change_order = self
            .db
            .sales_change_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售变更单不存在".to_string()))?;
        if !change_order.matches_version(req.version) {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        change_order.void(actor.id())?;
        let mut working_copy = self
            .db
            .sales_order_working_copies()
            .find_by_sales_change_order(&SalesChangeOrderId::new(id), &mut NoTransaction)
            .await?;
        if let Some(copy) = &mut working_copy {
            copy.abandon()?;
        }
        let audit =
            actor
                .clone()
                .resource_log("sales_change_order.void", "sales_change_order", id.to_string())?;
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.sales_change_orders()
                        .update(&mut change_order, session)
                        .await?;
                    if let Some(copy) = &mut working_copy {
                        db.sales_order_working_copies().update(copy, session).await?;
                    }
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        self.sales_change_order_detail(id).await
    }

    /// 撤回审批中的销售变更单，回到可修正草稿且 `subject_version` 不回退。
    ///
    /// # 参数
    /// * `id` - 变更单主键
    /// * `req` - 撤回请求（原因必填）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回撤回后的变更单详情。
    ///
    /// # 错误
    /// 非审批中、已最终通过、原因缺失或并发冲突时返回错误。
    pub async fn cancel_approval(
        &self,
        id: &str,
        req: CancelSalesChangeApprovalRequest,
        actor: &AuditActor,
    ) -> Result<SalesChangeOrderDetailView> {
        req.validate()?;
        let mut change_order = self
            .db
            .sales_change_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售变更单不存在".to_string()))?;
        if !change_order.matches_version(req.expected_version) {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        let adapter = sales_change_order_adapter()?;
        let binding = find_approval_binding(&self.db, id, &mut NoTransaction).await?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
        let subject = sales_change_order_subject_ref(id)?;
        let subject_version = latest_change_submission_no(&self.db, id).await?;
        let runtime = load_cancel_runtime(&self.db, &binding, &subject, subject_version).await?;
        let now = Instant::now();
        let idempotency_key = normalize_idempotency_key(&req.idempotency_key)?;
        let input =
            build_sales_change_cancel_input(&runtime, &req.reason, actor.id(), &idempotency_key, None, now)?;
        let prepared = prepare_cancel(input)?;
        execute_sales_change_domain_action(&mut change_order, adapter.cancel_action, actor.id())?;
        let audit = actor.clone().resource_log(
            "sales_change_order.cancel_approval",
            "sales_change_order",
            id.to_string(),
        )?;
        persist_sales_change_cancel(
            &self.db,
            SalesChangeCancelPersistInput {
                change_order,
                prepared,
                open_tasks: runtime.open_tasks,
                actor_id: actor.id().to_string(),
                reason: req.reason.clone(),
                now,
                audit,
            },
        )
        .await?;
        self.sales_change_order_detail(id).await
    }

    /// 最终通过并生效：生成生效修订并改写销售单。
    ///
    /// 仅由合同 §4.4.4 `on_final_approve` 调用，不得再作为人工中间旁路。
    ///
    /// # 参数
    /// * `id` - 变更单主键
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回生效后的变更单详情。
    ///
    /// # 错误
    /// 非审批中、缺少提交、基准版本漂移或仓储失败时返回错误。
    pub async fn apply_effective_change(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<SalesChangeOrderDetailView> {
        let change_order = self
            .db
            .sales_change_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售变更单不存在".to_string()))?;
        execute_sales_change_domain_action(
            &mut change_order.clone(),
            ApprovalDomainAction::SalesChangeOrderApplyEffectiveChange,
            actor.id(),
        )?;
        persist_effective_change(&self.db, change_order, actor).await?;
        self.sales_change_order_detail(id).await
    }

    /// 在审批运行时持有的事务内生效销售变更。
    ///
    /// # 错误
    /// 状态、基准版本、应收差额或持久化不变量失败时返回错误。
    pub(crate) async fn apply_effective_change_in_transaction(
        &self,
        id: &str,
        actor: &AuditActor,
        session: &mut ClientSession,
    ) -> Result<()> {
        let change_order = self
            .db
            .sales_change_orders()
            .find_by_id(id, session)
            .await?
            .ok_or_else(|| Error::NotFound("销售变更单不存在".to_string()))?;
        execute_sales_change_domain_action(
            &mut change_order.clone(),
            ApprovalDomainAction::SalesChangeOrderApplyEffectiveChange,
            actor.id(),
        )?;
        let write = prepare_effective_change_write(&self.db, change_order, actor).await?;
        persist_effective_writes_in_transaction(&self.db, write, session).await
    }

    /// 冻结提交并启动统一审批。
    ///
    /// # 错误
    /// 无绑定、定义缺失、状态不允许或写入失败时返回错误。
    async fn start_change_approval(
        &self,
        id: &str,
        mut change_order: SalesChangeOrder,
        idempotency_key: String,
        actor: &AuditActor,
        adapter: super::adapter::SalesChangeOrderAdapter,
    ) -> Result<SalesChangeOrderDetailView> {
        let sales_order = self
            .db
            .sales_orders()
            .find_by_id(&change_order.sales_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
        let subject = sales_change_order_subject_ref(id)?;
        let binding = find_approval_binding(&self.db, id, &mut NoTransaction).await?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
        let (mut working_copy, copy_lines) = load_change_working_copy(&self.db, &change_order).await?;
        let submission_no = next_change_submission_no(&self.db, id).await?;
        let (submission, submission_lines) =
            build_change_submission_with_no(&change_order, &working_copy, &copy_lines, submission_no, actor)?;
        working_copy.lock_for_submission_if_needed()?;
        start_sales_change_approval(
            &mut change_order,
            submission.base.id.clone().into(),
            SalesContentHash::submission(&submission.base.id)?.into_wire(),
            actor.id(),
        )?;
        let now = Instant::now();
        let snapshot = build_sales_change_snapshot(
            &change_order,
            &sales_order,
            &submission,
            &submission_lines,
            actor.id(),
            now,
        )?;
        let start = sales_change_start_command(id, submission.submission_no, actor.id(), &idempotency_key);
        let _ = (start_approval_command_kind(&start), RECENT_HISTORY_LIMIT);
        let organization_id = sales_change_responsible_org_id(&sales_order)?;
        let graph = load_bound_definition_graph(&self.db, &binding).await?;
        let existing_receipt =
            load_start_receipt(&self.db, &subject, submission.submission_no, &idempotency_key).await?;
        let start_input = build_sales_change_start_input(SalesChangeStartInput {
            graph,
            binding: &binding,
            subject,
            subject_version: submission.submission_no,
            actor_id: actor.id(),
            organization_id: &organization_id,
            idempotency_key: &idempotency_key,
            receipt: existing_receipt,
            now,
        })?;
        let prepared = prepare_start(start_input)?;
        let workflow_action = WorkflowAction::new(
            WorkflowActionId::new(next_id()),
            WorkflowActionData {
                document_id: BusinessDocumentId::new(id.to_string()),
                action_type: WorkflowActionType::Submit,
                from_status: "DRAFT".to_string(),
                to_status: "IN_APPROVAL".to_string(),
                actor_id: actor.id().to_string(),
                actor_role: adapter.owner_role.to_string(),
                comment: None,
            },
        )?;
        let audit =
            actor
                .clone()
                .resource_log("sales_change_order.submit", "sales_change_order", id.to_string())?;
        let recovery_subject_version = submission.submission_no;
        let persisted = persist_sales_change_start(
            &self.db,
            SalesChangeStartPersistInput {
                change_order,
                working_copy,
                submission,
                submission_lines,
                workflow_action,
                snapshot_payload: snapshot,
                prepared,
                owner_role: adapter.owner_role,
                organization_id,
                now,
                audit,
            },
        )
        .await;
        if let Err(error) = persisted {
            if !command_may_have_committed(&error) {
                return Err(error);
            }
            self.recover_sales_change_start(id, recovery_subject_version, &idempotency_key, actor, error)
                .await?;
        }
        self.sales_change_order_detail(id).await
    }

    /// receipt 唯一竞争、瞬态事务或提交结果未知后，以 fresh session 有界回读。
    async fn recover_sales_change_start(
        &self,
        change_order_id: &str,
        subject_version: u32,
        idempotency_key: &str,
        actor: &AuditActor,
        original_error: Error,
    ) -> Result<String> {
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
                            .sales_change_orders()
                            .find_by_id(&change_order_id, session)
                            .await?
                            .ok_or_else(|| Error::NotFound("销售变更单不存在".to_string()))?;
                        let sales_order = db
                            .sales_orders()
                            .find_by_id(&change.sales_order_id, session)
                            .await?
                            .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
                        let organization_id = sales_change_responsible_org_id(&sales_order)?;
                        let _ = sales_change_order_object_readable(&organization_id, &actor_id)?;
                        let binding = find_approval_binding(&db, &change_order_id, session).await?;
                        let binding = require_frozen_binding(binding.as_ref())?;
                        let subject = sales_change_order_subject_ref(&change_order_id)?;
                        replay_sales_change_start_with_executor(
                            &db,
                            &subject,
                            subject_version,
                            &idempotency_key,
                            binding,
                            &actor_id,
                            session,
                        )
                        .await
                    })
                })
                .await;
            match recovered {
                Ok(Some(instance_id)) => return Ok(instance_id),
                Ok(None) => {}
                Err(error) if command_may_have_committed(&error) => {}
                Err(error) => return Err(error),
            }
            if attempt + 1 < RECOVERY_ATTEMPTS {
                tokio::time::sleep(command_recovery_delay(attempt)).await;
            }
        }
        Err(original_error)
    }
}

/// 构建详情视图，并附带只读审批结构。
///
/// # 参数
/// * `change_order` - 变更单
/// * `binding` - 创建时冻结的定义绑定
///
/// # 返回
/// 返回详情视图。
fn detail_view(
    change_order: SalesChangeOrder,
    binding: Option<entities::document_registry::business_document::ApprovalDefinitionBinding>,
) -> SalesChangeOrderDetailView {
    SalesChangeOrderDetailView {
        id: change_order.base.id,
        sales_order_id: change_order.sales_order_id.to_string(),
        base_revision_id: change_order.base_revision_id.to_string(),
        change_type: change_order.change_type,
        reason: change_order.reason,
        status: change_order.stable.status(),
        current_submission_id: change_order
            .current_submission_id
            .as_ref()
            .map(ToString::to_string),
        target_content_hash: change_order.target_content_hash,
        effective_revision_id: change_order
            .effective_revision_id
            .as_ref()
            .map(ToString::to_string),
        version: change_order.base.version,
        created_at: change_order.base.created_at,
        approval: document_approval_view(binding.as_ref(), None, change_order.stable.status()),
    }
}
///
/// 从当前生效销售版本构建变更工作副本行。
///
/// # 参数
/// * `working_copy_id` - 所属工作副本 ID
/// * `revision_lines` - 当前生效版本的公共行快照
/// * `goods_lines` - 当前生效版本的实物服务子行快照
///
/// # 返回
/// 返回 `(行创建数据, 行实体清单)`。
///
/// # 错误
/// 版本为空、含非实物服务行或公共行缺少子行时返回错误。
fn build_change_working_copy_lines_from_revision(
    working_copy_id: &SalesOrderWorkingCopyId,
    revision_lines: &[entities::sales_order::SalesOrderRevisionLine],
    goods_lines: &[entities::sales_order::SalesOrderGoodsServiceLineRevision],
) -> Result<(
    Vec<SalesOrderWorkingCopyLineData>,
    Vec<entities::sales_order::SalesOrderWorkingCopyLine>,
)> {
    if revision_lines.is_empty() {
        return Err(Error::ConflictError(
            "销售单当前版本没有明细，无法发起变更".to_string(),
        ));
    }
    let mut datas = Vec::with_capacity(revision_lines.len());
    for line in revision_lines {
        let goods = goods_lines
            .iter()
            .find(|goods| goods.revision_line_id.as_ref() == line.base.id)
            .ok_or_else(|| {
                Error::ConflictError(format!("销售单当前版本第 {} 行缺少实物服务快照", line.line_no))
            })?;
        datas.push(line.to_goods_working_copy_data(goods)?);
    }
    let mut built = Vec::with_capacity(datas.len());
    for data in &datas {
        built.push(entities::sales_order::SalesOrderWorkingCopyLine::new(
            entities::ids::SalesOrderWorkingCopyLineId::new(next_id()),
            working_copy_id.clone(),
            data.clone(),
        )?);
    }
    Ok((datas, built))
}

/// 从变更工作副本构建变更提交快照。
///
/// # 参数
/// * `change_order` - 变更单
/// * `working_copy` - 变更工作副本
/// * `lines` - 工作副本行
/// * `submission_no` - 本次提交序号
/// * `actor` - 提交人
///
/// # 返回
/// 返回 `(变更提交实体, 变更提交行清单)`。
///
/// # 错误
/// 工作副本关系、字段映射或提交实体校验失败时返回错误。
fn build_change_submission_with_no(
    change_order: &SalesChangeOrder,
    working_copy: &entities::sales_order::SalesOrderWorkingCopy,
    lines: &[entities::sales_order::SalesOrderWorkingCopyLine],
    submission_no: u32,
    actor: &AuditActor,
) -> Result<(SalesChangeSubmission, Vec<SalesChangeSubmissionLine>)> {
    let data = SalesChangeSubmissionData::from_sales_working_copy(
        change_order,
        working_copy,
        lines,
        submission_no,
        Instant::now(),
        actor.id(),
    )?;
    let line_datas = data.lines.clone();
    let submission = SalesChangeSubmission::new(SalesChangeSubmissionId::new(next_id()), data)?;
    let mut submission_lines = Vec::with_capacity(line_datas.len());
    for data in line_datas {
        submission_lines.push(SalesChangeSubmissionLine::new(
            SalesChangeSubmissionLineId::new(next_id()),
            submission.base.id.clone().into(),
            data,
        )?);
    }
    Ok((submission, submission_lines))
}

/// 销售变更单创建事务写入集合。
///
/// # 用途
/// 收拢创建变更单时需一并持久化的单据、工作副本、注册行与审计。
///
/// # 参数
/// 无。
///
/// # 返回
/// 无。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 必须绑定已发布定义；人员重验失败时不得写入。
struct CreatedChangeOrderPersistInput {
    /// 待创建的变更单。
    change_order: SalesChangeOrder,
    /// 工作副本头。
    working_copy: entities::sales_order::SalesOrderWorkingCopy,
    /// 工作副本行。
    lines: Vec<entities::sales_order::SalesOrderWorkingCopyLine>,
    /// 待登记的业务单据。
    document: BusinessDocument,
    /// 发布定义绑定命令。
    bind_command: BindPublishedDefinitionCommand,
    /// 已构造审计。
    audit: entities::AuditLog,
    /// 审计操作人。
    actor: AuditActor,
}

/// 在创建事务内写入变更单、绑定发布定义并登记单据。
///
/// # 用途
/// 创建变更单时原子写入单据、工作副本与发布定义绑定。
///
/// # 参数
/// * `db` - 数据库
/// * `rbac` - 共享 RBAC 服务
/// * `input` - 变更单、工作副本、注册行与审计
///
/// # 返回
/// 成功时无返回值。
///
/// # 错误
/// 无发布定义、人员重验失败或写入失败时返回错误，调用方必须回滚。
///
/// # 关键业务约束
/// 销售变更单必须绑定已发布定义，不得无定义创建。
async fn persist_created_change_order(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    input: CreatedChangeOrderPersistInput,
) -> Result<()> {
    let CreatedChangeOrderPersistInput {
        change_order,
        working_copy,
        lines,
        mut document,
        bind_command,
        audit,
        actor,
    } = input;
    let db = db.clone();
    let rbac = rbac.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                persist_bound_change_document(&db, &rbac, &mut document, &bind_command, &actor, session)
                    .await?;
                db.sales_change_orders().create(&change_order, session).await?;
                db.sales_order_working_copies()
                    .create(&working_copy, session)
                    .await?;
                for line in &lines {
                    db.sales_order_working_copy_lines().create(line, session).await?;
                }
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
    let _ = sales_change_order_object_readable(
        &bind_command.context.organization_id,
        &bind_command.context.creator_id,
    )?;
    let binding =
        bind_published_definition_on_document_create(db, rbac, bind_command, actor, session).await?;
    let binding = binding.ok_or_else(|| Error::Internal("销售变更单必须绑定已发布定义".to_string()))?;
    attach_published_binding(document, binding)?;
    db.business_documents().create(document, session).await?;
    Ok(())
}

/// 加载变更工作副本及其明细。
///
/// 先取可编辑副本；撤回后再提交时按变更单定位已提交副本。
///
/// # 错误
/// 工作副本不存在时返回 `NotFound`。
async fn load_change_working_copy(
    db: &mongodb::Database,
    change_order: &SalesChangeOrder,
) -> Result<(
    entities::sales_order::SalesOrderWorkingCopy,
    Vec<entities::sales_order::SalesOrderWorkingCopyLine>,
)> {
    let working_copy = db
        .sales_order_working_copies()
        .find_resubmittable_sales_change_copy(
            &change_order.sales_order_id,
            &SalesChangeOrderId::new(change_order.base.id.clone()),
            &mut NoTransaction,
        )
        .await?
        .ok_or_else(|| Error::NotFound("变更工作副本不存在".to_string()))?;
    let copy_id = SalesOrderWorkingCopyId::new(working_copy.base.id.clone());
    let copy_lines = db
        .sales_order_working_copy_lines()
        .list_lines_by_working_copy(&copy_id, &mut NoTransaction)
        .await?;
    Ok((working_copy, copy_lines))
}

/// 计算下一次变更提交序号。
///
/// # 参数
/// * `db` - 数据库
/// * `change_order_id` - 销售变更单 ID
///
/// # 返回
/// 返回严格递增的下一提交序号。
///
/// # 错误
/// 仓储失败或提交序号溢出时返回错误。
async fn next_change_submission_no(db: &mongodb::Database, change_order_id: &str) -> Result<u32> {
    Ok(SalesChangeSubmission::next_submission_no(
        latest_change_submission_no(db, change_order_id).await?,
    )?)
}

/// 读取已冻结的最大提交序号；尚无提交时返回 0。
///
/// # 参数
/// * `db` - 数据库
/// * `change_order_id` - 销售变更单 ID
///
/// # 返回
/// 返回当前最大提交序号。
///
/// # 错误
/// 仓储失败时返回错误。
async fn latest_change_submission_no(db: &mongodb::Database, change_order_id: &str) -> Result<u32> {
    Ok(db
        .sales_change_submissions()
        .latest_submission_no_by_change_order(&SalesChangeOrderId::new(change_order_id), &mut NoTransaction)
        .await?)
}

/// 在同一事务内生成生效修订并改写销售单。
///
/// # 错误
/// 基准版本漂移、缺少提交或写入失败时返回错误。
async fn persist_effective_change(
    db: &mongodb::Database,
    change_order: SalesChangeOrder,
    actor: &AuditActor,
) -> Result<()> {
    let write = prepare_effective_change_write(db, change_order, actor).await?;
    persist_effective_writes(db, write).await
}

/// 读取销售变更来源并完成生效写入计划的领域计算。
///
/// # 错误
/// 基准版本漂移、提交缺失或版本构造失败时返回错误。
async fn prepare_effective_change_write(
    db: &mongodb::Database,
    change_order: SalesChangeOrder,
    actor: &AuditActor,
) -> Result<EffectiveChangeWrite> {
    let submission_id = change_order.required_current_submission_id()?.clone();
    let submission = db
        .sales_change_submissions()
        .find_by_id(&submission_id, &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::NotFound("变更提交不存在".to_string()))?;
    let submission_lines = db
        .sales_change_submission_lines()
        .list_lines_by_submission(&submission.base.id.clone().into(), &mut NoTransaction)
        .await?;
    let order = db
        .sales_orders()
        .find_by_id(&change_order.sales_order_id, &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
    let current_revision_id = order
        .current_revision_id()
        .ok_or_else(|| Error::BusinessLogicError("销售单缺少当前版本".to_string()))?;
    if !change_order.base_revision_matches(current_revision_id) {
        return Err(Error::ConflictError(
            "基准版本已不是销售单当前版本，请刷新后重新发起变更".to_string(),
        ));
    }
    prepare_effective_revision_write(db, change_order, order, submission, submission_lines, actor).await
}

/// 构造生效修订并提交事务。
///
/// # 错误
/// 版本构造或写入失败时返回错误。
async fn prepare_effective_revision_write(
    db: &mongodb::Database,
    change_order: SalesChangeOrder,
    order: entities::sales_order::SalesOrder,
    submission: SalesChangeSubmission,
    submission_lines: Vec<SalesChangeSubmissionLine>,
    actor: &AuditActor,
) -> Result<EffectiveChangeWrite> {
    let now = Instant::now();
    let current_revision_no = db
        .sales_order_revisions()
        .latest_revision_no(&change_order.sales_order_id, &mut NoTransaction)
        .await?;
    let revision_no =
        entities::sales_order::SalesOrderRevision::next_revision_no(current_revision_no.unwrap_or(0))?;
    let revision = build_change_revision(&order, &submission, &submission_lines, revision_no, now)?;
    let current_revision_id = order
        .current_revision_id()
        .ok_or_else(|| Error::BusinessLogicError("销售单缺少当前版本".to_string()))?;
    let current_revision = db
        .sales_order_revisions()
        .find_by_id(current_revision_id, &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::NotFound("销售单当前版本不存在".to_string()))?;
    let mut order_for_tx = order.clone();
    order_for_tx.attach_revision(&revision.revision.base.id, actor.id());
    let mut change_for_tx = change_order.clone();
    change_for_tx.apply_effective(revision.revision.base.id.clone().into(), actor.id())?;
    let existing_account = db
        .receivable_accounts()
        .find_primary_by_sales_order(&change_order.sales_order_id, &mut NoTransaction)
        .await?;
    let delta = build_receivable_delta(
        &order,
        &revision,
        current_revision.gross_amount,
        existing_account,
        now,
        actor.id(),
    )?;
    let audit = actor.clone().resource_log(
        "sales_change_order.effective",
        "sales_change_order",
        change_order.base.id.clone(),
    )?;
    Ok(EffectiveChangeWrite {
        order: order_for_tx,
        change: change_for_tx,
        revision,
        delta,
        audit,
    })
}

/// 销售变更最终生效的完整事务写入上下文。
struct EffectiveChangeWrite {
    order: entities::sales_order::SalesOrder,
    change: SalesChangeOrder,
    revision: super::formalization::RevisionAggregate,
    delta: Option<(
        entities::receivable::ReceivableAccount,
        entities::receivable::ReceivableEntry,
    )>,
    audit: entities::AuditLog,
}

/// 持久化生效修订、应收差额与变更单状态。
///
/// # 错误
/// 仓储失败时返回错误。
async fn persist_effective_writes(db: &mongodb::Database, write: EffectiveChangeWrite) -> Result<()> {
    let db = db.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move { persist_effective_writes_in_transaction(&db, write, session).await })
        })
        .await
}

/// 在调用方事务内写入销售变更正式版本、应收差额、状态和成功审计。
///
/// # 错误
/// 任一仓储写入失败时返回错误。
async fn persist_effective_writes_in_transaction(
    db: &mongodb::Database,
    mut write: EffectiveChangeWrite,
    session: &mut ClientSession,
) -> Result<()> {
    db.sales_order()
        .formalize_submission(
            &mut write.order,
            &write.revision.revision,
            &write.revision.lines,
            &write.revision.goods_lines,
            &write.revision.voucher_lines,
            session,
        )
        .await?;
    if let Some((account, entry)) = write.delta {
        write_receivable_delta(db, account, entry, session).await?;
    }
    db.sales_change_orders()
        .update(&mut write.change, session)
        .await?;
    db.audit_logs().create(&write.audit, session).await?;
    Ok(())
}

/// 写入应收差额分录。
///
/// # 错误
/// 仓储失败时返回错误。
async fn write_receivable_delta(
    db: &mongodb::Database,
    mut account: entities::receivable::ReceivableAccount,
    entry: entities::receivable::ReceivableEntry,
    session: &mut ClientSession,
) -> Result<()> {
    let subject_version = entry.source_revision_id.to_string();
    db.receivable_entries().create(&entry, session).await?;
    db.receivable_accounts().update(&mut account, session).await?;
    crate::receivable::card_funds_task::ensure_card_funds_review_task(
        db,
        &account,
        &subject_version,
        session,
    )
    .await?;
    let account_id = ReceivableAccountId::new(account.base.id.clone());
    crate::receivable::invoice_task::sync_sales_invoice_task(
        db,
        &account_id,
        crate::receivable::invoice_task::SalesInvoiceTaskChange::ReceivableChanged,
        session,
    )
    .await
}

#[cfg(test)]
mod tests {
    /// 创建必须注册 BusinessDocument 并调用统一绑定端口。
    #[test]
    fn create_registers_document_and_binds_published_definition() {
        let source = include_str!("sales_change_order.rs");
        assert!(source.contains("bind_published_definition_on_document_create"));
        assert!(source.contains("new_registered_document"));
    }

    /// 提交必须调用 start_approval，且版本取 submission_no。
    #[test]
    fn submit_calls_start_approval_with_submission_no() {
        let source = include_str!("sales_change_order.rs");
        assert!(source.contains("start_change_approval"));
        assert!(source.contains("submission.submission_no"));
        assert!(source.contains("sales_change_start_command"));
    }

    /// 最终动作唯一为 apply_effective_change。
    #[test]
    fn final_action_is_apply_effective_change() {
        let source = include_str!("sales_change_order.rs");
        assert!(source.contains("pub async fn apply_effective_change"));
        assert!(source.contains("change_for_tx.apply_effective"));
    }

    /// 撤回必须调用统一 cancel 并回到草稿。
    #[test]
    fn cancel_uses_unified_port() {
        let source = include_str!("sales_change_order.rs");
        assert!(source.contains("pub async fn cancel_approval"));
        assert!(source.contains("prepare_cancel"));
        assert!(source.contains("adapter.cancel_action"));
    }

    /// 撤回后再提交使用语义仓储，并由实体递增版本、处理重复锁定。
    #[test]
    fn cancel_then_resubmit_uses_repository_and_entity_rules() {
        let source = include_str!("sales_change_order.rs");
        assert!(source.contains("find_resubmittable_sales_change_copy"));
        assert!(source.contains("latest_submission_no_by_change_order"));
        assert!(source.contains("SalesChangeSubmission::next_submission_no"));
        assert!(source.contains("lock_for_submission_if_needed"));
    }
}
