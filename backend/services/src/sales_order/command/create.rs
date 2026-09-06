use database::{AccessControlExt, ContractExt, CustomerExt, DocumentRegistryExt, SalesOrderExt};
use entities::document_registry::{
    BusinessDocument, BusinessDocumentData, WorkflowAction, WorkflowActionData, WorkflowActionType,
};
use entities::sales_order::{SalesOrder, SalesOrderData};
use erp_core::common::time::Instant;
use erp_core::ids::{BusinessDocumentId, ContractId, CustomerAccountId, SalesOrderId, WorkflowActionId};
use id_generator::next_id;
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use super::super::adapter::{
    build_sales_order_snapshot, execute_sales_order_domain_action, sales_approval_ports,
    sales_order_object_readable, sales_order_responsible_org_id, sales_order_start_command,
    start_approval_command_kind, RECENT_HISTORY_LIMIT,
};
use super::super::dto::{
    CreateSalesOrderRequest, SalesOrderCreateIntent, SalesOrderDetailView, SalesOrderDraftRequest,
    SalesOrderEditableDraftRequest,
};
use super::super::mapper::{
    build_stable_lines, build_submission, build_submission_lines, build_working_copy,
};
use super::super::start_approval::{
    build_sales_order_start_input, load_bound_definition_graph_with_executor, persist_runtime_writes,
    SalesOrderRuntimeWriteInput, SalesOrderStartInput,
};
use super::super::SalesOrderService;
use super::identity::{
    persist_bound_sales_document, sales_create_bind_command, sales_order_create_audit_id,
    sales_order_create_fingerprint, sales_submission_audit_id,
};
use super::submit::ensure_unified_start_command;
use crate::approval::execution::prepare_start;
use crate::audit::AuditActorLogs;
use crate::errors::{Error, Result};
use application_core::AuditActor;

impl SalesOrderService {
    /// 解析销售命令所选合同的客户身份，供 HTTP 层执行客户数据范围校验。
    ///
    /// # 参数
    /// * `contract_id` - 前端选择的合同稳定身份
    ///
    /// # 返回
    /// 返回合同所属客户身份。
    ///
    /// # 错误
    /// 合同不存在时返回 `NotFound`。
    #[tracing::instrument(
        name = "sales_order.resolve_customer_scope",
        skip_all,
        fields(
            layer = "service",
            domain = "sales_order",
            operation = "resolve_customer_scope"
        )
    )]
    pub async fn sales_command_customer_id(&self, contract_id: &ContractId) -> Result<CustomerAccountId> {
        let contract = self
            .db
            .contracts()
            .find_by_id(contract_id.as_ref(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("合同不存在".to_string()))?;
        Ok(contract.customer_id)
    }

    /// 按当前有效合同修订补齐不可由客户端声明的销售草稿快照。
    ///
    /// # 参数
    /// * `contract_id` - 合同稳定身份
    /// * `editable` - 客户端可编辑字段与行
    ///
    /// # 返回
    /// 返回合同所属客户、结算主体与完整内部草稿。
    ///
    /// # 错误
    /// 合同或修订不存在、合同非生效态、所选修订已过期时返回错误。
    pub(super) async fn resolve_sales_command_draft(
        &self,
        contract_id: &ContractId,
        editable: SalesOrderEditableDraftRequest,
    ) -> Result<(CustomerAccountId, erp_core::ids::PartyId, SalesOrderDraftRequest)> {
        editable.validate()?;
        let contract = self
            .db
            .contracts()
            .find_by_id(contract_id.as_ref(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("合同不存在".to_string()))?;
        if contract.stable.status != entities::contract::ContractStatus::Effective {
            return Err(Error::BusinessLogicError(
                "合同当前不可用于新销售提交".to_string(),
            ));
        }
        if contract.stable.current_revision_id.as_deref()
            != Some(editable.requested_contract_revision_id.as_ref())
        {
            return Err(Error::ConflictError(
                "所选合同版本已不是当前可用版本，请刷新后重新选择".to_string(),
            ));
        }
        let revision = self
            .db
            .contract_revisions()
            .find_by_id(
                editable.requested_contract_revision_id.as_ref(),
                &mut NoTransaction,
            )
            .await?
            .ok_or_else(|| Error::NotFound("合同版本不存在".to_string()))?;
        if !revision.belongs_to_contract(contract_id) {
            return Err(Error::ValidationError("合同版本不属于所选合同".to_string()));
        }
        if !revision.matches_settlement_party(&contract.settlement_party_id) {
            return Err(Error::ConflictError(
                "合同当前结算主体与所选版本不一致，请刷新后重试".to_string(),
            ));
        }
        let draft = SalesOrderDraftRequest {
            editor_user_id: editable.editor_user_id,
            customer_name: revision.customer_snapshot.customer_name,
            contract_no: Some(revision.contract_no),
            requested_contract_revision_id: Some(editable.requested_contract_revision_id),
            settlement_party_name: Some(revision.settlement_party_snapshot.settlement_party_name),
            payment_term_code: revision.payment_term_snapshot.payment_term_code,
            payment_term_name: revision.payment_term_snapshot.payment_term_name,
            invoice_type: revision.invoice_requirement_snapshot.invoice_type,
            tax_point: revision.invoice_requirement_snapshot.tax_point,
            project_name: editable.project_name,
            business_remark: editable.business_remark,
            voucher_category_sku_id: editable.voucher_category_sku_id,
            voucher_expiry_at: editable.voucher_expiry_at,
            receivable_due_date: editable.receivable_due_date,
            lines: editable.lines,
        };
        draft.validate()?;
        Ok((contract.customer_id, contract.settlement_party_id, draft))
    }

    /// 创建销售单（订单 + 稳定明细 + 首次提交工作副本原子形成；`intent=SUBMIT`
    /// 时随后立即提交）。
    ///
    /// 表头金额三元组由服务端按 §4.2 铁律 2 汇总**已舍入**的行金额，客户端不可
    /// 指定；跨域校验客户（D08）与合同（D12）存在性。同一操作人使用同一幂等键
    /// 和完整载荷重试时返回原销售单；同一幂等键绑定不同载荷时返回 409。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回销售单详情视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败或行字段组缺失
    /// * `NotFound` - 客户/合同不存在
    /// * `BusinessLogicError` - 客户已停用
    /// * `ConflictError` - order_no 重复
    #[tracing::instrument(
        name = "sales_order.create",
        skip_all,
        fields(layer = "service", domain = "sales_order", operation = "create")
    )]
    pub async fn create_sales_order(
        &self,
        mut req: CreateSalesOrderRequest,
        actor: &AuditActor,
    ) -> Result<SalesOrderDetailView> {
        req.validate()?;
        let idempotency_key = req.idempotency_key.trim().to_string();
        if idempotency_key.is_empty() {
            return Err(Error::ValidationError("幂等键不能为空".to_string()));
        }
        req.idempotency_key.clone_from(&idempotency_key);
        let audit_id = sales_order_create_audit_id(actor.id(), &idempotency_key);
        let fingerprint = sales_order_create_fingerprint(actor.id(), &req)?;
        if let Some(order_id) = self
            .replay_sales_order_creation(&audit_id, &fingerprint, actor.id())
            .await?
        {
            return self.sales_order_detail(&order_id, None).await;
        }
        let (customer_id, settlement_party_id, draft) = self
            .resolve_sales_command_draft(&req.contract_id, req.draft.clone())
            .await?;
        self.ensure_sellable_draft_lines(&draft.lines).await?;
        let customer = self
            .db
            .customer_accounts()
            .find_by_id(&customer_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("客户不存在".to_string()))?;
        if !customer.is_active() {
            return Err(Error::BusinessLogicError(
                "客户已停用，禁止创建新销售单".to_string(),
            ));
        }

        let order = SalesOrder::new(
            SalesOrderId::new(next_id()),
            SalesOrderData {
                order_no: req.order_no,
                business_type: req.business_type,
                origin_system: entities::sales_order::OriginSystem::Erp,
                source_identity_id: None,
                customer_id,
                contract_id: Some(req.contract_id.clone()),
                settlement_party_id,
                source_status_code: None,
            },
            actor.id(),
        )?;
        let order_id = SalesOrderId::new(order.base.id.clone());
        let document_type =
            entities::approval_integration::document_type_of_sales_business(req.business_type);
        let document = BusinessDocument::new(
            BusinessDocumentId::new(order.base.id.clone()),
            BusinessDocumentData {
                document_type,
                document_no: order.order_no.clone(),
            },
        )?;
        let stable_lines = build_stable_lines(&order_id, &draft.lines)?;
        let (working_copy, working_copy_lines) = build_working_copy(&order, &stable_lines, &draft, 1, actor)?;

        if req.intent == SalesOrderCreateIntent::Submit {
            let ports = sales_approval_ports(order.business_type)?;
            let subject = entities::approval_integration::subject_ref_for_sales_business(
                order.business_type,
                &order.base.id,
            )
            .map_err(|error| Error::ValidationError(error.to_string()))?;
            let organization_id = sales_order_responsible_org_id(&order)?;
            let _ = sales_order_object_readable(&organization_id, actor.id())?;
            self.ensure_procurement_responsibility_before_submit(&order, &working_copy_lines)
                .await?;
            let submission = build_submission(&working_copy, &working_copy_lines, 1, actor)?;
            let submission_lines = build_submission_lines(&submission, &working_copy_lines)?;
            let mut submitted_working_copy = working_copy;
            submitted_working_copy.submit()?;
            let mut submitted_order = order.clone();
            execute_sales_order_domain_action(&mut submitted_order, ports.on_approval_start, actor.id())?;
            let now = Instant::now();
            let snapshot = build_sales_order_snapshot(
                &submitted_order,
                &submission,
                &submission_lines,
                actor.id(),
                now,
            )?;
            let start = sales_order_start_command(
                ports.document_type,
                &submitted_order.base.id,
                submission.submission_no,
                actor.id(),
                &idempotency_key,
            );
            ensure_unified_start_command(&start)?;
            let _ = (start_approval_command_kind(&start), RECENT_HISTORY_LIMIT);
            let workflow_action = WorkflowAction::new(
                WorkflowActionId::new(next_id()),
                WorkflowActionData {
                    document_id: BusinessDocumentId::new(submitted_order.base.id.clone()),
                    action_type: WorkflowActionType::Submit,
                    from_status: "DRAFT".to_string(),
                    to_status: "PENDING_REVIEW".to_string(),
                    actor_id: actor.id().to_string(),
                    actor_role: "role-sales".to_string(),
                    comment: None,
                },
            )?;
            let create_audit = actor.clone().resource_log_with_id(
                audit_id.clone(),
                "sales_order.create",
                "sales_order",
                submitted_order.base.id.clone(),
                Some(format!("command_sha256={fingerprint}")),
            )?;
            let submit_audit = actor.clone().resource_log_with_id(
                sales_submission_audit_id(actor.id(), &submitted_order.base.id, &idempotency_key),
                "sales_order.submit",
                "sales_order_submission",
                submission.base.id.clone(),
                Some(format!("command_sha256=create:{fingerprint}")),
            )?;
            let bind_command = sales_create_bind_command(&submitted_order, actor)?;
            let rbac = self.require_rbac().cloned()?;
            let sellable_refs = Self::sellable_working_copy_refs(&working_copy_lines)?;
            let detail_id = submitted_order.base.id.clone();
            let db = self.db.clone();
            let client = db.client().clone();
            let actor_owned = actor.clone();
            let mut document = document;
            let transaction_result = client
                .with_transaction(move |session| {
                    Box::pin(async move {
                        SalesOrderService::new(db.clone())
                            .ensure_sellable_refs(&sellable_refs, session)
                            .await?;
                        let binding = persist_bound_sales_document(
                            &db,
                            &rbac,
                            &mut document,
                            &bind_command,
                            &actor_owned,
                            session,
                        )
                        .await?;
                        let graph = load_bound_definition_graph_with_executor(&db, &binding, session).await?;
                        let start_input = build_sales_order_start_input(SalesOrderStartInput {
                            graph,
                            binding: &binding,
                            document_type: ports.document_type,
                            subject,
                            subject_version: submission.submission_no,
                            actor_id: actor_owned.id(),
                            organization_id: &organization_id,
                            idempotency_key: &idempotency_key,
                            receipt: None,
                            now,
                        })?;
                        let prepared = prepare_start(start_input)?;
                        db.sales_orders().create(&submitted_order, session).await?;
                        for line in &stable_lines {
                            db.sales_order_lines().create(line, session).await?;
                        }
                        db.sales_order_working_copies()
                            .create(&submitted_working_copy, session)
                            .await?;
                        for line in &working_copy_lines {
                            db.sales_order_working_copy_lines().create(line, session).await?;
                        }
                        db.sales_order_submissions().create(&submission, session).await?;
                        for line in &submission_lines {
                            db.sales_order_submission_lines().create(line, session).await?;
                        }
                        db.workflow_actions().create(&workflow_action, session).await?;
                        if let crate::approval::execution::PreparedExecution::Apply(writes) = prepared {
                            persist_runtime_writes(
                                &db,
                                &writes,
                                SalesOrderRuntimeWriteInput {
                                    document_type: ports.document_type,
                                    snapshot_payload: &snapshot,
                                    owner_role: ports.owner_role,
                                    organization_id: &organization_id,
                                    now,
                                },
                                session,
                            )
                            .await?;
                        }
                        db.audit_logs().create(&create_audit, session).await?;
                        db.audit_logs().create(&submit_audit, session).await?;
                        Ok::<(), crate::errors::Error>(())
                    })
                })
                .await;
            if let Err(error) = transaction_result {
                if let Some(order_id) = self
                    .replay_sales_order_creation(&audit_id, &fingerprint, actor.id())
                    .await?
                {
                    return self.sales_order_detail(&order_id, None).await;
                }
                return Err(error);
            }
            return self.sales_order_detail(&detail_id, None).await;
        }

        let audit = actor.clone().resource_log_with_id(
            audit_id.clone(),
            "sales_order.create",
            "sales_order",
            order.base.id.clone(),
            Some(format!("command_sha256={fingerprint}")),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let order_for_tx = order.clone();
        let lines_for_tx = stable_lines.clone();
        let working_copy_for_tx = working_copy.clone();
        let working_copy_lines_for_tx = working_copy_lines.clone();
        let mut document_for_tx = document;
        let bind_command = sales_create_bind_command(&order, actor)?;
        let rbac_for_tx = self.require_rbac().cloned()?;
        let actor_for_tx = actor.clone();
        let sellable_refs_for_tx = Self::sellable_working_copy_refs(&working_copy_lines)?;
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    SalesOrderService::new(db.clone())
                        .ensure_sellable_refs(&sellable_refs_for_tx, session)
                        .await?;
                    db.sales_orders().create(&order_for_tx, session).await?;
                    persist_bound_sales_document(
                        &db,
                        &rbac_for_tx,
                        &mut document_for_tx,
                        &bind_command,
                        &actor_for_tx,
                        session,
                    )
                    .await?;
                    for line in &lines_for_tx {
                        db.sales_order_lines().create(line, session).await?;
                    }
                    db.sales_order_working_copies()
                        .create(&working_copy_for_tx, session)
                        .await?;
                    for line in &working_copy_lines_for_tx {
                        db.sales_order_working_copy_lines().create(line, session).await?;
                    }
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await;
        if let Err(error) = transaction_result {
            if let Some(order_id) = self
                .replay_sales_order_creation(&audit_id, &fingerprint, actor.id())
                .await?
            {
                return self.sales_order_detail(&order_id, None).await;
            }
            return Err(error);
        }

        self.sales_order_detail(&order.base.id, None).await
    }

    /// 按稳定审计收据回读已创建的销售单身份。
    async fn replay_sales_order_creation(
        &self,
        audit_id: &str,
        expected_fingerprint: &str,
        actor_id: &str,
    ) -> Result<Option<String>> {
        let Some(audit) = self
            .db
            .audit_logs()
            .find_by_id(audit_id, &mut NoTransaction)
            .await?
        else {
            return Ok(None);
        };
        if audit.action != "sales_order.create"
            || audit.resource_type != "sales_order"
            || audit.actor_id != actor_id
        {
            return Err(Error::Internal("销售建单幂等收据身份不一致".to_string()));
        }
        if audit.message.as_deref() != Some(&format!("command_sha256={expected_fingerprint}")) {
            return Err(Error::ConflictError(
                "同一幂等键已用于不同的销售建单命令".to_string(),
            ));
        }
        let order_id = audit
            .resource_id
            .ok_or_else(|| Error::Internal("销售建单幂等收据缺少结果引用".to_string()))?;
        let order = self
            .db
            .sales_orders()
            .find_by_id(&order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("销售建单幂等收据对应销售单缺失".to_string()))?;
        if order.stable.created_by != actor_id {
            return Err(Error::Internal("销售建单幂等收据与创建人不一致".to_string()));
        }
        Ok(Some(order_id))
    }
}
