//! 创建命令的草稿与立即提交计划；准备阶段不写入业务事实。

use application_core::AuditActor;
use bpm::SubjectRef;
use erp_core::common::time::Instant;
use erp_core::ids::{BusinessDocumentId, SalesOrderId, SalesOrderSubmissionId, WorkflowActionId};
use erp_sales::dto::sales_order::SalesOrderCreateIntent;
use erp_sales::entity::command_receipt::SalesCommandResult;
use erp_sales::entity::sales_order::{
    SalesOrder, SalesOrderSubmission, SalesOrderSubmissionLine, SalesOrderWorkingCopyLine,
};
use erp_sales::service::sales_order::command::identity::sales_submission_audit_id;
use erp_sales::service::sales_order::mapper::{build_submission, build_submission_lines};
use erp_workflow::entity::approval_integration::ApprovalSubjectSnapshotPayload;
use erp_workflow::entity::document_registry::{WorkflowAction, WorkflowActionData, WorkflowActionType};
use id_generator::next_id;

use super::CreationIdentity;
use crate::order_to_cash::adapter::{
    RECENT_HISTORY_LIMIT, SalesApprovalPorts, build_sales_order_snapshot, execute_sales_order_domain_action,
    sales_approval_ports, sales_order_object_readable, sales_order_responsible_org_id,
    sales_order_start_command, start_approval_command_kind,
};
use crate::order_to_cash::command::create_prepare::PreparedSalesCreation;
use crate::order_to_cash::command::submit::ensure_unified_start_command;
use crate::order_to_cash::command_event::SalesCommandEvent;
use crate::order_to_cash::{SalesOrderCommandProcess, subject_ref_for_sales_business};
use crate::{Error, Result};

/// 不同建单意图保留各自的审批准备与事务写入顺序。
pub(super) enum CreationPlan {
    Draft(Box<CreationDraft>),
    Submit(Box<CreationSubmit>),
}

pub(super) struct CreationDraft {
    pub creation: PreparedSalesCreation,
    pub audit: SalesCommandEvent,
}

/// 首次冻结提交及其审批事实，全部由同一工作副本准备。
pub(super) struct CreationSubmit {
    pub creation: PreparedSalesCreation,
    pub submission: SalesOrderSubmission,
    pub submission_lines: Vec<SalesOrderSubmissionLine>,
    pub approval: CreatedApproval,
    pub create_audit: SalesCommandEvent,
    pub submit_audit: SalesCommandEvent,
}

pub(super) struct CreatedApproval {
    pub ports: SalesApprovalPorts,
    pub subject: SubjectRef,
    pub organization_id: String,
    pub idempotency_key: String,
    pub now: Instant,
    pub snapshot: ApprovalSubjectSnapshotPayload,
    pub workflow_action: WorkflowAction,
}

/// 已完成事前资格检查的审批上下文；事务内仍须重新校验。
struct ApprovalCreationContext {
    ports: SalesApprovalPorts,
    subject: SubjectRef,
    organization_id: String,
}

impl CreationPlan {
    /// 取得该计划使用的销售稳定对象。
    /// # 参数
    /// 无额外参数。
    /// # 返回
    /// 返回草稿或已进入审批状态的销售单。
    /// # 错误
    /// 本方法不返回错误。
    pub(super) fn order(&self) -> &SalesOrder {
        match self {
            Self::Draft(plan) => &plan.creation.order,
            Self::Submit(plan) => &plan.creation.order,
        }
    }

    /// 取得同一创建命令的独立回执事件。
    /// # 参数
    /// 无额外参数。
    /// # 返回
    /// 返回草稿或立即提交分支的创建事件。
    /// # 错误
    /// 本方法不返回错误。
    pub(super) fn create_event(&self) -> &SalesCommandEvent {
        match self {
            Self::Draft(plan) => &plan.audit,
            Self::Submit(plan) => &plan.create_audit,
        }
    }

    /// 取得当前工作副本行，供原子写入前校验商品引用。
    /// # 参数
    /// 无额外参数。
    /// # 返回
    /// 返回准备时保持原顺序的工作副本行。
    /// # 错误
    /// 本方法不返回错误。
    pub(super) fn working_copy_lines(&self) -> &[SalesOrderWorkingCopyLine] {
        match self {
            Self::Draft(plan) => &plan.creation.working_copy_lines,
            Self::Submit(plan) => &plan.creation.working_copy_lines,
        }
    }
}

impl SalesOrderCommandProcess {
    /// 先检查提交资格，再调用领域构造首次提交和审批快照。
    /// # 参数
    /// `creation` 为已准备的对象；`intent`、`identity` 和 `actor` 来自同一认证命令。
    /// # 返回
    /// 返回草稿或立即提交计划，尚未持久化。
    /// # 错误
    /// 按端口、主体、组织、采购责任、提交构造与事件构造的顺序返回首个错误。
    pub(super) async fn prepare_creation_plan(
        &self,
        creation: PreparedSalesCreation,
        intent: SalesOrderCreateIntent,
        identity: &CreationIdentity,
        actor: &AuditActor,
    ) -> Result<CreationPlan> {
        if intent == SalesOrderCreateIntent::Submit {
            let ports = sales_approval_ports(creation.order.business_type)?;
            let subject =
                subject_ref_for_sales_business(creation.order.business_type, &creation.order.base.id)
                    .map_err(|error| Error::ValidationError(error.to_string()))?;
            let organization_id = sales_order_responsible_org_id(&creation.order)?;
            let _ = sales_order_object_readable(&organization_id, actor.id())?;
            self.ensure_procurement_responsibility_before_submit(
                &creation.order,
                &creation.working_copy_lines,
            )
            .await?;
            let context = ApprovalCreationContext { ports, subject, organization_id };
            return Ok(CreationPlan::Submit(Box::new(prepare_submitted_creation(
                creation, context, identity, actor,
            )?)));
        }
        let audit = SalesCommandEvent::new(
            identity.audit_id.clone(),
            actor,
            &identity.idempotency_key,
            identity.fingerprint.clone(),
            SalesCommandResult::Created { sales_order_id: SalesOrderId::new(creation.order.base.id.clone()) },
            creation.order.order_no.clone(),
        )?;
        Ok(CreationPlan::Draft(Box::new(CreationDraft { creation, audit })))
    }
}

/// 调用领域迁移及既有快照构造，保持首次提交准备的首错顺序。
fn prepare_submitted_creation(
    mut creation: PreparedSalesCreation,
    context: ApprovalCreationContext,
    identity: &CreationIdentity,
    actor: &AuditActor,
) -> Result<CreationSubmit> {
    let submission = build_submission(&creation.working_copy, &creation.working_copy_lines, 1, actor)?;
    let submission_lines = build_submission_lines(&submission, &creation.working_copy_lines)?;
    creation.working_copy.submit()?;
    execute_sales_order_domain_action(&mut creation.order, context.ports.on_approval_start, actor.id())?;
    let now = Instant::now();
    let snapshot =
        build_sales_order_snapshot(&creation.order, &submission, &submission_lines, actor.id(), now)?;
    let start = sales_order_start_command(
        context.ports.document_type,
        &creation.order.base.id,
        submission.submission_no,
        actor.id(),
        &identity.idempotency_key,
    );
    ensure_unified_start_command(&start)?;
    let _ = (start_approval_command_kind(&start), RECENT_HISTORY_LIMIT);
    let workflow_action = created_submission_action(&creation.order, actor)?;
    let (create_audit, submit_audit) =
        creation_submission_events(identity, actor, &creation.order, &submission)?;
    let approval = CreatedApproval {
        ports: context.ports,
        subject: context.subject,
        organization_id: context.organization_id,
        idempotency_key: identity.idempotency_key.clone(),
        now,
        snapshot,
        workflow_action,
    };
    Ok(CreationSubmit { creation, submission, submission_lines, approval, create_audit, submit_audit })
}

/// 通过工作流实体构造对应首次提交的动作事实。
fn created_submission_action(order: &SalesOrder, actor: &AuditActor) -> Result<WorkflowAction> {
    Ok(WorkflowAction::new(
        WorkflowActionId::new(next_id()),
        WorkflowActionData {
            document_id: BusinessDocumentId::new(order.base.id.clone()),
            action_type: WorkflowActionType::Submit,
            from_status: "DRAFT".to_string(),
            to_status: "PENDING_REVIEW".to_string(),
            actor_id: actor.id().to_string(),
            actor_role: "role-sales".to_string(),
            comment: None,
        },
    )?)
}

/// 创建事件先构造、提交事件后构造，共用外层命令及连续事件序号。
fn creation_submission_events(
    identity: &CreationIdentity,
    actor: &AuditActor,
    order: &SalesOrder,
    submission: &SalesOrderSubmission,
) -> Result<(SalesCommandEvent, SalesCommandEvent)> {
    let create = SalesCommandEvent::new(
        identity.audit_id.clone(),
        actor,
        &identity.idempotency_key,
        identity.fingerprint.clone(),
        SalesCommandResult::Created { sales_order_id: SalesOrderId::new(order.base.id.clone()) },
        order.order_no.clone(),
    )?
    .with_command_sequence(&identity.audit_id, 1)?;
    let submit = SalesCommandEvent::new(
        sales_submission_audit_id(actor.id(), &order.base.id, &identity.idempotency_key),
        actor,
        &identity.idempotency_key,
        identity.fingerprint.clone(),
        SalesCommandResult::Submitted {
            sales_order_id: SalesOrderId::new(order.base.id.clone()),
            submission_id: SalesOrderSubmissionId::new(submission.base.id.clone()),
        },
        order.order_no.clone(),
    )?
    .with_command_sequence(&identity.audit_id, 2)?;
    Ok((create, submit))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use application_core::CommandFingerprint;
    use erp_core::ids::{CustomerAccountId, PartyId};
    use erp_core::money::{Amount, Quantity};
    use erp_core::{AccountKind, Error as CoreError};
    use erp_sales::dto::sales_order::SalesOrderDraftRequest;
    use erp_sales::entity::sales_order::{
        BusinessType, CommercialStatus, OriginSystem, ReviewStatus, SalesOrderData,
    };
    use erp_sales::service::sales_order::command::identity::sales_order_create_audit_id;
    use erp_sales::service::sales_order::mapper::{build_stable_lines, build_working_copy};
    use erp_workflow::entity::document_registry::{BusinessDocument, BusinessDocumentData, DocumentType};
    use serde_json::{Value, json};

    use super::*;

    fn actor() -> AuditActor {
        AuditActor::new("creator".into(), "sales".into(), AccountKind::Admin)
    }

    fn identity(actor: &AuditActor) -> CreationIdentity {
        CreationIdentity {
            idempotency_key: "key".into(),
            audit_id: sales_order_create_audit_id(actor.id(), "key"),
            fingerprint: "a".repeat(64),
            key_hash: CommandFingerprint::from_parts(["key".into()]),
        }
    }

    fn goods_line(number: u32) -> Value {
        json!({
            "line_no": number, "line_type": "GOODS_SERVICE", "sales_tax_rate": "0",
            "item_name_snapshot": format!("商品-{number}"), "unit_snapshot": "件",
            "goods": {
                "sku_id": format!("sku-{number}"), "sku_revision_id": format!("revision-{number}"),
                "welfare_scenario": "ANNUAL_GIFT_BAG", "service_region": "上海",
                "fulfillment_due_at": 1800000000, "quantity": "2", "base_unit_code": "件",
                "unit_price_gross": "5"
            }
        })
    }

    fn voucher_line() -> Value {
        json!({
            "line_no": 1, "line_type": "VOUCHER", "sales_tax_rate": "0",
            "item_name_snapshot": "电子券", "unit_snapshot": "张",
            "voucher": {
                "face_value": "100", "card_count": 3, "unit_price_gross": "90",
                "face_value_total": "300", "transaction_amount": "270", "gift_amount": "30",
                "gift_rate": null, "card_form": "ELECTRONIC"
            }
        })
    }

    /// 夹具经真实领域构造与 mapper 生成，提交准备直接执行生产方法。
    fn creation(business_type: BusinessType, actor: &AuditActor) -> PreparedSalesCreation {
        let voucher = business_type == BusinessType::Voucher;
        let order = SalesOrder::new(
            SalesOrderId::new("order"),
            SalesOrderData {
                order_no: "SO-1".into(),
                sales_owner_user_id: actor.id().into(),
                business_org_unit_id: "org".into(),
                business_type,
                origin_system: OriginSystem::Erp,
                source_identity_id: None,
                customer_id: CustomerAccountId::new("customer"),
                contract_id: None,
                settlement_party_id: PartyId::new("party"),
                source_status_code: None,
            },
            actor.id(),
        )
        .unwrap();
        let draft: SalesOrderDraftRequest = serde_json::from_value(json!({
            "editor_user_id": actor.id(), "customer_name": "客户", "settlement_party_name": "客户",
            "payment_term_code": "PREPAID", "payment_term_name": "预付", "invoice_type": "普通发票",
            "tax_point": "结算", "voucher_category_sku_id": voucher.then_some("voucher-category"),
            "voucher_expiry_at": voucher.then_some(1850000000_u64),
            "receivable_due_date": voucher.then_some("2026-12-31"),
            "lines": if voucher { vec![voucher_line()] } else { vec![goods_line(2), goods_line(1)] }
        }))
        .unwrap();
        let stable_lines = build_stable_lines(&SalesOrderId::new("order"), &draft.lines).unwrap();
        let (working_copy, working_copy_lines) =
            build_working_copy(&order, &stable_lines, &draft, 1, actor).unwrap();
        let document = BusinessDocument::new(
            BusinessDocumentId::new("order"),
            BusinessDocumentData {
                document_type: sales_approval_ports(business_type).unwrap().document_type,
                document_no: "SO-1".into(),
            },
        )
        .unwrap();
        PreparedSalesCreation { order, document, stable_lines, working_copy, working_copy_lines }
    }

    fn context(creation: &PreparedSalesCreation) -> ApprovalCreationContext {
        ApprovalCreationContext {
            ports: sales_approval_ports(creation.order.business_type).unwrap(),
            subject: subject_ref_for_sales_business(creation.order.business_type, &creation.order.base.id)
                .unwrap(),
            organization_id: sales_order_responsible_org_id(&creation.order).unwrap(),
        }
    }

    /// 两类新单都冻结同一副本、真实金额及顺序，并登记原创建和首次提交身份。
    #[test]
    fn creation_submission_freezes_first_copy_and_original_command_results() {
        let actor = actor();
        let identity = identity(&actor);
        for (business_type, document_type, line_nos, gross, quantity) in [
            (BusinessType::GoodsService, DocumentType::SalesOrder, vec![2, 1], "20", "4"),
            (BusinessType::Voucher, DocumentType::VoucherSalesOrder, vec![1], "270", "3"),
        ] {
            let creation = creation(business_type, &actor);
            let working_copy_id = creation.working_copy.base.id.clone();
            let context = context(&creation);
            let submitted = prepare_submitted_creation(creation, context, &identity, &actor).unwrap();
            assert_eq!(submitted.creation.order.commercial_status, CommercialStatus::PendingReview);
            assert_eq!(submitted.creation.order.review_status, ReviewStatus::InApproval);
            assert_eq!(submitted.creation.order.stable.created_by, actor.id());
            assert!(submitted.creation.working_copy.is_submitted());
            assert_eq!(submitted.submission.working_copy_id.as_ref(), working_copy_id);
            assert_eq!(submitted.submission.submission_no, 1);
            assert_eq!(submitted.submission.submitted_by, actor.id());
            assert_eq!(submitted.submission.gross_amount, Amount::from_str(gross).unwrap());
            assert_eq!(
                submitted.submission_lines.iter().map(|line| line.line_no).collect::<Vec<_>>(),
                line_nos
            );
            assert_eq!(submitted.approval.ports.document_type, document_type);
            assert_eq!(submitted.approval.snapshot.total_amount, Some(Amount::from_str(gross).unwrap()));
            assert_eq!(
                submitted.approval.snapshot.total_quantity,
                Some(Quantity::from_str(quantity).unwrap())
            );
            assert_eq!(submitted.approval.workflow_action.actor_id, actor.id());
            assert_eq!(submitted.create_audit.receipt.base.id, identity.audit_id);
            assert_eq!(submitted.create_audit.receipt.fingerprint, identity.fingerprint);
            assert_eq!(
                submitted.create_audit.receipt.result,
                SalesCommandResult::Created { sales_order_id: SalesOrderId::new("order") }
            );
            assert_eq!(
                submitted.submit_audit.receipt.result,
                SalesCommandResult::Submitted {
                    sales_order_id: SalesOrderId::new("order"),
                    submission_id: SalesOrderSubmissionId::new(submitted.submission.base.id),
                }
            );
        }
    }

    /// 已放弃副本和不可提交销售单同时出现时，先返回副本锁定错误。
    #[test]
    fn creation_submission_stops_on_copy_transition_before_order_transition() {
        let actor = actor();
        let mut creation = creation(BusinessType::GoodsService, &actor);
        let context = context(&creation);
        creation.working_copy.abandon().unwrap();
        creation.order.commercial_status = CommercialStatus::Voided;
        let result = prepare_submitted_creation(creation, context, &identity(&actor), &actor);
        assert!(matches!(result, Err(Error::Logic(CoreError::InvalidStateTransition { from, to }))
            if from == "Abandoned" && to == "Submitted"));
    }

    /// 行字段缺失先于副本迁移失败，错误仍按请求中第一行报告。
    #[test]
    fn creation_submission_keeps_first_line_error_before_copy_transition() {
        let actor = actor();
        let mut creation = creation(BusinessType::GoodsService, &actor);
        let context = context(&creation);
        creation.working_copy.abandon().unwrap();
        for line in &mut creation.working_copy_lines {
            line.sku_id = None;
        }
        let result = prepare_submitted_creation(creation, context, &identity(&actor), &actor);
        assert!(matches!(result, Err(Error::Logic(CoreError::LogicError(message)))
            if message == "第 2 行缺少商品字段组"));
    }
}
