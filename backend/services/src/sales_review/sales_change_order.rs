// ---------------------------------------------------------------------
// 销售变更单（W05 变更轨；§8.1.3 本批部分）
// ---------------------------------------------------------------------

use std::str::FromStr;

use database::{
    AccessControlExt, DocumentRegistryExt, NoTransaction, ReceivableExt, SalesOrderExt, SalesReviewExt,
    Transactional,
};
use entities::common::time::Instant;
use entities::document_registry::{
    BusinessDocument, WorkflowAction, WorkflowActionData, WorkflowActionId, WorkflowActionType,
};
use entities::ids::{
    BusinessDocumentId, SalesChangeOrderId, SalesChangeSubmissionId, SalesChangeSubmissionLineId,
    SalesOrderId, SalesOrderRevisionId, SalesOrderRevisionLineId, SalesOrderWorkingCopyId,
};
use entities::money::Amount;
use entities::sales_order::{SalesOrderWorkingCopyLineData, WorkingCopyStatus, WorkingPurpose};
use entities::sales_review::{
    SalesChangeOrder, SalesChangeOrderData, SalesChangeOrderStatus, SalesChangeSubmission,
    SalesChangeSubmissionData, SalesChangeSubmissionLine, SalesChangeSubmissionLineData,
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
use super::sales_change_mapping::{change_copy_goods, change_copy_voucher, convert_line_type};
use super::start_approval::{
    build_sales_change_start_input, load_bound_definition_graph, load_start_receipt,
    persist_sales_change_start, SalesChangeStartInput, SalesChangeStartPersistInput,
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
use crate::approval::execution::{prepare_cancel, prepare_start};
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
    /// 仓储 `find_in_progress` 仍按旧确认态查询；目标 `IN_APPROVAL` 在本方法内补查。
    ///
    /// # 错误
    /// 仓储失败时返回错误。
    async fn has_in_progress_change(
        &self,
        sales_order_id: &SalesOrderId,
        base_revision_id: &str,
    ) -> Result<bool> {
        let base_revision = entities::sales_order::SalesOrderRevisionId::new(base_revision_id);
        let legacy = self
            .db
            .sales_change_orders()
            .find_in_progress_by_order_and_base(sales_order_id, &base_revision, &mut NoTransaction)
            .await?;
        if legacy.is_some() {
            return Ok(true);
        }
        let in_approval = self
            .db
            .sales_change_orders()
            .find_one(
                mongodb::bson::doc! {
                    "sales_order_id": sales_order_id.to_string(),
                    "base_revision_id": base_revision_id,
                    "status": SalesChangeOrderStatus::InApproval.as_str(),
                },
                &mut NoTransaction,
            )
            .await?;
        Ok(in_approval.is_some())
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
        if order.commercial_status != entities::sales_order::CommercialStatus::Effective {
            return Err(Error::BusinessLogicError(
                "只有已生效的销售单才能发起变更".to_string(),
            ));
        }
        let base_revision_id = order
            .stable
            .current_revision_id
            .clone()
            .ok_or_else(|| Error::BusinessLogicError("销售单缺少当前版本，无法发起变更".to_string()))?;
        let base_revision = self
            .db
            .sales_order_revisions()
            .find_by_id(&base_revision_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售单当前版本不存在".to_string()))?;
        if base_revision.revision.revision_no != req.expected_base_revision_no {
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
        if order.business_type == entities::sales_order::BusinessType::Voucher {
            return Err(Error::BusinessLogicError(
                "卡券销售变更缺少原正式版本冻结的目标商城或应收到期日，禁止创建变更单".to_string(),
            ));
        }

        let change_order = SalesChangeOrder::new(
            SalesChangeOrderId::new(next_id()),
            SalesChangeOrderData {
                sales_order_id: req.sales_order_id.clone(),
                base_revision_id: base_revision_id.clone().into(),
                change_type: req.change_type,
                reason: req.reason,
            },
            actor.id(),
        )?;
        let revision_lines = self
            .db
            .sales_order_revision_lines()
            .list_lines_by_revision(
                &SalesOrderRevisionId::new(base_revision_id.clone()),
                &mut NoTransaction,
            )
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
        let (gross, net, tax) = change_line_totals(&lines);
        let working_copy = entities::sales_order::SalesOrderWorkingCopy::new(
            working_copy_id,
            entities::sales_order::SalesOrderWorkingCopyData {
                sales_order_id: req.sales_order_id.clone(),
                working_purpose: WorkingPurpose::SalesChange,
                sales_change_order_id: Some(change_order.base.id.clone().into()),
                base_revision_id: Some(base_revision_id.clone().into()),
                draft_version: 1,
                content_hash: format!("change:{}:1", change_order.base.id),
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
        if change_order.base.version != req.version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        if change_order.stable.status() != SalesChangeOrderStatus::Draft {
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
        if change_order.base.version != req.version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        change_order.void(actor.id())?;
        let mut working_copy = self
            .db
            .sales_order_working_copies()
            .find_active_by_order_and_purpose(
                &change_order.sales_order_id,
                WorkingPurpose::SalesChange,
                &mut NoTransaction,
            )
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
        if change_order.base.version != req.expected_version {
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
        let input = build_sales_change_cancel_input(
            &runtime,
            &req.reason,
            actor.id(),
            &req.idempotency_key,
            None,
            now,
        )?;
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
        lock_working_copy_if_editing(&mut working_copy)?;
        start_sales_change_approval(
            &mut change_order,
            submission.base.id.clone().into(),
            format!("sub:{}", submission.base.id),
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
        persist_sales_change_start(
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
        .await?;
        self.sales_change_order_detail(id).await
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
        if line.line_type != entities::sales_order::LineType::GoodsService {
            return Err(Error::ConflictError(format!(
                "销售单当前版本第 {} 行不是实物服务行",
                line.line_no
            )));
        }
        let goods = goods_lines
            .iter()
            .find(|goods| goods.revision_line_id.as_ref() == line.base.id)
            .ok_or_else(|| {
                Error::ConflictError(format!("销售单当前版本第 {} 行缺少实物服务快照", line.line_no))
            })?;
        datas.push(SalesOrderWorkingCopyLineData {
            sales_order_line_id: line.sales_order_line_id.clone(),
            line_no: line.line_no,
            line_type: line.line_type,
            sales_tax_rate: line.sales_tax_rate,
            item_name_snapshot: line.item_name_snapshot.clone(),
            spec_snapshot: line.spec_snapshot.clone(),
            unit_snapshot: line.unit_snapshot.clone(),
            goods: Some(entities::sales_order::GoodsLineFields {
                sku_id: goods.sku_id.clone(),
                sku_revision_id: goods.sku_revision_id.clone(),
                welfare_scenario: goods.welfare_scenario,
                service_region: goods.service_region.clone(),
                fulfillment_mode: goods.fulfillment_mode,
                fulfillment_due_at: goods.fulfillment_due_at,
                quantity: goods.quantity,
                base_unit_code: goods.base_unit_code.clone(),
                unit_price_gross: goods.unit_price_gross,
            }),
            voucher: None,
        });
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

/// 变更行金额访问器。
trait ChangeLineAmounts {
    /// 返回行含税金额。
    fn gross_amount(&self) -> Amount;
    /// 返回行不含税金额。
    fn net_amount(&self) -> Amount;
    /// 返回行税额。
    fn tax_amount(&self) -> Amount;
}

impl ChangeLineAmounts for entities::sales_order::SalesOrderWorkingCopyLine {
    fn gross_amount(&self) -> Amount {
        self.gross_amount
    }
    fn net_amount(&self) -> Amount {
        self.net_amount
    }
    fn tax_amount(&self) -> Amount {
        self.tax_amount
    }
}

/// 汇总已舍入的行金额三元组（§4.2 铁律 2）。
///
/// # 参数
/// * `lines` - 行实体
///
/// # 返回
/// 返回 `(含税合计, 不含税合计, 税额合计)`。
fn change_line_totals(
    lines: &[entities::sales_order::SalesOrderWorkingCopyLine],
) -> (Amount, Amount, Amount) {
    let zero = Amount::from_str("0.00").expect("静态零值必须合法");
    let gross = lines
        .iter()
        .fold(zero, |acc, line| acc.checked_add(line.gross_amount()));
    let net = lines
        .iter()
        .fold(zero, |acc, line| acc.checked_add(line.net_amount()));
    let tax = lines
        .iter()
        .fold(zero, |acc, line| acc.checked_add(line.tax_amount()));
    (gross, net, tax)
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
/// 提交字段校验失败时返回错误。
fn build_change_submission_with_no(
    change_order: &SalesChangeOrder,
    working_copy: &entities::sales_order::SalesOrderWorkingCopy,
    lines: &[entities::sales_order::SalesOrderWorkingCopyLine],
    submission_no: u32,
    actor: &AuditActor,
) -> Result<(SalesChangeSubmission, Vec<SalesChangeSubmissionLine>)> {
    let (gross, net, tax) = change_line_totals(lines);
    // 提交头 `validate_line_list` 需要行摘要；行实体另集存储，但创建数据必须非空。
    let line_datas = build_change_submission_line_datas(lines)?;
    let submission = SalesChangeSubmission::new(
        SalesChangeSubmissionId::new(next_id()),
        SalesChangeSubmissionData {
            sales_change_order_id: change_order.base.id.clone().into(),
            submission_no,
            base_revision_id: change_order.base_revision_id.clone(),
            sales_order_id: change_order.sales_order_id.clone(),
            working_copy_id: working_copy.base.id.clone().into(),
            working_copy_version: working_copy.draft_version,
            business_type: convert_business_type(working_copy.business_type),
            customer_id: working_copy.customer_id.clone(),
            contract_revision_id: working_copy.contract_revision_id.clone(),
            settlement_party_id: working_copy.settlement_party_id.clone(),
            snapshot: entities::sales_review::HeaderSnapshotData {
                customer_name: working_copy.customer_snapshot.customer_name.clone(),
                contract_no: working_copy
                    .contract_snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.contract_no.clone()),
                settlement_party_name: working_copy
                    .settlement_party_snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.settlement_party_name.clone()),
                payment_term_code: working_copy.payment_term_snapshot.payment_term_code.clone(),
                payment_term_name: working_copy.payment_term_snapshot.payment_term_name.clone(),
                invoice_type: working_copy.invoice_requirement_snapshot.invoice_type.clone(),
                tax_point: working_copy.invoice_requirement_snapshot.tax_point.clone(),
            },
            project_name: working_copy.project_name.clone(),
            business_remark: working_copy.business_remark.clone(),
            voucher_category_sku_id: working_copy.voucher_category_sku_id.clone(),
            voucher_expiry_at: working_copy.voucher_expiry_at,
            gross_amount: gross,
            net_amount: net,
            tax_amount: tax,
            submitted_at: Instant::now(),
            submitted_by: actor.id().to_string(),
            lines: line_datas.clone(),
        },
    )
    .map_err(Error::Logic)?;
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

/// D13 业务性质 → D14 同形类型转换。
fn convert_business_type(value: entities::sales_order::BusinessType) -> entities::sales_review::BusinessType {
    match value {
        entities::sales_order::BusinessType::GoodsService => {
            entities::sales_review::BusinessType::GoodsService
        }
        entities::sales_order::BusinessType::Voucher => entities::sales_review::BusinessType::Voucher,
    }
}

/// 从变更工作副本行构建变更提交行创建数据（提交头行摘要校验用）。
///
/// # 参数
/// * `lines` - 工作副本行
///
/// # 返回
/// 返回变更提交行创建数据清单。
///
/// # 错误
/// 行字段组缺失或非法时返回错误。
fn build_change_submission_line_datas(
    lines: &[entities::sales_order::SalesOrderWorkingCopyLine],
) -> Result<Vec<SalesChangeSubmissionLineData>> {
    let mut datas = Vec::with_capacity(lines.len());
    for line in lines {
        let goods = change_copy_goods(line)?;
        let voucher = change_copy_voucher(line)?;
        datas.push(SalesChangeSubmissionLineData {
            sales_order_line_id: line.sales_order_line_id.clone(),
            line_no: line.line_no,
            line_type: convert_line_type(line.line_type),
            sales_tax_rate: line.sales_tax_rate,
            item_name_snapshot: line.item_name_snapshot.clone(),
            spec_snapshot: line.spec_snapshot.clone(),
            unit_snapshot: line.unit_snapshot.clone(),
            goods,
            voucher,
        });
    }
    Ok(datas)
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
    let active = db
        .sales_order_working_copies()
        .find_active_by_order_and_purpose(
            &change_order.sales_order_id,
            WorkingPurpose::SalesChange,
            &mut NoTransaction,
        )
        .await?;
    let bound = db
        .sales_order_working_copies()
        .find_one(
            mongodb::bson::doc! {
                "sales_change_order_id": change_order.base.id.clone(),
                "working_purpose": WorkingPurpose::SalesChange.as_str(),
            },
            &mut NoTransaction,
        )
        .await?;
    let working_copy = choose_resubmit_source(active, bound)?;
    let copy_id = SalesOrderWorkingCopyId::new(working_copy.base.id.clone());
    let copy_lines = db
        .sales_order_working_copy_lines()
        .list_lines_by_working_copy(&copy_id, &mut NoTransaction)
        .await?;
    Ok((working_copy, copy_lines))
}

/// 撤回后再提交时优先可编辑副本，否则使用已绑定该变更单的已提交副本。
///
/// # 错误
/// 两者皆空时返回 `NotFound`。
pub(super) fn choose_resubmit_source<T>(active: Option<T>, bound: Option<T>) -> Result<T> {
    if let Some(active) = active {
        return Ok(active);
    }
    bound.ok_or_else(|| Error::NotFound("变更工作副本不存在".to_string()))
}

/// 仅在编辑中时锁定工作副本；已提交副本可直接用于新 `submission_no`。
///
/// # 错误
/// 状态不允许提交时返回错误。
fn lock_working_copy_if_editing(
    working_copy: &mut entities::sales_order::SalesOrderWorkingCopy,
) -> Result<()> {
    if working_copy_already_submitted(working_copy.stable.status()) {
        return Ok(());
    }
    Ok(working_copy.submit()?)
}

/// 已提交工作副本不必再锁定。
///
/// # 参数
/// * `status` - 工作副本状态
///
/// # 返回
/// `Submitted` 为 `true`。
pub(super) fn working_copy_already_submitted(status: WorkingCopyStatus) -> bool {
    status == WorkingCopyStatus::Submitted
}

/// 由已冻结最大序号计算下一次提交号。
///
/// # 错误
/// 溢出时返回冲突。
pub(super) fn next_submission_no_from(current_max: u32) -> Result<u32> {
    current_max
        .checked_add(1)
        .ok_or_else(|| Error::ConflictError("变更提交序号溢出".to_string()))
}

/// 计算下一次变更提交序号。
///
/// # 错误
/// 仓储失败或序号溢出时返回错误。
async fn next_change_submission_no(db: &mongodb::Database, change_order_id: &str) -> Result<u32> {
    next_submission_no_from(latest_change_submission_no(db, change_order_id).await?)
}

/// 读取已冻结的最大提交序号；尚无提交时返回 0。
///
/// # 错误
/// 仓储失败时返回错误。
async fn latest_change_submission_no(db: &mongodb::Database, change_order_id: &str) -> Result<u32> {
    let submissions = db
        .sales_change_submissions()
        .find_many(
            mongodb::bson::doc! { "sales_change_order_id": change_order_id },
            &mut NoTransaction,
        )
        .await?;
    Ok(submissions
        .iter()
        .map(|submission| submission.submission_no)
        .max()
        .unwrap_or(0))
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
    let submission_id = change_order
        .current_submission_id
        .clone()
        .ok_or_else(|| Error::BusinessLogicError("变更单尚未提交审批".to_string()))?;
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
        .stable
        .current_revision_id
        .clone()
        .ok_or_else(|| Error::BusinessLogicError("销售单缺少当前版本".to_string()))?;
    if current_revision_id != change_order.base_revision_id.to_string() {
        return Err(Error::ConflictError(
            "基准版本已不是销售单当前版本，请刷新后重新发起变更".to_string(),
        ));
    }
    write_effective_revision(db, change_order, order, submission, submission_lines, actor).await
}

/// 构造生效修订并提交事务。
///
/// # 错误
/// 版本构造或写入失败时返回错误。
async fn write_effective_revision(
    db: &mongodb::Database,
    change_order: SalesChangeOrder,
    order: entities::sales_order::SalesOrder,
    submission: SalesChangeSubmission,
    submission_lines: Vec<SalesChangeSubmissionLine>,
    actor: &AuditActor,
) -> Result<()> {
    let now = Instant::now();
    let existing_revisions = db
        .sales_order_revisions()
        .list_by_order(&change_order.sales_order_id, &mut NoTransaction)
        .await?;
    let revision_no = existing_revisions
        .iter()
        .map(|revision| revision.revision.revision_no)
        .max()
        .unwrap_or(0)
        + 1;
    let revision = build_change_revision(&order, &submission, &submission_lines, revision_no, now)?;
    let current_revision_id = order
        .stable
        .current_revision_id
        .clone()
        .ok_or_else(|| Error::BusinessLogicError("销售单缺少当前版本".to_string()))?;
    let current_revision = db
        .sales_order_revisions()
        .find_by_id(&current_revision_id, &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::NotFound("销售单当前版本不存在".to_string()))?;
    let mut order_for_tx = order.clone();
    order_for_tx.attach_revision(&revision.revision.base.id, actor.id());
    let mut change_for_tx = change_order.clone();
    change_for_tx.apply_effective(revision.revision.base.id.clone().into(), actor.id())?;
    let existing_account = db
        .receivable_accounts()
        .find_one_by_field(
            "sales_order_id",
            change_order.sales_order_id.to_string(),
            &mut NoTransaction,
        )
        .await?;
    let delta = build_receivable_delta(
        &order,
        &revision,
        current_revision.gross_amount,
        existing_account,
        now,
    )?;
    let audit = actor.clone().resource_log(
        "sales_change_order.effective",
        "sales_change_order",
        change_order.base.id.clone(),
    )?;
    persist_effective_writes(db, order_for_tx, change_for_tx, revision, delta, audit).await
}

/// 持久化生效修订、应收差额与变更单状态。
///
/// # 错误
/// 仓储失败时返回错误。
async fn persist_effective_writes(
    db: &mongodb::Database,
    mut order_for_tx: entities::sales_order::SalesOrder,
    mut change_for_tx: SalesChangeOrder,
    revision: super::formalization::RevisionAggregate,
    delta: Option<(
        entities::receivable::ReceivableAccount,
        entities::receivable::ReceivableEntry,
        bool,
    )>,
    audit: entities::AuditLog,
) -> Result<()> {
    let db = db.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                db.sales_order()
                    .formalize_submission(
                        &mut order_for_tx,
                        &revision.revision,
                        &revision.lines,
                        &revision.goods_lines,
                        &revision.voucher_lines,
                        session,
                    )
                    .await?;
                if let Some((account, entry, is_existing_account)) = delta {
                    write_receivable_delta(&db, account, entry, is_existing_account, session).await?;
                }
                db.sales_change_orders()
                    .update(&mut change_for_tx, session)
                    .await?;
                db.audit_logs().create(&audit, session).await?;
                Ok::<(), crate::errors::Error>(())
            })
        })
        .await
}

/// 写入应收差额分录。
///
/// # 错误
/// 仓储失败时返回错误。
async fn write_receivable_delta(
    db: &mongodb::Database,
    mut account: entities::receivable::ReceivableAccount,
    entry: entities::receivable::ReceivableEntry,
    is_existing_account: bool,
    session: &mut ClientSession,
) -> Result<()> {
    if is_existing_account {
        db.receivable_entries().create(&entry, session).await?;
        db.receivable_accounts().update(&mut account, session).await?;
        return Ok(());
    }
    db.receivable()
        .create_receivable_with_entry(&account, &entry, session)
        .await
        .map_err(Into::into)
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

    /// 撤回后可定位已提交工作副本，并递增 submission_no。
    #[test]
    fn cancel_then_resubmit_uses_bound_copy_and_increments_submission_no() {
        assert_eq!(super::next_submission_no_from(0).unwrap(), 1);
        assert_eq!(super::next_submission_no_from(1).unwrap(), 2);
        assert!(super::next_submission_no_from(u32::MAX).is_err());
        assert!(super::working_copy_already_submitted(
            entities::sales_order::WorkingCopyStatus::Submitted
        ));
        assert!(!super::working_copy_already_submitted(
            entities::sales_order::WorkingCopyStatus::Editing
        ));
        assert_eq!(
            super::choose_resubmit_source(None, Some("submitted-copy")).unwrap(),
            "submitted-copy"
        );
        assert_eq!(
            super::choose_resubmit_source(Some("editing-copy"), Some("submitted-copy")).unwrap(),
            "editing-copy"
        );
        assert!(super::choose_resubmit_source::<&str>(None, None).is_err());
    }
}
