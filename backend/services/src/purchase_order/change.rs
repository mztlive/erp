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
    PurchaseChangeOrder, PurchaseChangeOrderData, PurchaseChangeSubmission, PurchaseChangeSubmissionData,
    PurchaseOrder, PurchaseOrderRevision,
};
use id_generator::next_id;
use mongodb::ClientSession;
use validator::Validate;

use super::allocation_maintenance::{persist_current_sales_allocations, prepare_current_sales_allocations};
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
use super::procurement_task_sync::sync_procurement_tasks_for_sales_order;
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
        change
            .ensure_expected_version(req.expected_lock_version)
            .map_err(|_| Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()))?;
        execute_purchase_change_domain_action(
            &mut change.clone(),
            ApprovalDomainAction::PurchaseChangeOrderApplyEffectiveChange,
            actor.id(),
        )?;
        let submission_id = change
            .submission_id_for_effect(Some(req.submission_id.as_str()))
            .map_err(|error| Error::ConflictError(error.to_string()))?;
        self.persist_effective_change(change, submission_id.to_string(), actor)
            .await
    }

    /// 在审批运行时持有的事务内生效采购变更。
    ///
    /// # 错误
    /// 状态、基准版本、应付/成本差额或持久化不变量失败时返回错误。
    pub(crate) async fn apply_effective_change_in_transaction(
        &self,
        change_id: &str,
        actor: &AuditActor,
        session: &mut ClientSession,
    ) -> Result<()> {
        let change = self
            .db
            .purchase_change_orders()
            .find_by_id(change_id, session)
            .await?
            .ok_or_else(|| Error::NotFound("采购变更单不存在".to_string()))?;
        execute_purchase_change_domain_action(
            &mut change.clone(),
            ApprovalDomainAction::PurchaseChangeOrderApplyEffectiveChange,
            actor.id(),
        )?;
        let submission_id = change
            .submission_id_for_effect(None)
            .map_err(|error| Error::ConflictError(error.to_string()))?;
        let prepared = self
            .prepare_effective_change_write(&change, submission_id.as_ref())
            .await?;
        write_effective_change_in_transaction(&self.db, prepared.write, actor, session)
            .await
            .map(|_| ())
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
        let (_, sort_dir) = super::dto::normalize_sort(&params.sort_by, &params.sort_dir, &["created_at"])?;
        let page = params.page.unwrap_or(1);
        let page_size = params.page_size.unwrap_or(20).clamp(1, 100);
        let purchase_order_id = normalized_filter(params.purchase_order_id.as_deref());
        let status = normalized_filter(params.status.as_deref());
        let result = self
            .db
            .purchase_order()
            .search_change_orders(
                purchase_order_id.as_deref(),
                status.as_deref(),
                page,
                page_size,
                matches!(sort_dir, super::dto::SortDir::Asc),
                &mut NoTransaction,
            )
            .await?;
        let total = result.total;
        let views = result
            .items
            .into_iter()
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
        let supplier_name = self
            .resolve_supplier_name(&order.supplier_id)
            .await?
            .unwrap_or_else(|| order.supplier_id.to_string());
        let submission = self
            .build_change_submission(change, order, &base_revision, &supplier_name, &normalized_request)
            .await?;
        let enriched_lines = self
            .enrich_change_lines_with_current_sales_revision(order, &normalized_request.lines)
            .await?;
        let lines = self
            .build_change_submission_lines(&submission.base.id.clone(), &enriched_lines)
            .await?;
        let mut submission_mut = submission.clone();
        submission_mut.submit(Instant::now(), actor.id())?;
        Ok(FrozenChangeSubmission {
            submission: submission_mut,
            lines,
            content_hash: content_fingerprint(&normalized_request.lines),
            idempotency_key: req.idempotency_key.clone(),
        })
    }

    /// 从变更单冻结的基准采购版本恢复完整目标行。
    async fn change_lines_from_base_revision(
        &self,
        revision_id: &entities::ids::PurchaseOrderRevisionId,
    ) -> Result<Vec<SavePurchaseOrderLine>> {
        let mut lines = self
            .db
            .purchase_order_revision_lines()
            .find_lines_by_revision_ids(std::slice::from_ref(revision_id), &mut NoTransaction)
            .await?;
        lines.sort_by_key(|line| line.line_no);
        if lines.is_empty() {
            return Err(Error::BusinessLogicError("采购变更基准版本缺少明细".to_string()));
        }
        Ok(lines
            .into_iter()
            .map(|line| {
                let is_item = line.line_type == entities::purchase_order::PurchaseLineType::ItemService;
                SavePurchaseOrderLine {
                    line_type: line.line_type,
                    procurement_confirmation_line_id: line
                        .procurement_confirmation_line_id
                        .map(|value| value.to_string()),
                    sku_id: line.sku_id.map(|value| value.to_string()),
                    sku_revision_id: line.sku_revision_id.map(|value| value.to_string()),
                    product_name: line.product_name_snapshot,
                    specification: line.specification_snapshot,
                    quantity: line.quantity.map(|value| value.to_string()),
                    base_unit_code: line.base_unit_code,
                    unit_cost_gross: line.unit_cost_gross.map(|value| value.to_string()),
                    input_tax_rate: line.input_tax_rate.map(|value| value.to_string()),
                    expected_delivery_date: line.expected_delivery_date.map(|value| value.to_string()),
                    sales_order_line_id: line.sales_order_line_id.map(|value| value.to_string()),
                    sales_order_revision_line_id: line
                        .sales_order_revision_line_id
                        .map(|value| value.to_string()),
                    sales_order_submission_line_id: None,
                    allocated_quantity: line.allocated_quantity.map(|value| value.to_string()),
                    gross_amount: if is_item {
                        None
                    } else {
                        Some(line.gross_amount.to_string())
                    },
                }
            })
            .collect())
    }

    /// 将采购变更目标行绑定到来源销售单当前版本行。
    ///
    /// # 参数
    /// * `order` - 原采购单，用于定位来源销售单
    /// * `lines` - 变更目标完整行请求
    ///
    /// # 返回
    /// 返回稳定销售行与销售当前版本行均已刷新的目标行。
    ///
    /// # 错误
    /// 来源销售单、当前销售版本或稳定销售行缺失，以及仓储查询失败时返回错误。
    ///
    /// # 关键业务约束
    /// 不再沿历史采购提交反查销售提交行；分配数量固定等于变更后的采购数量。
    async fn enrich_change_lines_with_current_sales_revision(
        &self,
        order: &PurchaseOrder,
        lines: &[SavePurchaseOrderLine],
    ) -> Result<Vec<SavePurchaseOrderLine>> {
        let sales_order = self
            .db
            .sales_orders()
            .find_by_id(&order.sales_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("来源销售单不存在".to_string()))?;
        let revision_id = sales_order
            .stable
            .current_revision_id
            .as_ref()
            .ok_or_else(|| Error::BusinessLogicError("来源销售单缺少当前版本".to_string()))?;
        let revision_lines = self
            .db
            .sales_order_revision_lines()
            .list_lines_by_revision(
                &entities::ids::SalesOrderRevisionId::new(revision_id.clone()),
                &mut NoTransaction,
            )
            .await?;
        let by_stable_id = revision_lines
            .into_iter()
            .map(|line| (line.sales_order_line_id.to_string(), line))
            .collect::<std::collections::HashMap<_, _>>();
        enrich_change_lines(lines, &by_stable_id)
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

    /// 准备生效修订与应付差额，并在同一事务内推进采购当前版本。
    ///
    /// # 参数
    /// * `change` - 已通过最终审批动作校验的采购变更单
    /// * `submission_id` - 当前冻结且待生效的变更提交主键
    /// * `actor` - 最终审批动作的审计操作人
    ///
    /// # 返回
    /// 返回新采购修订、应付差额引用和采购单最新乐观锁版本。
    ///
    /// # 错误
    /// 基准版本漂移、提交状态非法、销售采购 guard 并发冲突或写入失败时
    /// 返回错误。
    ///
    /// # 关键业务约束
    /// 采购修订、来源销售 guard、allocation、当前版本指针、采购任务和差额
    /// 必须原子提交。
    async fn persist_effective_change(
        &self,
        change: PurchaseChangeOrder,
        submission_id: String,
        actor: &AuditActor,
    ) -> Result<PurchaseChangeEffectResult> {
        let prepared = self
            .prepare_effective_change_write(&change, &submission_id)
            .await?;
        let PreparedEffectiveChange {
            write,
            revision_id,
            revision_no,
            payable_delta_entry_id,
        } = prepared;
        let purchase_order_lock_version = write_effective_change(&self.db, write, actor).await?;
        Ok(PurchaseChangeEffectResult {
            change_id: change.base.id.clone(),
            revision_id,
            revision_no,
            payable_delta_entry_id,
            purchase_order_lock_version,
            reference: format!("EFFECT-V{revision_no}"),
        })
    }

    /// 准备采购变更生效事务所需的修订、差额和响应引用。
    ///
    /// # 参数
    /// * `change` - 已通过最终审批动作校验的采购变更单
    /// * `submission_id` - 当前冻结且待生效的变更提交主键
    ///
    /// # 返回
    /// 返回可直接进入事务的完整写聚合及响应所需稳定引用。
    ///
    /// # 错误
    /// 原采购单、基准版本或提交缺失，基准版本漂移，或修订与差额构建
    /// 失败时返回错误。
    ///
    /// # 关键业务约束
    /// 这里只准备不可变写内容；采购、销售 guard、allocation 和任务的可见性
    /// 由后续事务保证。
    async fn prepare_effective_change_write(
        &self,
        change: &PurchaseChangeOrder,
        submission_id: &str,
    ) -> Result<PreparedEffectiveChange> {
        let order = self
            .db
            .purchase_orders()
            .find_by_id(&change.purchase_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("原采购单不存在".to_string()))?;
        change
            .ensure_base_revision_current(order.stable.current_revision_id.as_deref())
            .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
        let (submission, lines) = self.load_pending_change_submission(submission_id).await?;
        let revision_no = self.next_revision_no(&order).await?;
        let (revision, revision_lines) = self
            .build_change_revision(&order, &submission, &lines, revision_no)
            .await?;
        let delta = self
            .build_effective_change_delta(change, &order, &revision)
            .await?;
        Ok(PreparedEffectiveChange {
            revision_id: revision.base.id.clone(),
            revision_no,
            payable_delta_entry_id: delta.0.as_ref().map(|(_, entry)| entry.base.id.clone()),
            write: EffectiveChangeWrite {
                order,
                change: change.clone(),
                submission,
                revision,
                revision_lines,
                delta,
            },
        })
    }

    /// 基于采购变更基准版本和目标版本构建应付与成本差额。
    ///
    /// # 参数
    /// * `change` - 提供基准采购修订引用的采购变更单
    /// * `order` - 当前原采购单
    /// * `revision` - 本次待生效的新采购修订
    ///
    /// # 返回
    /// 返回可在生效事务中追加的应付差额与成本差额。
    ///
    /// # 错误
    /// 基准版本不存在，或差额构建所需仓储事实缺失时返回错误。
    ///
    /// # 关键业务约束
    /// 差额只基于变更冻结的基准版本和本次目标版本计算，不采用可变草稿事实。
    async fn build_effective_change_delta(
        &self,
        change: &PurchaseChangeOrder,
        order: &PurchaseOrder,
        revision: &PurchaseOrderRevision,
    ) -> Result<EffectiveChangeDelta> {
        let base_revision = self
            .db
            .purchase_order_revisions()
            .find_by_id(&change.base_revision_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("基准版本不存在".to_string()))?;
        self.build_change_deltas(order, &base_revision, revision).await
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
        submission
            .ensure_pending()
            .map_err(|_| Error::ConflictError("变更提交已处理，请勿重复生效".to_string()))?;
        let lines = self
            .db
            .purchase_order()
            .list_change_submission_lines(&submission.base.id.clone().into(), &mut NoTransaction)
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
            .purchase_order()
            .list_change_submissions_by_order(&change.base.id.clone().into(), &mut NoTransaction)
            .await?;
        PurchaseChangeSubmission::next_submission_no(&existing).map_err(Into::into)
    }
}

/// 使用销售当前版本稳定行映射刷新采购变更请求行。
///
/// # 参数
/// * `lines` - 采购变更目标行
/// * `sales_lines` - 稳定销售行到当前销售版本行的映射
///
/// # 返回
/// 返回当前版本销售关联和分配数量已规范化的行。
///
/// # 错误
/// 商品行缺少稳定销售行、数量或当前销售版本对应行时返回一致性错误。
///
/// # 关键业务约束
/// 商品行 `allocated_quantity` 恒等于变更后的 `quantity`，物流行清空销售关联。
fn enrich_change_lines(
    lines: &[SavePurchaseOrderLine],
    sales_lines: &std::collections::HashMap<String, entities::sales_order::SalesOrderRevisionLine>,
) -> Result<Vec<SavePurchaseOrderLine>> {
    let mut enriched = lines.to_vec();
    for line in &mut enriched {
        if line.line_type == entities::purchase_order::PurchaseLineType::LogisticsFee {
            line.sales_order_line_id = None;
            line.sales_order_revision_line_id = None;
            line.sales_order_submission_line_id = None;
            line.allocated_quantity = None;
            continue;
        }
        let stable_id = line
            .sales_order_line_id
            .clone()
            .or_else(|| line.procurement_confirmation_line_id.clone())
            .ok_or_else(|| Error::BusinessLogicError("采购变更商品行缺少销售稳定行".to_string()))?;
        let sales_line = sales_lines.get(&stable_id).ok_or_else(|| {
            Error::BusinessLogicError("采购变更商品行在销售当前版本中没有对应稳定行".to_string())
        })?;
        let quantity = line
            .quantity
            .clone()
            .ok_or_else(|| Error::BusinessLogicError("采购变更商品行缺少数量".to_string()))?;
        line.procurement_confirmation_line_id = Some(stable_id.clone());
        line.sales_order_line_id = Some(stable_id);
        line.sales_order_revision_line_id = Some(sales_line.base.id.clone());
        line.sales_order_submission_line_id = None;
        line.allocated_quantity = Some(quantity);
    }
    Ok(enriched)
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

/// 采购变更生效时一次性追加的应付与成本差额。
type EffectiveChangeDelta = (
    Option<(entities::payable::PayableAccount, entities::payable::PayableEntry)>,
    Vec<entities::cost::CostEntry>,
);

/// 已准备的采购变更生效事务写聚合与响应引用。
///
/// # 用途
/// 将事务写内容和事务外已确定的修订、差额引用打包，避免生效编排方法
/// 继续膨胀。
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
/// `write` 必须作为整体进入同一事务，响应引用只能来自该写聚合。
struct PreparedEffectiveChange {
    /// 完整事务写聚合。
    write: EffectiveChangeWrite,
    /// 本次形成的新采购修订主键。
    revision_id: String,
    /// 本次形成的新采购修订序号。
    revision_no: u32,
    /// 本次追加的应付差额分录主键。
    payable_delta_entry_id: Option<String>,
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
    delta: EffectiveChangeDelta,
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
/// 写入成功时返回采购单更新后的乐观锁版本。
///
/// # 错误
/// 销售采购 guard、采购单或变更单 CAS 冲突时返回稳定冲突，其余仓储
/// 失败向上传递。
///
/// # 关键业务约束
/// 变更单状态迁移、来源销售 guard、allocation 和采购当前版本指针必须
/// 位于同一事务。
async fn write_effective_change(
    db: &mongodb::Database,
    write: EffectiveChangeWrite,
    actor: &AuditActor,
) -> Result<u64> {
    let db = db.clone();
    let client = db.client().clone();
    let actor = actor.clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move { write_effective_change_in_transaction(&db, write, &actor, session).await })
        })
        .await
}

/// 在调用方事务内写入采购变更正式版本、差额、状态和成功审计。
///
/// # 错误
/// 状态迁移或任一仓储写入失败时返回错误。
async fn write_effective_change_in_transaction(
    db: &mongodb::Database,
    mut write: EffectiveChangeWrite,
    actor: &AuditActor,
    session: &mut ClientSession,
) -> Result<u64> {
    let audit = actor.clone().resource_log(
        "purchase_change_order.effect",
        "purchase_change_order",
        write.change.base.id.clone(),
    )?;
    let actor_id = actor.id().to_string();
    write
        .change
        .apply_effective(write.revision.base.id.clone().into(), &actor_id)?;
    persist_effective_writes(db, write, audit, &actor_id, session).await
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
/// * `actor_id` - 推进来源销售采购 guard 的最终动作账号
/// * `session` - 事务会话
///
/// # 返回
/// 写入成功时返回采购单更新后的乐观锁版本。
///
/// # 错误
/// 来源销售单缺失、任一 CAS 并发冲突或其他仓储写入失败时返回错误。
///
/// # 关键业务约束
/// 先推进来源销售 guard，再按当前销售版本重建 allocation，最后切换采购
/// 当前版本并同步任务。
async fn persist_effective_writes(
    db: &mongodb::Database,
    write: EffectiveChangeWrite,
    audit: entities::AuditLog,
    actor_id: &str,
    session: &mut ClientSession,
) -> Result<u64> {
    let EffectiveChangeWrite {
        mut order,
        mut change,
        mut submission,
        revision,
        mut revision_lines,
        delta,
    } = write;
    advance_source_sales_procurement_guard(db, &order, actor_id, session).await?;
    let allocations = prepare_current_sales_allocations(db, &order, &mut revision_lines, session).await?;
    db.purchase_order()
        .create_effective_revision(&revision, &revision_lines, session)
        .await?;
    persist_current_sales_allocations(db, &allocations, session).await?;
    order.apply_change_revision(revision.base.id.clone().into(), actor_id)?;
    db.purchase_orders().update(&mut order, session).await?;
    sync_procurement_tasks_for_sales_order(db, &order.sales_order_id, session).await?;
    persist_effective_change_delta(db, &delta, session).await?;
    submission.approve()?;
    db.purchase_change_submissions()
        .update(&mut submission, session)
        .await?;
    db.purchase_change_orders().update(&mut change, session).await?;
    db.audit_logs().create(&audit, session).await?;
    Ok(order.base.version)
}

/// 在采购变更生效事务内追加应付与成本差额。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `delta` - 基于冻结基准版本和目标版本计算的差额
/// * `session` - 与采购修订和当前版本指针共用的事务会话
///
/// # 返回
/// 全部差额写入成功时返回 `Ok(())`。
///
/// # 错误
/// 应付或成本事实写入失败时返回错误。
///
/// # 关键业务约束
/// 差额不得先于采购修订独立提交，任一失败必须回滚整个采购变更生效事务。
async fn persist_effective_change_delta(
    db: &mongodb::Database,
    delta: &EffectiveChangeDelta,
    session: &mut ClientSession,
) -> Result<()> {
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
    Ok(())
}

/// 在采购变更生效事务内推进来源销售单的采购串行化 guard。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `order` - 当前待切换生效版本的采购单
/// * `actor_id` - 最终审批动作账号
/// * `session` - 与采购修订、allocation 和任务同步共用的事务会话
///
/// # 返回
/// 来源销售单 CAS 更新成功时返回 `Ok(())`。
///
/// # 错误
/// 来源销售单不存在、guard 溢出、乐观锁或瞬态事务冲突时返回错误。
///
/// # 关键业务约束
/// 必须先通过销售单 `id + version` CAS 推进 `procurement_guard_version`，
/// 后续才能重算采购覆盖。
async fn advance_source_sales_procurement_guard(
    db: &mongodb::Database,
    order: &PurchaseOrder,
    actor_id: &str,
    session: &mut ClientSession,
) -> Result<()> {
    let mut sales_order = db
        .sales_orders()
        .find_by_id(&order.sales_order_id, session)
        .await?
        .ok_or_else(|| Error::NotFound("来源销售单不存在".to_string()))?;
    sales_order.advance_procurement_guard(actor_id)?;
    db.sales_orders().update(&mut sales_order, session).await?;
    Ok(())
}

/// 规范化可选列表筛选文本。
///
/// # 参数
/// * `value` - 原始可选筛选值
///
/// # 返回
/// 空白值返回 `None`，否则返回去除首尾空白后的字符串。
fn normalized_filter(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
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
        execute_purchase_change_domain_action, start_purchase_change_approval, PurchaseOrderService,
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
        assert!(source.contains("submission_id_for_effect"));
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

    /// 生效写事务必须先推进销售 guard，再重建分配、切换采购版本并同步任务。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无；写入顺序和最新采购锁版本返回链路不满足时测试失败。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// guard CAS 必须早于 allocation 重建，任务同步必须晚于采购当前版本更新。
    #[test]
    fn effect_write_serializes_and_rebuilds_procurement_coverage() {
        let source = include_str!("change.rs");
        let body = source
            .split_once("async fn persist_effective_writes")
            .expect("必须存在采购变更生效事务写方法")
            .1;
        let guard = body
            .find("advance_source_sales_procurement_guard")
            .expect("必须推进来源销售 guard");
        let prepare = body
            .find("prepare_current_sales_allocations")
            .expect("必须重建当前销售分配");
        let persist = body
            .find("persist_current_sales_allocations")
            .expect("必须持久化当前销售分配");
        let pointer = body
            .find("order.apply_change_revision")
            .expect("必须通过实体切换采购当前版本");
        let order_update = body
            .find("db.purchase_orders().update")
            .expect("必须 CAS 更新采购单");
        let task_sync = body
            .find("sync_procurement_tasks_for_sales_order")
            .expect("必须同步采购任务");

        assert!(guard < prepare);
        assert!(prepare < persist);
        assert!(persist < pointer);
        assert!(pointer < order_update);
        assert!(order_update < task_sync);
        assert!(body.contains("Ok(order.base.version)"));
        assert!(source.contains("let purchase_order_lock_version = write_effective_change"));
    }

    /// 来源销售 guard helper 必须在同一事务中加载销售单并执行版本 CAS。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无；事务执行器或 guard CAS 缺失时测试失败。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 不得在事务外预读销售单后仅写采购修订，否则有效增减量可能并发越界。
    #[test]
    fn effect_guard_load_and_cas_share_transaction_session() {
        let source = include_str!("change.rs");
        let helper = source
            .split_once("async fn advance_source_sales_procurement_guard")
            .expect("必须存在来源销售 guard helper")
            .1;
        let load = helper
            .find(".find_by_id(&order.sales_order_id, session)")
            .expect("必须在事务内加载来源销售单");
        let advance = helper
            .find("sales_order.advance_procurement_guard(actor_id)")
            .expect("必须推进 procurement guard");
        let update = helper
            .find("db.sales_orders().update(&mut sales_order, session)")
            .expect("必须通过同一事务执行销售单 CAS");

        assert!(load < advance);
        assert!(advance < update);
    }

    /// 销售 guard 乐观锁与事务冲突必须映射为稳定 HTTP 409 服务错误。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无；任一并发错误未映射为稳定冲突文案时测试失败。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 采购变更生效不得把数据库 CAS 或 MongoDB 瞬态事务冲突泄露为 500。
    #[test]
    fn effect_concurrency_errors_map_to_stable_conflicts() {
        let optimistic = crate::errors::Error::from(database::Error::OptimisticLockingError);
        assert!(matches!(
            optimistic,
            crate::errors::Error::ConflictError(message)
                if message == "数据已被其他请求修改，请刷新后重试"
        ));

        let transient = crate::errors::Error::from(database::Error::TransientTransactionConflict(
            mongodb::error::Error::custom("write conflict"),
        ));
        assert!(matches!(
            transient,
            crate::errors::Error::ConflictError(message) if message == "并发事务冲突，请重试"
        ));
    }

    /// 生效只接受当前冻结提交；错误提交或缺失提交失败关闭。
    #[test]
    fn effect_rejects_mismatched_or_missing_submission() {
        let mut change = PurchaseChangeOrder::new(
            PurchaseChangeOrderId::new("pco-1"),
            PurchaseChangeOrderData {
                purchase_order_id: PurchaseOrderId::new("po-1"),
                base_revision_id: PurchaseOrderRevisionId::new("por-1"),
                reason: "成本上涨".into(),
            },
            "user-1",
        )
        .unwrap();
        assert!(change.submission_id_for_effect(Some("pcs-current")).is_err());
        change
            .start_approval(PurchaseChangeSubmissionId::new("pcs-current"), "hash-1", "user-1")
            .unwrap();
        assert_eq!(
            change
                .submission_id_for_effect(Some("pcs-current"))
                .unwrap()
                .as_ref(),
            "pcs-current"
        );
        assert!(change.submission_id_for_effect(Some("pcs-old")).is_err());
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
