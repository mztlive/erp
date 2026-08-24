//! 销售单命令用例：建单、保存草稿、提交、作废。

use std::collections::HashSet;

use mongodb::ClientSession;

use database::{
    AccessControlExt, CatalogExt, ContractExt, CustomerExt, DocumentRegistryExt, Executor, NoTransaction,
    SalesOrderExt, SourceRegistryExt, Transactional,
};
use entities::common::time::{BusinessDate, Instant};
use entities::document_registry::{
    BusinessDocument, BusinessDocumentData, WorkflowAction, WorkflowActionData, WorkflowActionType,
};
use entities::ids::{
    BusinessDocumentId, ContractId, ContractRevisionId, CustomerAccountId, SalesOrderId,
    SalesOrderSubmissionId, SalesOrderWorkingCopyId, WorkflowActionId,
};
use entities::sales_order::{
    BusinessType, LineType, SalesOrder, SalesOrderData, SalesOrderWorkingCopy, SalesOrderWorkingCopyLine,
    SalesOrderWorkingCopyUpdate, WorkingPurpose,
};
use entities::source_registry::{ExternalObjectType, SourceSystemType};
use id_generator::next_id;
use sha2::{Digest, Sha256};
use validator::Validate;

use super::adapter::{
    build_sales_order_snapshot, document_type_for_sales_create, execute_sales_order_domain_action,
    reject_legacy_card_sales_decision, reject_legacy_card_sales_work_item, require_frozen_binding,
    sales_approval_ports, sales_order_object_readable, sales_order_responsible_org_id,
    sales_order_start_command, start_approval_command_kind, subject_ref_for_sales_business,
    RECENT_HISTORY_LIMIT,
};
use super::cancel_approval::{
    build_sales_order_cancel_input, load_cancel_runtime, persist_sales_order_cancel,
    SalesOrderCancelPersistInput,
};
use super::dto::{
    CancelSalesOrderApprovalRequest, CreateSalesOrderRequest, SalesOrderCreateIntent, SalesOrderDetailView,
    SalesOrderDraftLineRequest, SaveWorkingCopyRequest, SubmissionView, SubmitSalesOrderRequest,
    VoidSalesOrderRequest, WorkingCopyView,
};
use super::mapper::{
    build_stable_lines, build_submission, build_submission_lines, build_working_copy,
    build_working_copy_lines, draft_hash, header_snapshot, submission_view,
};
use super::pricing::line_totals;
use super::start_approval::{
    build_sales_order_start_input, load_bound_definition_graph, load_start_receipt,
    persist_sales_order_start, SalesOrderStartInput, SalesOrderStartPersistInput,
};
use super::SalesOrderService;
use crate::approval::binding::{
    attach_published_binding, bind_published_definition_on_document_create, BindPublishedDefinitionCommand,
};
use crate::approval::business_adapter::BindingRevalidationContext;
use crate::approval::execution::{prepare_cancel, prepare_start};
use crate::audit::AuditActor;
use crate::document_registry::find_approval_binding;
use crate::errors::{Error, Result};
use crate::iam::SharedRbacService;

/// 提交时从目标商城映射注册表精确解析出的两类外部身份。
#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedCardExternalIdentities {
    customer: String,
    voucher_category: String,
}

/// 销售单提交并启动审批所需的单据集合。
///
/// # 用途
/// 将销售单、工作副本、冻结提交与外部身份打包。
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
/// 提交行必须由当前工作副本冻结；卡券身份在提交期间不得变化。
struct ApprovalSubmissionStart<'a> {
    /// 销售单主键。
    id: &'a str,
    /// 已进入审批中的销售单。
    order: SalesOrder,
    /// 已锁定的工作副本。
    working_copy: SalesOrderWorkingCopy,
    /// 冻结提交头。
    submission: entities::sales_order::SalesOrderSubmission,
    /// 冻结提交行。
    submission_lines: Vec<entities::sales_order::SalesOrderSubmissionLine>,
    /// 提交时解析的卡券外部身份。
    resolved_identities: Option<ResolvedCardExternalIdentities>,
    /// 工作副本行，用于可售引用重验。
    copy_lines: Vec<SalesOrderWorkingCopyLine>,
}

/// 为销售提交幂等命令生成不泄露原始幂等键的稳定收据 ID。
fn sales_submission_audit_id(actor_id: &str, sales_order_id: &str, idempotency_key: &str) -> String {
    format!(
        "sales-order-submit-{:x}",
        Sha256::digest(format!("{actor_id}|{sales_order_id}|{idempotency_key}").as_bytes())
    )
}

/// 锁定同一幂等键可重放的完整请求身份。
fn sales_submission_fingerprint(
    actor_id: &str,
    sales_order_id: &str,
    expected_working_copy_version: u64,
    idempotency_key: &str,
) -> String {
    format!(
        "{:x}",
        Sha256::digest(
            format!("{actor_id}|{sales_order_id}|{expected_working_copy_version}|{idempotency_key}")
                .as_bytes(),
        )
    )
}

/// 按业务性质构造创建时绑定命令。
///
/// `GoodsService` 绑定 `SalesOrder`，`Voucher` 绑定 `VoucherSalesOrder`。
///
/// # 错误
/// 责任组织为空时返回校验错误。
fn sales_create_bind_command(
    order: &SalesOrder,
    actor: &AuditActor,
) -> Result<BindPublishedDefinitionCommand> {
    Ok(BindPublishedDefinitionCommand {
        document_type: document_type_for_sales_create(order.business_type),
        business_object_id: order.base.id.clone(),
        business_object_version: order.base.version,
        context: BindingRevalidationContext {
            organization_id: sales_order_responsible_org_id(order)?,
            creator_id: actor.id().to_string(),
        },
    })
}

/// 查询发布定义、写入绑定并持久化注册行。
///
/// # 错误
/// 无发布定义或绑定失败时返回错误，调用方必须回滚。
async fn persist_bound_sales_document(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    document: &mut BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    session: &mut ClientSession,
) -> Result<()> {
    let _ = sales_order_object_readable(
        &bind_command.context.organization_id,
        &bind_command.context.creator_id,
    )?;
    let binding =
        bind_published_definition_on_document_create(db, rbac, bind_command, actor, session).await?;
    let binding = binding.ok_or_else(|| Error::Internal("销售单必须绑定已发布定义".to_string()))?;
    attach_published_binding(document, binding)?;
    db.business_documents().create(document, session).await?;
    Ok(())
}

/// 为销售建单命令生成不泄露原始幂等键的稳定收据 ID。
fn sales_order_create_audit_id(actor_id: &str, idempotency_key: &str) -> String {
    let mut digest = Sha256::new();
    for part in [actor_id, idempotency_key.trim()] {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part.as_bytes());
    }
    format!("sales-order-create-{:x}", digest.finalize())
}

/// 锁定销售建单命令的完整载荷与鉴权操作者。
fn sales_order_create_fingerprint<T: serde::Serialize>(actor_id: &str, request: &T) -> Result<String> {
    let payload = serde_json::to_vec(&(actor_id, request))
        .map_err(|error| Error::Internal(format!("销售建单命令序列化失败: {error}")))?;
    Ok(format!("{:x}", Sha256::digest(payload)))
}

impl SalesOrderService {
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
            return self
                .complete_sales_order_creation(&order_id, req.intent, req.idempotency_key.clone(), actor)
                .await;
        }
        self.ensure_sellable_draft_lines(&req.draft.lines).await?;
        let customer = self
            .db
            .customer_accounts()
            .find_by_id(&req.customer_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("客户不存在".to_string()))?;
        if !customer.is_active() {
            return Err(Error::BusinessLogicError(
                "客户已停用，禁止创建新销售单".to_string(),
            ));
        }
        self.ensure_selected_contract_revision(
            req.contract_id.as_ref(),
            &req.customer_id,
            req.draft.requested_contract_revision_id.as_ref(),
        )
        .await?;

        let order = SalesOrder::new(
            SalesOrderId::new(next_id()),
            SalesOrderData {
                order_no: req.order_no,
                business_type: req.business_type,
                origin_system: entities::sales_order::OriginSystem::Erp,
                source_identity_id: None,
                customer_id: req.customer_id,
                contract_id: req.contract_id,
                settlement_party_id: req.settlement_party_id,
                source_status_code: None,
            },
            actor.id(),
        )?;
        let order_id = SalesOrderId::new(order.base.id.clone());
        let document_type = document_type_for_sales_create(req.business_type);
        let document = BusinessDocument::new(
            BusinessDocumentId::new(order.base.id.clone()),
            BusinessDocumentData {
                document_type,
                document_no: order.order_no.clone(),
            },
        )?;
        let stable_lines = build_stable_lines(&order_id, &req.draft.lines)?;
        let (working_copy, working_copy_lines) =
            build_working_copy(&order, &stable_lines, &req.draft, 1, actor)?;

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
                return self
                    .complete_sales_order_creation(&order_id, req.intent, req.idempotency_key.clone(), actor)
                    .await;
            }
            return Err(error);
        }

        self.complete_sales_order_creation(&order.base.id, req.intent, req.idempotency_key, actor)
            .await
    }

    /// 完成建单命令声明的意图；重放 `SUBMIT` 时复用提交命令自身的幂等收据。
    async fn complete_sales_order_creation(
        &self,
        order_id: &str,
        intent: SalesOrderCreateIntent,
        idempotency_key: String,
        actor: &AuditActor,
    ) -> Result<SalesOrderDetailView> {
        if intent == SalesOrderCreateIntent::Submit {
            self.submit_sales_order(
                order_id,
                SubmitSalesOrderRequest {
                    version: 1,
                    idempotency_key,
                },
                actor,
            )
            .await?;
        }
        self.sales_order_detail(order_id, None).await
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

    /// 保存草稿（整表头覆盖 + 明细整批替换，乐观锁语义）。
    ///
    /// 采购/销售驳回后订单回到草稿，但首次提交工作副本已是 `Submitted` 终态时，
    /// 会新开一份 `Editing` 副本，而不是返回「有效工作副本不存在」。
    /// 已有有效副本时，`req.version` 必须与当前工作副本版本一致；新开副本不校验
    /// 该版本（前端在无副本时会把销售单版本误当成草稿版本）。
    /// 行替换在事务内「软删旧行 + 写入新行」原子完成。
    ///
    /// # 参数
    /// * `id` - 销售单 ID
    /// * `req` - 保存请求（含期望版本与草稿内容）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回保存后的工作副本视图。
    ///
    /// # 错误
    /// * `NotFound` - 销售单不存在
    /// * `ConflictError` - 非草稿，或已有副本但期望版本不一致
    pub async fn save_working_copy(
        &self,
        id: &str,
        req: SaveWorkingCopyRequest,
        actor: &AuditActor,
    ) -> Result<WorkingCopyView> {
        req.validate()?;
        let order = self
            .db
            .sales_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
        let order_id = SalesOrderId::new(order.base.id.clone());
        self.ensure_selected_contract_revision(
            order.contract_id.as_ref(),
            &order.customer_id,
            req.draft.requested_contract_revision_id.as_ref(),
        )
        .await?;
        self.ensure_sellable_draft_lines(&req.draft.lines).await?;
        let (mut working_copy, stable, opened_new) = self
            .load_or_reopen_first_submission_working_copy(&order, req.version, &req.draft, actor)
            .await?;
        if opened_new {
            return self.working_copy_view(&working_copy).await;
        }

        let snapshot = header_snapshot(&req.draft)?;
        let created_stable_lines = stable.created;
        let lines = build_working_copy_lines(
            &order_id,
            &working_copy.base.id.clone().into(),
            &stable.all,
            &req.draft.lines,
        )?;
        let (gross, net, tax) = line_totals(&lines);
        let next_version = working_copy.draft_version + 1;
        working_copy.update(
            SalesOrderWorkingCopyUpdate {
                content_hash: Some(draft_hash(&working_copy.base.id, next_version)),
                customer_id: Some(order.customer_id.clone()),
                contract_id: order.contract_id.clone(),
                contract_revision_id: req.draft.requested_contract_revision_id.clone(),
                settlement_party_id: Some(order.settlement_party_id.clone()),
                snapshot: Some(snapshot),
                project_name: req.draft.project_name.clone(),
                business_remark: req.draft.business_remark.clone(),
                voucher_category_sku_id: req.draft.voucher_category_sku_id.clone(),
                voucher_expiry_at: req
                    .draft
                    .voucher_expiry_at
                    .map(|secs| Instant::from_unix_secs(secs as i64)),
                target_mall_id: req.draft.target_mall_id.clone(),
                receivable_due_date: req.draft.receivable_due_date,
                gross_amount: Some(gross),
                net_amount: Some(net),
                tax_amount: Some(tax),
            },
            actor.id(),
        )?;
        working_copy.save_draft(
            draft_hash(&working_copy.base.id, next_version),
            req.draft.editor_user_id.clone(),
        )?;

        let old_lines = self
            .db
            .sales_order_working_copy_lines()
            .list_lines_by_working_copy(&working_copy.base.id.clone().into(), &mut NoTransaction)
            .await?;
        let audit = actor
            .clone()
            .resource_log("sales_order.save_draft", "sales_order", id.to_string())?;
        let db = self.db.clone();
        let client = db.client().clone();
        let lines_for_tx = lines.clone();
        let created_stable_for_tx = created_stable_lines;
        let sellable_refs_for_tx = Self::sellable_working_copy_refs(&lines)?;
        let working_copy = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    SalesOrderService::new(db.clone())
                        .ensure_sellable_refs(&sellable_refs_for_tx, session)
                        .await?;
                    for line in &created_stable_for_tx {
                        db.sales_order_lines().create(line, session).await?;
                    }
                    for mut old in old_lines {
                        db.sales_order_working_copy_lines()
                            .soft_delete(&mut old, session)
                            .await?;
                    }
                    for line in &lines_for_tx {
                        db.sales_order_working_copy_lines().create(line, session).await?;
                    }
                    db.sales_order_working_copies()
                        .update(&mut working_copy, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<SalesOrderWorkingCopy, crate::errors::Error>(working_copy)
                })
            })
            .await?;

        self.working_copy_view(&working_copy).await
    }

    /// 提交销售单并冻结提交快照。
    ///
    /// `GoodsService` 与 `Voucher` 均直接调用统一 `start_approval`，以
    /// `submission_no` 冻结 `subject_version` 与快照；禁止经 `sales_review`
    /// 准入、`CARD_SALES_APPROVAL` 或第二条启动路径。
    ///
    /// # 参数
    /// * `id` - 销售单 ID
    /// * `req` - 提交请求（含期望草稿版本与幂等键）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回提交快照视图；已提交过的草稿幂等返回既有提交。
    ///
    /// # 错误
    /// * `NotFound` - 销售单或有效工作副本不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    pub async fn submit_sales_order(
        &self,
        id: &str,
        req: SubmitSalesOrderRequest,
        actor: &AuditActor,
    ) -> Result<SubmissionView> {
        req.validate()?;
        let idempotency_key = req.idempotency_key.trim();
        if idempotency_key.is_empty() {
            return Err(Error::ValidationError("幂等键不能为空".to_string()));
        }
        let audit_id = sales_submission_audit_id(actor.id(), id, idempotency_key);
        let fingerprint = sales_submission_fingerprint(actor.id(), id, req.version, idempotency_key);
        if let Some(existing) = self
            .replay_sales_submission(&audit_id, &fingerprint, id, actor.id())
            .await?
        {
            return Ok(existing);
        }
        let order = self
            .db
            .sales_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
        let order_id = SalesOrderId::new(order.base.id.clone());
        let mut working_copy = self
            .db
            .sales_order_working_copies()
            .find_active_by_order_and_purpose(&order_id, WorkingPurpose::FirstSubmission, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("有效工作副本不存在".to_string()))?;
        if working_copy.base.version != req.version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        if let Some(existing) = self
            .db
            .sales_order_submissions()
            .find_one_by_field(
                "working_copy_id",
                working_copy.base.id.as_str(),
                &mut NoTransaction,
            )
            .await?
        {
            let existing_id = SalesOrderSubmissionId::new(existing.base.id.clone());
            let existing_lines = self
                .db
                .sales_order_submission_lines()
                .list_lines_by_submissions(&[existing_id], &mut NoTransaction)
                .await?;
            return Ok(submission_view(existing, existing_lines));
        }

        let copy_id = SalesOrderWorkingCopyId::new(working_copy.base.id.clone());
        let copy_lines = self
            .db
            .sales_order_working_copy_lines()
            .list_lines_by_working_copy(&copy_id, &mut NoTransaction)
            .await?;
        self.ensure_sellable_working_copy_lines(&copy_lines).await?;
        self.ensure_procurement_responsibility_before_submit(&order, &copy_lines)
            .await?;
        let existing_submissions = self
            .db
            .sales_order_submissions()
            .find_many(mongodb::bson::doc! { "sales_order_id": id }, &mut NoTransaction)
            .await?;
        let submission_no = existing_submissions
            .iter()
            .map(|submission| submission.submission_no)
            .max()
            .unwrap_or(0)
            + 1;
        let resolved_identities = self
            .resolve_card_external_identities(&working_copy, &mut NoTransaction)
            .await?;
        let submission = build_submission(
            &working_copy,
            &copy_lines,
            submission_no,
            actor,
            resolved_identities.as_ref().map(|value| value.customer.as_str()),
            resolved_identities
                .as_ref()
                .map(|value| value.voucher_category.as_str()),
        )?;
        let submission_lines = build_submission_lines(&submission, &copy_lines)?;
        working_copy.submit()?;
        self.start_approval_submission(
            ApprovalSubmissionStart {
                id,
                order,
                working_copy,
                submission,
                submission_lines,
                resolved_identities,
                copy_lines,
            },
            actor,
            &req.idempotency_key,
            audit_id,
            fingerprint,
        )
        .await
    }

    /// 销售单提交并启动统一审批。
    ///
    /// # 用途
    /// 冻结提交快照并启动统一审批。
    ///
    /// # 参数
    /// * `start` - 销售单、工作副本、提交与外部身份
    /// * `actor` - 审计操作人
    /// * `idempotency_key` - 客户端幂等键
    /// * `audit_id` - 幂等审计主键
    /// * `fingerprint` - 请求摘要
    ///
    /// # 返回
    /// 返回提交快照视图。
    ///
    /// # 错误
    /// 无绑定、定义缺失、状态不允许或写入失败时返回错误。
    ///
    /// # 关键业务约束
    /// 提交期间目标商城外部身份不得变化。
    async fn start_approval_submission(
        &self,
        start: ApprovalSubmissionStart<'_>,
        actor: &AuditActor,
        idempotency_key: &str,
        audit_id: String,
        fingerprint: String,
    ) -> Result<SubmissionView> {
        let ApprovalSubmissionStart {
            id,
            mut order,
            working_copy,
            submission,
            submission_lines,
            resolved_identities,
            copy_lines,
        } = start;
        let ports = sales_approval_ports(order.business_type)?;
        let subject = subject_ref_for_sales_business(order.business_type, id)?;
        let binding = find_approval_binding(&self.db, id, &mut NoTransaction).await?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
        execute_sales_order_domain_action(&mut order, ports.on_approval_start, actor.id())?;
        let now = Instant::now();
        let snapshot = build_sales_order_snapshot(&order, &submission, &submission_lines, actor.id(), now)?;
        let start = sales_order_start_command(
            ports.document_type,
            id,
            submission.submission_no,
            actor.id(),
            idempotency_key,
        );
        ensure_unified_start_command(&start)?;
        let _ = (start_approval_command_kind(&start), RECENT_HISTORY_LIMIT);
        let organization_id = sales_order_responsible_org_id(&order)?;
        let graph = load_bound_definition_graph(&self.db, &binding).await?;
        let existing_receipt = load_start_receipt(
            &self.db,
            ports.document_type,
            &subject,
            submission.submission_no,
            idempotency_key,
        )
        .await?;
        let start_input = build_sales_order_start_input(SalesOrderStartInput {
            graph,
            binding: &binding,
            document_type: ports.document_type,
            subject,
            subject_version: submission.submission_no,
            actor_id: actor.id(),
            organization_id: &organization_id,
            idempotency_key,
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
                to_status: "PENDING_REVIEW".to_string(),
                actor_id: actor.id().to_string(),
                actor_role: "role-sales".to_string(),
                comment: None,
            },
        )?;
        let audit = actor.clone().resource_log_with_id(
            audit_id.clone(),
            "sales_order.submit",
            "sales_order_submission",
            submission.base.id.clone(),
            Some(format!("command_sha256={fingerprint}")),
        )?;
        let sellable_refs = Self::sellable_working_copy_refs(&copy_lines)?;
        SalesOrderService::new(self.db.clone())
            .ensure_sellable_refs(&sellable_refs, &mut NoTransaction)
            .await?;
        let current_identities = self
            .resolve_card_external_identities(&working_copy, &mut NoTransaction)
            .await?;
        if current_identities != resolved_identities {
            return Err(Error::ConflictError(
                "目标商城外部身份映射在提交期间已变化，请刷新后重试".to_string(),
            ));
        }
        persist_sales_order_start(
            &self.db,
            SalesOrderStartPersistInput {
                order,
                working_copy,
                submission,
                submission_lines,
                workflow_action,
                document_type: ports.document_type,
                snapshot_payload: snapshot,
                prepared,
                owner_role: ports.owner_role,
                organization_id,
                now,
                audit,
            },
        )
        .await
    }

    /// 撤回审批中的销售单，回到可修正草稿。
    ///
    /// `GoodsService` 与 `Voucher` 均先按主体加载 RUNNING/BLOCKED 实例并调用
    /// 统一 `prepare_cancel`，关闭开放任务后再执行 `cancel_action`。
    /// 已 `APPROVED` 必须拒绝。
    ///
    /// # 参数
    /// * `id` - 销售单主键
    /// * `req` - 撤回请求（原因必填）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回撤回后的销售单详情。
    ///
    /// # 错误
    /// 非审批中、已最终通过、原因缺失或并发冲突时返回错误。
    pub async fn cancel_approval_submission(
        &self,
        id: &str,
        req: CancelSalesOrderApprovalRequest,
        actor: &AuditActor,
    ) -> Result<SalesOrderDetailView> {
        req.validate()?;
        let mut order = self
            .db
            .sales_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
        if order.base.version != req.expected_version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        let ports = sales_approval_ports(order.business_type)?;
        let binding = find_approval_binding(&self.db, id, &mut NoTransaction).await?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
        let subject = subject_ref_for_sales_business(order.business_type, id)?;
        let subject_version = latest_submission_no(&self.db, id).await?;
        let runtime = load_cancel_runtime(&self.db, &binding, &subject, subject_version).await?;
        let now = Instant::now();
        let input = build_sales_order_cancel_input(
            &runtime,
            &req.reason,
            actor.id(),
            &req.idempotency_key,
            None,
            now,
        )?;
        let prepared = prepare_cancel(input)?;
        execute_sales_order_domain_action(&mut order, ports.cancel_action, actor.id())?;
        let audit =
            actor
                .clone()
                .resource_log("sales_order.cancel_approval", "sales_order", id.to_string())?;
        persist_sales_order_cancel(
            &self.db,
            SalesOrderCancelPersistInput {
                order,
                prepared,
                open_tasks: runtime.open_tasks,
                actor_id: actor.id().to_string(),
                reason: req.reason.clone(),
                now,
                audit,
            },
        )
        .await?;
        self.sales_order_detail(id, None).await
    }

    /// 按稳定审计收据重放已提交的销售快照。
    async fn replay_sales_submission(
        &self,
        audit_id: &str,
        expected_fingerprint: &str,
        sales_order_id: &str,
        actor_id: &str,
    ) -> Result<Option<SubmissionView>> {
        let Some(audit) = self
            .db
            .audit_logs()
            .find_by_id(audit_id, &mut NoTransaction)
            .await?
        else {
            return Ok(None);
        };
        if audit.message.as_deref() != Some(&format!("command_sha256={expected_fingerprint}")) {
            return Err(Error::ConflictError("同一幂等键已用于不同的销售提交".to_string()));
        }
        let submission_id = audit
            .resource_id
            .as_deref()
            .ok_or_else(|| Error::Internal("销售提交幂等收据缺少结果引用".to_string()))?;
        let submission = self
            .db
            .sales_order_submissions()
            .find_by_id(submission_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("销售提交幂等收据对应快照缺失".to_string()))?;
        if submission.sales_order_id.as_ref() != sales_order_id || submission.submitted_by != actor_id {
            return Err(Error::Internal("销售提交幂等收据与业务对象不一致".to_string()));
        }
        let submission_id = SalesOrderSubmissionId::new(submission.base.id.clone());
        let lines = self
            .db
            .sales_order_submission_lines()
            .list_lines_by_submissions(&[submission_id], &mut NoTransaction)
            .await?;
        Ok(Some(submission_view(submission, lines)))
    }

    /// 解析并校验卡券提交所需的目标商城与两类外部身份。
    async fn resolve_card_external_identities(
        &self,
        working_copy: &SalesOrderWorkingCopy,
        executor: &mut dyn Executor,
    ) -> Result<Option<ResolvedCardExternalIdentities>> {
        if working_copy.business_type != BusinessType::Voucher {
            return Ok(None);
        }
        let target_mall_id = working_copy
            .target_mall_id
            .as_ref()
            .ok_or_else(|| Error::ValidationError("卡券销售提交前必须选择目标商城".to_string()))?;
        let receivable_due_date = working_copy
            .receivable_due_date
            .ok_or_else(|| Error::ValidationError("卡券销售提交前必须填写应收到期日".to_string()))?;
        if receivable_due_date < BusinessDate::today() {
            return Err(Error::ValidationError(
                "应收到期日不得早于服务端提交日".to_string(),
            ));
        }
        let voucher_category_id = working_copy
            .voucher_category_sku_id
            .as_ref()
            .ok_or_else(|| Error::ValidationError("卡券销售提交前必须选择卡券类目".to_string()))?;
        let mall = self
            .db
            .source_systems()
            .find_by_id(target_mall_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::NotFound("目标商城不存在".to_string()))?;
        if mall.system_type != SourceSystemType::Mall || !mall.is_active() {
            return Err(Error::BusinessLogicError(
                "目标来源系统必须是启用中的商城".to_string(),
            ));
        }
        let as_of = Instant::now().unix_secs();
        let customer = self
            .unique_card_external_identity(
                target_mall_id,
                ExternalObjectType::Customer,
                working_copy.customer_id.as_ref(),
                "客户",
                as_of,
                executor,
            )
            .await?;
        let voucher_category = self
            .unique_card_external_identity(
                target_mall_id,
                ExternalObjectType::VoucherCategory,
                voucher_category_id.as_ref(),
                "卡券类目",
                as_of,
                executor,
            )
            .await?;
        Ok(Some(ResolvedCardExternalIdentities {
            customer,
            voucher_category,
        }))
    }

    /// 要求目标商城下给定 ERP 对象恰好存在一条当前有效外部身份。
    async fn unique_card_external_identity(
        &self,
        target_mall_id: &entities::ids::SourceSystemId,
        object_type: ExternalObjectType,
        internal_object_id: &str,
        object_label: &str,
        as_of: i64,
        executor: &mut dyn Executor,
    ) -> Result<String> {
        let matches = self
            .db
            .source_registry()
            .active_external_identities_for_internal_object(
                target_mall_id,
                object_type,
                internal_object_id,
                as_of,
                executor,
            )
            .await?;
        require_unique_card_external_identity(matches, object_label)
    }

    /// 校验请求草稿中的实物及服务行仍引用公司商品池内的精确 SKU 修订。
    ///
    /// # 参数
    /// * `lines` - 草稿行请求
    ///
    /// # 返回
    /// 全部引用仍可销售时返回 `Ok(())`。
    ///
    /// # 错误
    /// 任一 `sku_id + sku_revision_id` 不再可销售时返回校验错误。
    async fn ensure_sellable_draft_lines(&self, lines: &[SalesOrderDraftLineRequest]) -> Result<()> {
        let refs = lines
            .iter()
            .filter_map(|line| line.goods.as_ref())
            .map(|goods| (goods.sku_id.to_string(), goods.sku_revision_id.to_string()))
            .collect::<Vec<_>>();
        self.ensure_sellable_refs(&refs, &mut NoTransaction).await
    }

    /// 校验销售草稿精确引用所选合同的当前不可变版本。
    async fn ensure_selected_contract_revision(
        &self,
        contract_id: Option<&ContractId>,
        customer_id: &CustomerAccountId,
        revision_id: Option<&ContractRevisionId>,
    ) -> Result<()> {
        let (Some(contract_id), Some(revision_id)) = (contract_id, revision_id) else {
            if contract_id.is_none() && revision_id.is_none() {
                return Ok(());
            }
            return Err(Error::ValidationError(
                "合同与合同版本必须同时提供或同时省略".to_string(),
            ));
        };
        let contract = self
            .db
            .contracts()
            .find_by_id(contract_id.as_ref(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("合同不存在".to_string()))?;
        if &contract.customer_id != customer_id {
            return Err(Error::ValidationError(
                "销售单引用的合同不属于所选客户".to_string(),
            ));
        }
        if contract.stable.status != entities::contract::ContractStatus::Effective {
            return Err(Error::BusinessLogicError(
                "合同当前不可用于新销售提交".to_string(),
            ));
        }
        if contract.stable.current_revision_id.as_deref() != Some(revision_id.as_ref()) {
            return Err(Error::ConflictError(
                "所选合同版本已不是当前可用版本，请刷新后重新选择".to_string(),
            ));
        }
        let revision = self
            .db
            .contract_revisions()
            .find_by_id(revision_id.as_ref(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("合同版本不存在".to_string()))?;
        if revision.contract_id.as_ref() != contract_id.as_ref() {
            return Err(Error::ValidationError("合同版本不属于所选合同".to_string()));
        }
        Ok(())
    }

    /// 提交前重新校验已保存工作副本的精确 SKU 修订资格。
    ///
    /// # 参数
    /// * `lines` - 已保存工作副本行
    ///
    /// # 返回
    /// 全部引用仍可销售时返回 `Ok(())`。
    ///
    /// # 错误
    /// 缺 SKU/修订或引用失效时返回校验错误。
    async fn ensure_sellable_working_copy_lines(&self, lines: &[SalesOrderWorkingCopyLine]) -> Result<()> {
        let refs = Self::sellable_working_copy_refs(lines)?;
        self.ensure_sellable_refs(&refs, &mut NoTransaction).await
    }

    /// 从工作副本行提取必须成对存在的销售 SKU 与修订引用。
    ///
    /// # 参数
    /// * `lines` - 工作副本行
    ///
    /// # 返回
    /// 返回 `(sku_id, sku_revision_id)` 列表。
    ///
    /// # 错误
    /// 实物行缺少 SKU 或修订身份时返回校验错误。
    pub(super) fn sellable_working_copy_refs(
        lines: &[SalesOrderWorkingCopyLine],
    ) -> Result<Vec<(String, String)>> {
        let refs = lines
            .iter()
            .filter(|line| line.line_type == LineType::GoodsService)
            .map(|line| {
                let sku_id = line
                    .sku_id
                    .as_ref()
                    .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少 SKU", line.line_no)))?;
                let revision_id = line
                    .sku_revision_id
                    .as_ref()
                    .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少 SKU 修订", line.line_no)))?;
                Ok((sku_id.to_string(), revision_id.to_string()))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(refs)
    }

    /// 批量执行公司商品池资格校验并对缺失引用 fail-closed。
    ///
    /// # 参数
    /// * `refs` - `(sku_id, sku_revision_id)` 列表
    /// * `executor` - 事务会话或 `NoTransaction`
    ///
    /// # 返回
    /// 全部引用仍可销售时返回 `Ok(())`。
    ///
    /// # 错误
    /// 任一引用不在当日可销售集合中时返回校验错误。
    pub(super) async fn ensure_sellable_refs(
        &self,
        refs: &[(String, String)],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if refs.is_empty() {
            return Ok(());
        }
        let expected = refs.iter().cloned().collect::<HashSet<_>>();
        let qualified = self
            .db
            .catalog()
            .find_sellable_sku_refs(refs, BusinessDate::today(), executor)
            .await?
            .into_iter()
            .map(|row| (row.sku_id, row.sku_revision_id))
            .collect::<HashSet<_>>();
        let mut invalid = expected
            .difference(&qualified)
            .map(|(sku_id, _)| sku_id.clone())
            .collect::<Vec<_>>();
        invalid.sort();
        if invalid.is_empty() {
            Ok(())
        } else {
            Err(crate::catalog::sellable_sku_invalid_error(&invalid))
        }
    }

    /// 作废销售单草稿（主状态 `DRAFT → VOIDED`；放弃有效工作副本）。
    ///
    /// # 参数
    /// * `id` - 销售单 ID
    /// * `req` - 作废请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回作废后的销售单详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 销售单不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    pub async fn void_sales_order(
        &self,
        id: &str,
        req: VoidSalesOrderRequest,
        actor: &AuditActor,
    ) -> Result<SalesOrderDetailView> {
        req.validate()?;
        let mut order = self
            .db
            .sales_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
        if order.base.version != req.version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        order.void(actor.id())?;
        let order_id = SalesOrderId::new(order.base.id.clone());
        let mut working_copy = self
            .db
            .sales_order_working_copies()
            .find_active_by_order_and_purpose(&order_id, WorkingPurpose::FirstSubmission, &mut NoTransaction)
            .await?;
        if let Some(copy) = &mut working_copy {
            copy.abandon()?;
        }
        let audit = actor
            .clone()
            .resource_log("sales_order.void", "sales_order", id.to_string())?;

        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.sales_orders().update(&mut order, session).await?;
                    if let Some(copy) = &mut working_copy {
                        db.sales_order_working_copies().update(copy, session).await?;
                    }
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        self.sales_order_detail(id, None).await
    }
}

/// 禁止回退 `CARD_SALES_APPROVAL` 或卡券专用工作项。
///
/// # 错误
/// 主体种类为旧卡券定义时返回冲突。
fn ensure_unified_start_command(start: &super::adapter::SalesOrderStartCommand) -> Result<()> {
    reject_legacy_card_sales_work_item("DOCUMENT_APPROVAL")?;
    if start.subject_kind == "CARD_SALES_APPROVAL" {
        return reject_legacy_card_sales_decision();
    }
    Ok(())
}

/// 读取销售单最新提交号，作为撤回查找实例的 `subject_version`。
///
/// # 错误
/// 没有提交时返回冲突。
async fn latest_submission_no(db: &mongodb::Database, sales_order_id: &str) -> Result<u32> {
    let submissions = db
        .sales_order_submissions()
        .find_many(
            mongodb::bson::doc! { "sales_order_id": sales_order_id },
            &mut NoTransaction,
        )
        .await?;
    submissions
        .iter()
        .map(|item| item.submission_no)
        .max()
        .ok_or_else(|| Error::ConflictError("销售单没有可撤回的提交版本".to_string()))
}

/// 将仓储的全部活动映射命中收敛为提交允许的精确唯一身份。
fn require_unique_card_external_identity(matches: Vec<String>, object_label: &str) -> Result<String> {
    match matches.as_slice() {
        [external_id] => Ok(external_id.clone()),
        [] => Err(Error::BusinessLogicError(format!(
            "目标商城缺少已确认的{object_label}外部身份映射，禁止提交"
        ))),
        _ => Err(Error::ConflictError(format!(
            "目标商城存在多个有效{object_label}外部身份映射，禁止提交"
        ))),
    }
}

#[cfg(test)]
mod card_projection_input_tests {
    use super::{
        require_unique_card_external_identity, sales_order_create_audit_id, sales_order_create_fingerprint,
        sales_submission_audit_id, sales_submission_fingerprint, Error,
    };
    use serde_json::json;

    #[test]
    fn unique_external_identity_accepts_exactly_one_mapping() {
        assert_eq!(
            require_unique_card_external_identity(vec!["mall-customer-1".to_string()], "客户").unwrap(),
            "mall-customer-1"
        );
    }

    #[test]
    fn unique_external_identity_rejects_missing_or_ambiguous_mapping() {
        assert!(matches!(
            require_unique_card_external_identity(Vec::new(), "客户"),
            Err(Error::BusinessLogicError(_))
        ));
        assert!(matches!(
            require_unique_card_external_identity(
                vec!["mall-customer-1".to_string(), "mall-customer-2".to_string()],
                "客户"
            ),
            Err(Error::ConflictError(_))
        ));
    }

    #[test]
    fn submission_idempotency_identity_is_stable_and_payload_bound() {
        let receipt = sales_submission_audit_id("actor-1", "order-1", "secret-request");
        assert_eq!(
            receipt,
            sales_submission_audit_id("actor-1", "order-1", "secret-request")
        );
        assert!(!receipt.contains("secret-request"));
        assert_ne!(
            sales_submission_fingerprint("actor-1", "order-1", 1, "secret-request"),
            sales_submission_fingerprint("actor-1", "order-1", 2, "secret-request")
        );
    }

    #[test]
    fn creation_idempotency_identity_is_stable_and_full_payload_bound() {
        let receipt = sales_order_create_audit_id("actor-1", " secret-request ");
        assert_eq!(receipt, sales_order_create_audit_id("actor-1", "secret-request"));
        assert!(!receipt.contains("secret-request"));

        let first =
            sales_order_create_fingerprint("actor-1", &json!({"order_no": "SO-1", "intent": "SAVE_DRAFT"}))
                .unwrap();
        assert_eq!(
            first,
            sales_order_create_fingerprint("actor-1", &json!({"order_no": "SO-1", "intent": "SAVE_DRAFT"}))
                .unwrap()
        );
        assert_ne!(
            first,
            sales_order_create_fingerprint("actor-1", &json!({"order_no": "SO-2", "intent": "SAVE_DRAFT"}))
                .unwrap()
        );
    }
}

#[cfg(test)]
mod goods_service_cutover_tests {
    /// 实物及服务提交不得再写入采购确认或旧待办。
    #[test]
    fn goods_service_submit_does_not_create_procurement_confirmation() {
        let source = include_str!("command.rs");
        let submit = source
            .split("pub async fn submit_sales_order")
            .nth(1)
            .and_then(|body| body.split("pub async fn cancel_approval_submission").next())
            .expect("提交方法");
        assert!(submit.contains("prepare_start"));
        assert!(submit.contains("start_approval_submission"));
        assert!(!submit.contains("ProcurementConfirmation::new"));
        assert!(!submit.contains("WorkItemType::ImportBusinessConfirmation"));
        assert!(!submit.contains("create_procurement_confirmation"));
        assert!(!submit.contains("CARD_SALES_APPROVAL"));
        assert!(!submit.contains("InternalApprovalRuntime"));
        assert!(!submit.contains("FailClosedApprovalActionPort"));
        assert!(!submit.contains("CardSalesManagerApproval"));
        assert!(!submit.contains("CardSalesOperationApproval"));
    }

    /// 卡券提交必须绑定 VoucherSalesOrder 并走统一启动，不得回退旧路径。
    #[test]
    fn voucher_create_binds_and_submit_starts_unified_approval() {
        let source = include_str!("command.rs");
        assert!(source.contains("sales_create_bind_command"));
        assert!(source.contains("document_type_for_sales_create"));
        let submit = source
            .split("pub async fn submit_sales_order")
            .nth(1)
            .and_then(|body| body.split("pub async fn cancel_approval_submission").next())
            .expect("提交方法");
        assert!(submit.contains("start_approval_submission"));
        assert!(!submit.contains("CARD_SALES_APPROVAL"));
        assert!(!submit.contains("InternalApprovalRuntime"));
        assert!(!submit.contains("submit_for_review"));
        let create = source
            .split("fn sales_create_bind_command")
            .nth(1)
            .and_then(|body| body.split("async fn persist_bound_sales_document").next())
            .expect("绑定命令");
        assert!(create.contains("document_type_for_sales_create"));
        assert!(!create.contains("DocumentType::SalesOrder"));
    }

    /// 撤回必须构造并调用统一取消端口。
    #[test]
    fn cancel_calls_unified_prepare_cancel() {
        let source = include_str!("command.rs");
        let cancel = source
            .split("pub async fn cancel_approval_submission")
            .nth(1)
            .and_then(|body| body.split("async fn replay_sales_submission").next())
            .expect("撤回方法");
        assert!(cancel.contains("prepare_cancel"));
        assert!(cancel.contains("build_sales_order_cancel_input"));
        assert!(cancel.contains("load_cancel_runtime"));
        assert!(cancel.contains("persist_sales_order_cancel"));
    }
}
