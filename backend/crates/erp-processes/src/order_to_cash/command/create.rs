use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_contract::ContractExt;
use erp_core::common::time::Instant;
use erp_core::ids::{BusinessDocumentId, ContractId, CustomerAccountId, SalesOrderId, WorkflowActionId};
use erp_read_models::sales_center::order::dto::SalesOrderDetailView;
use erp_sales::dto::sales_order::{
    CreateSalesOrderRequest, SalesOrderCreateIntent, SalesOrderDraftRequest, SalesOrderEditableDraftRequest,
};
use erp_sales::repository::SalesOrderExt;
use erp_sales::service::sales_order::command::identity::{
    sales_order_create_audit_id, sales_order_create_fingerprint, sales_submission_audit_id,
};
use erp_sales::service::sales_order::mapper::{
    build_stable_lines, build_submission, build_submission_lines, build_working_copy,
};
use erp_workflow::DocumentRegistryExt;
use erp_workflow::entity::document_registry::{
    BusinessDocument, BusinessDocumentData, WorkflowAction, WorkflowActionData, WorkflowActionType,
};
use erp_workflow::service::approval::execution::prepare_start;
use id_generator::next_id;
use persistence_core::{Executor, NoTransaction, Transactional};
use validator::Validate;

use super::super::SalesOrderCommandProcess;
use super::super::adapter::{
    RECENT_HISTORY_LIMIT, build_sales_order_snapshot, execute_sales_order_domain_action,
    sales_approval_ports, sales_order_object_readable, sales_order_responsible_org_id,
    sales_order_start_command, start_approval_command_kind,
};
use super::super::start_approval::{
    SalesOrderRuntimeWriteInput, SalesOrderStartInput, build_sales_order_start_input,
    load_bound_definition_graph_with_executor, persist_runtime_writes,
};
use super::identity::{persist_bound_sales_document, sales_create_bind_command};
use super::submit::ensure_unified_start_command;
use crate::{Error, Result};

impl SalesOrderCommandProcess {
    /// 解析销售命令所选合同的客户身份，供 HTTP 层执行客户数据范围校验。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `contract_id` - 前端选择的合同稳定身份
    ///
    /// # 返回
    /// 返回合同所属客户身份。
    ///
    /// # 错误
    /// 合同不存在或不可见时返回 `NotFound`。
    ///
    /// # 关键业务约束
    /// 本入口只做事前检查；写入事务仍须 `related` 重验，不得当作唯一凭证。
    #[tracing::instrument(
        name = "sales_order.resolve_customer_scope",
        skip_all,
        fields(layer = "service", domain = "sales_order", operation = "resolve_customer_scope")
    )]
    pub async fn sales_command_customer_id(
        &self,
        actor: &AuditActor,
        contract_id: &ContractId,
    ) -> Result<CustomerAccountId> {
        let access = self.command_access(actor, "detail")?;
        let contract = access.load_contract(contract_id.as_ref(), &mut NoTransaction).await?;
        Ok(contract.customer_id)
    }

    /// 按当前有效合同修订补齐不可由客户端声明的销售草稿快照。
    ///
    /// # 参数
    /// * `access` - 已构造的命令检查器，用于合同／客户 detail 重验
    /// * `contract_id` - 合同稳定身份
    /// * `editable` - 客户端可编辑字段与行
    /// * `executor` - 调用方执行器；预装载可用 `NoTransaction`，不得代替写入事务重验
    ///
    /// # 返回
    /// 返回合同所属客户、结算主体与完整内部草稿。
    ///
    /// # 错误
    /// 合同或客户不可见、合同或修订不存在、合同非生效态、所选修订已过期时返回错误。
    ///
    /// # 关键业务约束
    /// 禁止以无范围 `find_by_id` 作为授权读取；写入事务必须再次 `related`。
    pub(super) async fn resolve_sales_command_draft(
        &self,
        access: &super::super::authorization::SalesCommandAccess,
        contract_id: &ContractId,
        editable: SalesOrderEditableDraftRequest,
        executor: &mut dyn Executor,
    ) -> Result<(CustomerAccountId, erp_core::ids::PartyId, SalesOrderDraftRequest)> {
        editable.validate()?;
        let contract = access.load_contract(contract_id.as_ref(), executor).await?;
        if contract.stable.status != erp_contract::ContractStatus::Effective {
            return Err(Error::BusinessLogicError("合同当前不可用于新销售提交".to_string()));
        }
        if contract.stable.current_revision_id.as_deref()
            != Some(editable.requested_contract_revision_id.as_ref())
        {
            return Err(Error::ConflictError("所选合同版本已不是当前可用版本，请刷新后重新选择".to_string()));
        }
        let revision = self
            .db
            .contract_revisions()
            .find_by_id(editable.requested_contract_revision_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::NotFound("合同版本不存在".to_string()))?;
        if !revision.belongs_to_contract(contract_id) {
            return Err(Error::ValidationError("合同版本不属于所选合同".to_string()));
        }
        if !revision.matches_settlement_party(&contract.settlement_party_id) {
            return Err(Error::ConflictError("合同当前结算主体与所选版本不一致，请刷新后重试".to_string()));
        }
        let customer = access.load_customer(contract.customer_id.as_ref(), executor).await?;
        if !customer.is_active() {
            return Err(Error::BusinessLogicError("客户已停用，禁止创建新销售单".to_string()));
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
        let access = self
            .command_access(actor, "create")?
            .require_submit(req.intent == SalesOrderCreateIntent::Submit)?;
        let idempotency_key = req.idempotency_key.trim().to_string();
        if idempotency_key.is_empty() {
            return Err(Error::ValidationError("幂等键不能为空".to_string()));
        }
        req.idempotency_key.clone_from(&idempotency_key);
        let audit_id = sales_order_create_audit_id(actor.id(), &idempotency_key);
        let fingerprint = sales_order_create_fingerprint(actor.id(), &req)?;
        if let Some(order_id) =
            self.replay_sales_order_creation(&audit_id, &fingerprint, actor.id(), &access).await?
        {
            return self.read_model().sales_order_detail(&order_id, None).await.map_err(crate::Error::from);
        }
        let (customer_id, settlement_party_id, draft) = self
            .resolve_sales_command_draft(&access, &req.contract_id, req.draft.clone(), &mut NoTransaction)
            .await?;
        self.sales().ensure_sellable_draft_lines(&draft.lines, &self.catalog()).await?;

        let business_org_unit_id =
            crate::business_ownership::required_business_org(&self.db, actor.id(), &mut NoTransaction)
                .await?;
        let order = erp_sales::service::sales_order::SalesOrderService::prepare_order(
            &req,
            customer_id,
            settlement_party_id,
            actor,
            business_org_unit_id,
        )?;
        let order_id = SalesOrderId::new(order.base.id.clone());
        let document_type = crate::order_to_cash::document_type_of_sales_business(req.business_type);
        let document = BusinessDocument::new(
            BusinessDocumentId::new(order.base.id.clone()),
            BusinessDocumentData { document_type, document_no: order.order_no.clone() },
        )?;
        let stable_lines = build_stable_lines(&order_id, &draft.lines)?;
        let (working_copy, working_copy_lines) = build_working_copy(&order, &stable_lines, &draft, 1, actor)?;

        if req.intent == SalesOrderCreateIntent::Submit {
            let ports = sales_approval_ports(order.business_type)?;
            let subject =
                crate::order_to_cash::subject_ref_for_sales_business(order.business_type, &order.base.id)
                    .map_err(|error| Error::ValidationError(error.to_string()))?;
            let organization_id = sales_order_responsible_org_id(&order)?;
            let _ = sales_order_object_readable(&organization_id, actor.id())?;
            self.ensure_procurement_responsibility_before_submit(&order, &working_copy_lines).await?;
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
            let object_read = std::sync::Arc::clone(&self.object_read);
            let sellable_refs =
                erp_sales::service::sales_order::SalesOrderService::sellable_working_copy_refs(
                    &working_copy_lines,
                )?;
            let detail_id = submitted_order.base.id.clone();
            let db = self.db.clone();
            let client = db.client().clone();
            let actor_owned = actor.clone();
            let mut document = document;
            let access_for_tx = access.clone();
            let transaction_result = client
                .with_transaction(move |executor| {
                    Box::pin(async move {
                        access_for_tx.related_order(&submitted_order, executor).await?;
                        access_for_tx.creation(&submitted_order, executor).await?;
                        erp_sales::service::sales_order::SalesOrderService::new(db.clone())
                            .ensure_sellable_refs(
                                &sellable_refs,
                                &crate::order_to_cash::adapters::catalog::CatalogQualificationAdapter::new(
                                    db.clone(),
                                ),
                                executor,
                            )
                            .await?;
                        let binding = persist_bound_sales_document(
                            &db,
                            &rbac,
                            object_read.as_ref(),
                            &mut document,
                            &bind_command,
                            &actor_owned,
                            executor,
                        )
                        .await?;
                        let graph =
                            load_bound_definition_graph_with_executor(&db, &binding, executor).await?;
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
                        crate::business_ownership::ensure_creation_org(
                            &db,
                            &submitted_order.sales_owner_user_id,
                            &submitted_order.business_org_unit_id,
                            executor,
                        )
                        .await?;
                        erp_sales::service::sales_order::SalesOrderService::new(db.clone())
                            .create_order(&submitted_order, executor)
                            .await?;
                        let sales = erp_sales::service::sales_order::SalesOrderService::new(db.clone());
                        sales
                            .create_working_copy(
                                &stable_lines,
                                &submitted_working_copy,
                                &working_copy_lines,
                                executor,
                            )
                            .await?;
                        sales.create_submission(&submission, &submission_lines, executor).await?;
                        db.workflow_actions().create(&workflow_action, executor).await?;
                        if let erp_workflow::service::approval::execution::PreparedExecution::Apply(writes) =
                            prepared
                        {
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
                                executor,
                            )
                            .await?;
                        }
                        db.audit_logs().create(&create_audit, executor).await?;
                        db.audit_logs().create(&submit_audit, executor).await?;
                        Ok::<(), crate::Error>(())
                    })
                })
                .await;
            if let Err(error) = transaction_result {
                if let Some(order_id) =
                    self.replay_sales_order_creation(&audit_id, &fingerprint, actor.id(), &access).await?
                {
                    return self
                        .read_model()
                        .sales_order_detail(&order_id, None)
                        .await
                        .map_err(crate::Error::from);
                }
                return Err(error);
            }
            return self.read_model().sales_order_detail(&detail_id, None).await.map_err(crate::Error::from);
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
        let object_read = std::sync::Arc::clone(&self.object_read);
        let actor_for_tx = actor.clone();
        let sellable_refs_for_tx =
            erp_sales::service::sales_order::SalesOrderService::sellable_working_copy_refs(
                &working_copy_lines,
            )?;
        let access_for_tx = access.clone();
        let transaction_result = client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    access_for_tx.related_order(&order_for_tx, executor).await?;
                    access_for_tx.creation(&order_for_tx, executor).await?;
                    erp_sales::service::sales_order::SalesOrderService::new(db.clone())
                        .ensure_sellable_refs(
                            &sellable_refs_for_tx,
                            &crate::order_to_cash::adapters::catalog::CatalogQualificationAdapter::new(
                                db.clone(),
                            ),
                            executor,
                        )
                        .await?;
                    crate::business_ownership::ensure_creation_org(
                        &db,
                        &order_for_tx.sales_owner_user_id,
                        &order_for_tx.business_org_unit_id,
                        executor,
                    )
                    .await?;
                    erp_sales::service::sales_order::SalesOrderService::new(db.clone())
                        .create_order(&order_for_tx, executor)
                        .await?;
                    persist_bound_sales_document(
                        &db,
                        &rbac_for_tx,
                        object_read.as_ref(),
                        &mut document_for_tx,
                        &bind_command,
                        &actor_for_tx,
                        executor,
                    )
                    .await?;
                    erp_sales::service::sales_order::SalesOrderService::new(db.clone())
                        .create_working_copy(
                            &lines_for_tx,
                            &working_copy_for_tx,
                            &working_copy_lines_for_tx,
                            executor,
                        )
                        .await?;
                    db.audit_logs().create(&audit, executor).await?;
                    Ok::<(), crate::Error>(())
                })
            })
            .await;
        if let Err(error) = transaction_result {
            if let Some(order_id) =
                self.replay_sales_order_creation(&audit_id, &fingerprint, actor.id(), &access).await?
            {
                return self
                    .read_model()
                    .sales_order_detail(&order_id, None)
                    .await
                    .map_err(crate::Error::from);
            }
            return Err(error);
        }

        self.read_model().sales_order_detail(&order.base.id, None).await.map_err(crate::Error::from)
    }

    /// 按稳定审计收据回读已创建的销售单身份。
    async fn replay_sales_order_creation(
        &self,
        audit_id: &str,
        expected_fingerprint: &str,
        actor_id: &str,
        access: &super::super::authorization::SalesCommandAccess,
    ) -> Result<Option<String>> {
        let Some(audit) = self.db.audit_logs().find_by_id(audit_id, &mut NoTransaction).await? else {
            return Ok(None);
        };
        if audit.action != "sales_order.create"
            || audit.resource_type != "sales_order"
            || audit.actor_id != actor_id
        {
            return Err(Error::Internal("销售建单幂等收据身份不一致".to_string()));
        }
        if audit.message.as_deref() != Some(&format!("command_sha256={expected_fingerprint}")) {
            return Err(Error::ConflictError("同一幂等键已用于不同的销售建单命令".to_string()));
        }
        let order_id =
            audit.resource_id.ok_or_else(|| Error::Internal("销售建单幂等收据缺少结果引用".to_string()))?;
        let order = self
            .db
            .sales_orders()
            .find_by_id(&order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("销售建单幂等收据对应销售单缺失".to_string()))?;
        if order.stable.created_by != actor_id {
            return Err(Error::Internal("销售建单幂等收据与创建人不一致".to_string()));
        }
        let access = access.clone();
        let check_id = order_id.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { access.current(&check_id, executor).await })
            })
            .await?;
        Ok(Some(order_id))
    }
}
