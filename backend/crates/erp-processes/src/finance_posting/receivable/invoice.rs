//! 发票列表、详情、草稿创建、销项提交与过账编排。

use erp_finance::repository::ReceivableExt;

use erp_audit::AuditExt;
use erp_core::ids::InvoiceId;
use erp_finance::entity::receivable::{Invoice, InvoiceData, InvoiceStatus};

use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::entity::document_registry::{BusinessDocument, DocumentType};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::dto::{CommitInvoiceRequest, CreateInvoiceRequest, InvoiceView, PostInvoiceRequest};
use super::ReceivableProcess;
use crate::{Error, Result};
use application_core::AuditActor;
use application_core::CommandReceipt;
use erp_audit::AuditActorLogs;
use erp_audit::CommandReceiptServiceExt as _;
use erp_finance::service::receivable::invoice_commit::{
    convert_post_allocations, ensure_sales_invoice, PreparedInvoiceCommit,
};
use erp_identity::SharedRbacService;
use erp_read_models::finance::receivable::snapshot::{ensure_expected_version, zero_amount};
use erp_workflow::service::approval::binding::{
    binding_decision, BindPublishedDefinitionCommand, BindingDecision,
};
use erp_workflow::service::approval::business_adapter::{adapter_spec_of, BindingRevalidationContext};
use erp_workflow::service::approval::policy::{policy_of, DocumentApprovalPolicy};
use erp_workflow::service::document_registry::{new_registered_document, persist_registered_document};

impl ReceivableProcess {
    // -----------------------------------------------------------------------
    // 发票
    // -----------------------------------------------------------------------

    /// 登记发票草稿：同一事务注册 `BusinessDocument` 并调用统一绑定端口。
    ///
    /// 发票为 `NO_APPROVAL`：返回空绑定，不查询已发布定义，不启动审批实例，
    /// 不创建审批任务。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建发票视图。
    ///
    /// # 错误
    /// * `ValidationError` - 金额三元组不恒等或字段非法
    pub async fn create_invoice(&self, req: CreateInvoiceRequest, actor: &AuditActor) -> Result<InvoiceView> {
        req.validate()?;
        let invoice = Invoice::new(
            InvoiceId::new(next_id()),
            InvoiceData {
                invoice_direction: req.invoice_direction,
                invoice_kind: req.invoice_kind,
                party_id: req.party_id,
                invoice_code: req.invoice_code,
                invoice_no: req.invoice_no,
                invoice_date: req.invoice_date,
                gross_amount: req.gross_amount,
                net_amount: req.net_amount,
                tax_amount: req.tax_amount,
                rounding_adjustment_amount: req.rounding_adjustment_amount.unwrap_or(zero_amount()),
                rounding_reason: req.rounding_reason,
                original_invoice_id: None,
            },
            actor.id(),
        )?;
        persist_created_invoice(
            &self.db,
            &self.rbac,
            std::sync::Arc::clone(&self.object_read),
            invoice.clone(),
            actor.clone(),
        )
        .await?;
        self.finance
            .invoice_detail(&invoice.base.id)
            .await
            .map_err(Error::from)
    }

    /// 原子创建或提交销项发票并完成分配。
    ///
    /// 新发票的 `BusinessDocument` 注册、发票实体、销项分配、应收子账开票进度、
    /// 销售单开票进度和审计全部位于同一 MongoDB 事务。已有草稿则用乐观锁
    /// 校验后在同一事务过账，前端不得再执行“先创建、再过账”。
    ///
    /// # 参数
    /// * `req` - 新发票或已有草稿身份、最终分配与幂等键
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回已登记发票及其正式分配。
    ///
    /// # 错误
    /// * `ValidationError` - 新建/已有草稿参数组合或金额不合法
    /// * `ConflictError` - 草稿版本、状态或规范化发票号码冲突
    /// * `BusinessLogicError` - 跨主体、分配不守恒或超额开票
    pub async fn commit_invoice(&self, req: CommitInvoiceRequest, actor: &AuditActor) -> Result<InvoiceView> {
        req.validate()?;
        let command_receipt = CommandReceipt::from_payload(
            "sales-invoice-commit-",
            actor.id(),
            "invoice.commit",
            "invoice",
            &req.idempotency_key,
            &req,
        )?;
        if let Some(invoice_id) = command_receipt.committed_resource_id(&self.db).await? {
            return self
                .finance
                .invoice_detail(&invoice_id)
                .await
                .map_err(Error::from);
        }
        let prepared = req.prepare()?;
        let expected_task_version =
            erp_workflow::service::work_item::expected_task_version(&req.expected_task_version)?;
        let work_item_id = req.work_item_id.clone();
        let policy_revision = self.rbac.current_policy_revision().await?;
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let object_read = std::sync::Arc::clone(&self.object_read);
        let actor_owned = actor.clone();
        let actor_id = actor.id().to_string();
        let command_receipt_for_tx = command_receipt.clone();
        let transaction_result = rbac
            .clone()
            .run_authorized_policy_transaction(policy_revision, move |session| {
                Box::pin(async move {
                    let (mut invoice, plan_lines) = match prepared {
                        PreparedInvoiceCommit::New { invoice, allocations } => {
                            invoice.validate()?;
                            let new_invoice = Invoice::new(
                                InvoiceId::new(next_id()),
                                InvoiceData {
                                    invoice_direction: invoice.invoice_direction,
                                    invoice_kind: invoice.invoice_kind,
                                    party_id: invoice.party_id,
                                    invoice_code: invoice.invoice_code,
                                    invoice_no: invoice.invoice_no,
                                    invoice_date: invoice.invoice_date,
                                    gross_amount: invoice.gross_amount,
                                    net_amount: invoice.net_amount,
                                    tax_amount: invoice.tax_amount,
                                    rounding_adjustment_amount: invoice
                                        .rounding_adjustment_amount
                                        .unwrap_or(zero_amount()),
                                    rounding_reason: invoice.rounding_reason,
                                    original_invoice_id: None,
                                },
                                actor_id.as_str(),
                            )?;
                            register_created_invoice_document(
                                &db,
                                &rbac,
                                object_read.as_ref(),
                                &new_invoice,
                                &actor_owned,
                                session,
                            )
                            .await?;
                            db.invoices().create(&new_invoice, session).await?;
                            (new_invoice, allocations)
                        }
                        PreparedInvoiceCommit::Existing {
                            invoice_id,
                            expected_version,
                            allocations,
                        } => {
                            let invoice = db
                                .invoices()
                                .find_by_id(&invoice_id, session)
                                .await?
                                .ok_or_else(|| Error::NotFound("发票不存在".to_string()))?;
                            ensure_expected_version(invoice.base.version, expected_version)?;
                            ensure_sales_invoice(&invoice)?;
                            (invoice, allocations)
                        }
                    };
                    if invoice.stable.status() != InvoiceStatus::Draft {
                        return Err(Error::ConflictError("发票已登记，请勿重复提交".to_string()));
                    }
                    let duplicate = db
                        .invoices()
                        .find_by_direction_and_normalized_no(
                            invoice.invoice_direction,
                            &invoice.normalized_no,
                            session,
                        )
                        .await?;
                    if duplicate
                        .as_ref()
                        .is_some_and(|other| other.base.id != invoice.base.id)
                    {
                        return Err(Error::ConflictError("发票号码已登记，请勿重复提交".to_string()));
                    }
                    super::invoice_posting::post_invoice_in_transaction(
                        &db,
                        &mut invoice,
                        super::invoice_posting::InvoicePostingInput {
                            work_item_id: &work_item_id,
                            expected_task_version,
                            plan_lines: &plan_lines,
                            actor: &actor_owned,
                            action: "invoice.commit",
                            command_receipt: Some(&command_receipt_for_tx),
                        },
                        session,
                    )
                    .await?;
                    let committed_id = invoice.base.id.clone();
                    Ok::<String, crate::Error>(committed_id)
                })
            })
            .await;

        let detail_id = match transaction_result {
            Ok(invoice_id) => invoice_id,
            Err(error) => match command_receipt.committed_resource_id(&self.db).await? {
                Some(invoice_id) => invoice_id,
                None => return Err(error),
            },
        };

        self.finance.invoice_detail(&detail_id).await.map_err(Error::from)
    }

    /// 发票登记过账并分配（§8.3-2 事务不变量）。
    ///
    /// 同一事务内：规范化号码去重（`find_by_direction_and_normalized_no` +
    /// 唯一索引兜底）；校验发票与可开票子账同一往来主体；分配合计等于发票
    /// 金额；写销项发票分配；按条件原子更新子账净已开票进度
    /// （`apply_invoicing` 不超额开票）；发票迁移为已登记。
    /// 任一校验失败整体回滚。规范化发票号码唯一构成重复提交去重。
    ///
    /// # 参数
    /// * `id` - 发票 ID
    /// * `req` - 过账请求（分配行）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回登记后发票视图。
    ///
    /// # 错误
    /// * `NotFound` - 发票或子账不存在
    /// * `ConflictError` - 规范化号码已登记或发票已登记
    /// * `BusinessLogicError` - 跨主体开票、分配合计不等或超额开票
    pub async fn post_invoice(
        &self,
        id: &str,
        req: PostInvoiceRequest,
        actor: &AuditActor,
    ) -> Result<InvoiceView> {
        req.validate()?;
        let expected_task_version =
            erp_workflow::service::work_item::expected_task_version(&req.expected_task_version)?;
        let policy_revision = self.rbac.current_policy_revision().await?;
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let actor_owned = actor.clone();
        let invoice_id = id.to_string();
        let detail_id = invoice_id.clone();
        let work_item_id = req.work_item_id.clone();
        let plan_lines = convert_post_allocations(&req.allocations);
        rbac.run_authorized_policy_transaction(policy_revision, move |session| {
            Box::pin(async move {
                let mut invoice = db
                    .invoices()
                    .find_by_id(&invoice_id, session)
                    .await?
                    .ok_or_else(|| Error::NotFound("发票不存在".to_string()))?;
                ensure_sales_invoice(&invoice)?;
                if invoice.stable.status() != erp_finance::entity::receivable::InvoiceStatus::Draft {
                    return Err(Error::ConflictError("发票已登记，请勿重复提交".to_string()));
                }
                let duplicate = db
                    .invoices()
                    .find_by_direction_and_normalized_no(
                        invoice.invoice_direction,
                        &invoice.normalized_no,
                        session,
                    )
                    .await?;
                if let Some(other) = duplicate {
                    if other.base.id != invoice.base.id {
                        return Err(Error::ConflictError("发票号码已登记，请勿重复提交".to_string()));
                    }
                }

                super::invoice_posting::post_invoice_in_transaction(
                    &db,
                    &mut invoice,
                    super::invoice_posting::InvoicePostingInput {
                        work_item_id: &work_item_id,
                        expected_task_version,
                        plan_lines: &plan_lines,
                        actor: &actor_owned,
                        action: "invoice.post",
                        command_receipt: None,
                    },
                    session,
                )
                .await?;
                Ok::<(), crate::Error>(())
            })
        })
        .await?;

        self.finance.invoice_detail(&detail_id).await.map_err(Error::from)
    }
}

/// 发票创建必须跳过绑定：政策只能是 `NO_APPROVAL`。
///
/// # 返回
/// 返回 `SkipNoApproval`。
///
/// # 错误
/// 政策缺失或误登记为必须审批时返回部署不变量错误。
fn invoice_create_binding_decision() -> Result<BindingDecision> {
    let policy = policy_of(DocumentType::Invoice)?;
    match &policy {
        DocumentApprovalPolicy::NoApproval(no_approval) => {
            if no_approval.document_type != DocumentType::Invoice {
                return Err(Error::Internal("发票政策类型不匹配".to_string()));
            }
            Ok(binding_decision(policy.requirement()))
        }
        DocumentApprovalPolicy::ProcessRequired(_) => Err(Error::Internal(
            "发票必须是 NO_APPROVAL，不得绑定流程".to_string(),
        )),
    }
}

/// 确认发票创建路径不得查询发布定义。
///
/// # 错误
/// 绑定决定不是跳过时返回错误。
fn ensure_invoice_skips_approval_binding() -> Result<BindingDecision> {
    let decision = invoice_create_binding_decision()?;
    if decision != BindingDecision::SkipNoApproval {
        return Err(Error::Internal("发票创建必须跳过审批绑定".to_string()));
    }
    Ok(decision)
}

/// 发票不得注册空审批适配器。
///
/// # 错误
/// 适配器登记存在时返回部署不变量错误。
fn ensure_invoice_has_no_adapter() -> Result<()> {
    if adapter_spec_of(DocumentType::Invoice).is_ok() {
        return Err(Error::Internal("发票不得注册审批适配器".to_string()));
    }
    Ok(())
}

/// 发票往来主体作为绑定上下文组织，不得用空串补位。
///
/// # 参数
/// * `invoice` - 待登记发票
///
/// # 返回
/// 返回非空往来主体。
///
/// # 错误
/// 往来主体为空时返回校验错误。
fn invoice_binding_organization_id(invoice: &Invoice) -> Result<String> {
    let org = invoice.party_id.to_string();
    if org.trim().is_empty() {
        return Err(Error::ValidationError(
            "发票缺少往来主体，无法构造绑定上下文".to_string(),
        ));
    }
    Ok(org)
}

/// 构造发票创建绑定命令。客户端不得提交定义 ID。
///
/// # 参数
/// * `invoice` - 待登记发票
/// * `creator_id` - 创建人
///
/// # 错误
/// 往来主体为空时返回校验错误。
fn invoice_bind_command(invoice: &Invoice, creator_id: &str) -> Result<BindPublishedDefinitionCommand> {
    Ok(BindPublishedDefinitionCommand {
        document_type: DocumentType::Invoice,
        business_object_id: invoice.base.id.clone(),
        business_object_version: invoice.base.version,
        context: BindingRevalidationContext {
            organization_id: invoice_binding_organization_id(invoice)?,
            creator_id: creator_id.to_string(),
        },
    })
}

/// 将绑定端口返回值落实为发票注册行：空绑定保持未绑定。
///
/// # 参数
/// * `document` - 发票注册行
/// * `binding` - 统一绑定端口返回值
///
/// # 返回
/// 固定返回 `None`。
///
/// # 错误
/// 端口返回绑定或注册行已预置绑定时返回错误。
fn apply_invoice_create_binding(
    document: &mut BusinessDocument,
    binding: Option<ApprovalDefinitionBinding>,
) -> Result<Option<ApprovalDefinitionBinding>> {
    if binding.is_some() {
        return Err(Error::Internal(
            "发票为 NO_APPROVAL，不得写入审批绑定".to_string(),
        ));
    }
    if document.approval_binding.is_some() {
        return Err(Error::Internal("发票注册行不得预置审批绑定".to_string()));
    }
    if document.document_type != DocumentType::Invoice {
        return Err(Error::Internal("发票创建只能注册 Invoice 单据".to_string()));
    }
    Ok(None)
}

/// 在调用方事务内登记发票单据并证明空绑定。
///
/// 必须先确认政策跳过，再调用统一绑定端口；不得查询发布定义后假装成功。
///
/// # 错误
/// 政策非无审批、端口返回绑定或写入失败时返回错误。
async fn persist_unbound_invoice_document(
    db: &Database,
    rbac: &SharedRbacService,
    object_read: &dyn erp_workflow::ApprovalObjectReadPort,
    mut document: BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let _ = ensure_invoice_skips_approval_binding()?;
    ensure_invoice_has_no_adapter()?;
    let binding = crate::adapters::workflow::bind_published_definition_on_document_create(
        db,
        rbac,
        object_read,
        bind_command,
        actor,
        executor,
    )
    .await?;
    apply_invoice_create_binding(&mut document, binding)?;
    persist_registered_document(db, &document, executor)
        .await
        .map_err(crate::Error::from)
}

/// 为已构造发票登记 `BusinessDocument` 并调用统一绑定端口。
///
/// # 错误
/// 绑定端口或注册写入失败时返回错误。
pub(super) async fn register_created_invoice_document(
    db: &Database,
    rbac: &SharedRbacService,
    object_read: &dyn erp_workflow::ApprovalObjectReadPort,
    invoice: &Invoice,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let bind_command = invoice_bind_command(invoice, actor.id())?;
    let document = new_registered_document(
        &invoice.base.id,
        DocumentType::Invoice,
        invoice.invoice_no.clone(),
    )
    .map_err(crate::Error::from)?;
    persist_unbound_invoice_document(db, rbac, object_read, document, &bind_command, actor, executor).await
}

/// 在创建事务内写入发票草稿并登记无绑定单据。
///
/// # 错误
/// 绑定、注册或发票写入失败时返回错误，调用方必须视作整体回滚。
async fn persist_created_invoice(
    db: &Database,
    rbac: &SharedRbacService,
    object_read: std::sync::Arc<dyn erp_workflow::ApprovalObjectReadPort>,
    invoice: Invoice,
    actor: AuditActor,
) -> Result<()> {
    let audit = actor
        .clone()
        .resource_log("invoice.create", "invoice", invoice.base.id.clone())?;
    let db = db.clone();
    let rbac = rbac.clone();
    let object_read = object_read.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                register_created_invoice_document(
                    &db,
                    &rbac,
                    object_read.as_ref(),
                    &invoice,
                    &actor,
                    session,
                )
                .await?;
                db.invoices().create(&invoice, session).await?;
                db.audit_logs().create(&audit, session).await?;
                Ok::<(), crate::Error>(())
            })
        })
        .await
}

#[cfg(test)]
mod invoice_no_approval_tests {
    use super::{
        apply_invoice_create_binding, ensure_invoice_has_no_adapter, ensure_invoice_skips_approval_binding,
        invoice_bind_command, invoice_create_binding_decision, policy_of, BindingDecision,
        DocumentApprovalPolicy, DocumentType, Invoice, InvoiceData,
    };
    use bpm::ids::ApprovalProcessDefinitionId;
    use bpm::ProcessKind;
    use erp_core::common::time::{BusinessDate, Instant};
    use erp_core::ids::{InvoiceId, PartyId};
    use erp_core::money::Amount;
    use erp_finance::entity::receivable::{InvoiceDirection, InvoiceKind};
    use erp_workflow::service::approval::binding::binding_from_published;
    use erp_workflow::service::document_registry::new_registered_document;
    use std::str::FromStr;

    fn draft_invoice() -> Invoice {
        Invoice::new(
            InvoiceId::new("inv-1"),
            InvoiceData {
                invoice_direction: InvoiceDirection::Sales,
                invoice_kind: InvoiceKind::Blue,
                party_id: PartyId::new("party-1"),
                invoice_code: None,
                invoice_no: "INV-1".into(),
                invoice_date: BusinessDate::from_ymd(2026, 8, 6).expect("日期合法"),
                gross_amount: Amount::from_str("100").expect("金额合法"),
                net_amount: Amount::from_str("88.50").expect("金额合法"),
                tax_amount: Amount::from_str("11.50").expect("金额合法"),
                rounding_adjustment_amount: Amount::from_str("0").expect("金额合法"),
                rounding_reason: None,
                original_invoice_id: None,
            },
            "admin-1",
        )
        .expect("草稿必须可构造")
    }

    /// 政策仅含 document_type、approval_requirement、process_kind，不得注册空 Adapter。
    #[test]
    fn invoice_policy_is_no_approval_identity_only() {
        let policy = policy_of(DocumentType::Invoice).expect("发票政策必须存在");
        let DocumentApprovalPolicy::NoApproval(no_approval) = &policy else {
            panic!("发票必须是 NO_APPROVAL");
        };
        assert_eq!(no_approval.document_type, DocumentType::Invoice);
        assert_eq!(no_approval.process_kind, ProcessKind::Invoice);
        assert_eq!(
            invoice_create_binding_decision().expect("绑定决定"),
            BindingDecision::SkipNoApproval
        );
        assert_eq!(
            ensure_invoice_skips_approval_binding().expect("必须跳过"),
            BindingDecision::SkipNoApproval
        );
        ensure_invoice_has_no_adapter().expect("不得注册空适配器");
    }

    /// 创建必须注册 BusinessDocument，绑定端口返回空，禁止写入绑定。
    #[test]
    fn create_registers_document_and_returns_empty_binding() {
        let invoice = draft_invoice();
        let command = invoice_bind_command(&invoice, "admin-1").expect("绑定命令");
        assert_eq!(command.document_type, DocumentType::Invoice);
        assert_eq!(command.business_object_id, invoice.base.id);
        assert_eq!(command.context.organization_id, "party-1");

        let mut document = new_registered_document(
            &invoice.base.id,
            DocumentType::Invoice,
            invoice.invoice_no.clone(),
        )
        .expect("可注册");
        assert!(document.approval_binding.is_none());
        let empty = apply_invoice_create_binding(&mut document, None).expect("空绑定");
        assert!(empty.is_none());
        assert!(document.approval_binding.is_none());

        let forged = binding_from_published(
            ApprovalProcessDefinitionId::new("def-1"),
            1,
            Instant::from_unix_secs(10),
        )
        .expect("测试绑定");
        assert!(apply_invoice_create_binding(&mut document, Some(forged)).is_err());
    }

    /// 创建路径调用统一绑定端口，不查询发布定义、不启动实例、不建任务。
    #[test]
    fn create_does_not_query_definition_or_start_instance() {
        let production = include_str!("invoice.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(production.contains("persist_created_invoice"));
        assert!(production.contains("register_created_invoice_document"));
        assert!(production.contains("persist_unbound_invoice_document"));
        assert!(production.contains("bind_published_definition_on_document_create"));
        assert!(production.contains("DocumentType::Invoice"));
        assert!(production.contains("new_registered_document"));
        assert!(production.contains("ensure_invoice_skips_approval_binding"));
        assert!(production.contains("ensure_invoice_has_no_adapter"));
        assert!(!production.contains("pub async fn submit_invoice"));
        assert!(!production.contains("start_invoice_approval"));
        assert!(!production.contains("InvoiceAdapter"));
        assert!(!production.contains("load_published_graph"));
        let invoice_create = production
            .split("pub async fn create_invoice")
            .nth(1)
            .and_then(|rest| rest.split("pub async fn post_invoice").next())
            .expect("create_invoice 生产片段");
        assert!(invoice_create.contains("persist_created_invoice"));
        assert!(!invoice_create.contains("prepare_start"));
        assert!(!invoice_create.contains("attach_published_binding"));
        assert!(!invoice_create.contains("WorkItem"));
        assert!(!invoice_create.contains("start_approval"));
    }
}
