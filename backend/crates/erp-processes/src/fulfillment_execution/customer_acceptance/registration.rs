//! 客户验收 NO_APPROVAL 单据登记；所有写入使用调用方 Executor。
use application_core::AuditActor;
use erp_fulfillment::entity::fulfillment::CustomerAcceptance;
use erp_identity::SharedRbacService;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::entity::document_registry::{BusinessDocument, DocumentType};
use erp_workflow::service::approval::binding::{
    BindPublishedDefinitionCommand, BindingDecision, binding_decision,
};
use erp_workflow::service::approval::business_adapter::{BindingRevalidationContext, adapter_spec_of};
use erp_workflow::service::approval::policy::{DocumentApprovalPolicy, policy_of};
use erp_workflow::service::document_registry::{new_registered_document, persist_registered_document};
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};
/// 客户验收创建必须跳过绑定：政策只能是 `NO_APPROVAL`。
///
/// # 返回
/// 返回 `SkipNoApproval`。
///
/// # 错误
/// 政策缺失或误登记为必须审批时返回部署不变量错误。
fn customer_acceptance_create_binding_decision() -> Result<BindingDecision> {
    let policy = policy_of(DocumentType::CustomerAcceptance)?;
    match &policy {
        DocumentApprovalPolicy::NoApproval(no_approval) => {
            if no_approval.document_type != DocumentType::CustomerAcceptance {
                return Err(Error::Internal("客户验收政策类型不匹配".to_string()));
            }
            Ok(binding_decision(policy.requirement()))
        },
        DocumentApprovalPolicy::ProcessRequired(_) => {
            Err(Error::Internal("客户验收必须是 NO_APPROVAL，不得绑定流程".to_string()))
        },
    }
}

/// 确认客户验收创建路径不得查询发布定义。
///
/// # 错误
/// 绑定决定不是跳过时返回错误。
fn ensure_customer_acceptance_skips_approval_binding() -> Result<BindingDecision> {
    let decision = customer_acceptance_create_binding_decision()?;
    if decision != BindingDecision::SkipNoApproval {
        return Err(Error::Internal("客户验收创建必须跳过审批绑定".to_string()));
    }
    Ok(decision)
}

/// 客户验收不得注册空审批适配器。
///
/// # 错误
/// 适配器登记存在时返回部署不变量错误。
fn ensure_customer_acceptance_has_no_adapter() -> Result<()> {
    if adapter_spec_of(DocumentType::CustomerAcceptance).is_ok() {
        return Err(Error::Internal("客户验收不得注册审批适配器".to_string()));
    }
    Ok(())
}

/// 验收所属销售单作为绑定上下文组织，不得用空串补位。
///
/// # 参数
/// * `acceptance` - 待登记客户验收单
///
/// # 返回
/// 返回非空销售单标识。
///
/// # 错误
/// 销售单为空时返回校验错误。
fn customer_acceptance_binding_organization_id(acceptance: &CustomerAcceptance) -> Result<String> {
    let org = acceptance.sales_order_id.to_string();
    if org.trim().is_empty() {
        return Err(Error::ValidationError("客户验收单缺少销售单，无法构造绑定上下文".to_string()));
    }
    Ok(org)
}

/// 构造客户验收创建绑定命令。客户端不得提交定义 ID。
///
/// # 参数
/// * `acceptance` - 待登记客户验收单
/// * `creator_id` - 创建人
///
/// # 错误
/// 销售单为空时返回校验错误。
fn customer_acceptance_bind_command(
    acceptance: &CustomerAcceptance,
    creator_id: &str,
) -> Result<BindPublishedDefinitionCommand> {
    Ok(BindPublishedDefinitionCommand {
        document_type: DocumentType::CustomerAcceptance,
        business_object_id: acceptance.base.id.clone(),
        business_object_version: acceptance.base.version,
        context: BindingRevalidationContext::new(
            customer_acceptance_binding_organization_id(acceptance)?,
            creator_id.to_string(),
        ),
    })
}

/// 将绑定端口返回值落实为客户验收注册行：空绑定保持未绑定。
///
/// # 参数
/// * `document` - 客户验收注册行
/// * `binding` - 统一绑定端口返回值
///
/// # 返回
/// 固定返回 `None`。
///
/// # 错误
/// 端口返回绑定或注册行已预置绑定时返回错误。
fn apply_customer_acceptance_create_binding(
    document: &mut BusinessDocument,
    binding: Option<ApprovalDefinitionBinding>,
) -> Result<Option<ApprovalDefinitionBinding>> {
    if binding.is_some() {
        return Err(Error::Internal("客户验收为 NO_APPROVAL，不得写入审批绑定".to_string()));
    }
    if document.approval_binding.is_some() {
        return Err(Error::Internal("客户验收注册行不得预置审批绑定".to_string()));
    }
    if document.document_type != DocumentType::CustomerAcceptance {
        return Err(Error::Internal("客户验收创建只能注册 CustomerAcceptance 单据".to_string()));
    }
    Ok(None)
}

/// 在调用方事务内登记客户验收单据并证明空绑定。
///
/// 必须先确认政策跳过，再调用统一绑定端口；不得查询发布定义后假装成功。
///
/// # 错误
/// 政策非无审批、端口返回绑定或写入失败时返回错误。
async fn persist_unbound_customer_acceptance_document(
    db: &Database,
    rbac: &SharedRbacService,
    object_read: &dyn erp_workflow::ApprovalObjectReadPort,
    mut document: BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let _ = ensure_customer_acceptance_skips_approval_binding()?;
    ensure_customer_acceptance_has_no_adapter()?;
    let binding = crate::adapters::workflow::bind_published_definition_on_document_create(
        db,
        rbac,
        object_read,
        bind_command,
        actor,
        executor,
    )
    .await?;
    apply_customer_acceptance_create_binding(&mut document, binding)?;
    persist_registered_document(db, &document, executor).await.map_err(crate::Error::from)
}

/// 为已构造客户验收登记 `BusinessDocument` 并调用统一绑定端口。
///
/// # 错误
/// 绑定端口或注册写入失败时返回错误。
pub async fn register_created_customer_acceptance_document(
    db: &Database,
    rbac: &SharedRbacService,
    object_read: &dyn erp_workflow::ApprovalObjectReadPort,
    acceptance: &CustomerAcceptance,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let bind_command = customer_acceptance_bind_command(acceptance, actor.id())?;
    let document = new_registered_document(
        &acceptance.base.id,
        DocumentType::CustomerAcceptance,
        acceptance.acceptance_no.clone(),
    )
    .map_err(crate::Error::from)?;
    persist_unbound_customer_acceptance_document(
        db,
        rbac,
        object_read,
        document,
        &bind_command,
        actor,
        executor,
    )
    .await
}

#[cfg(test)]
mod customer_acceptance_no_approval_tests {
    use bpm::ProcessKind;
    use bpm::ids::ApprovalProcessDefinitionId;
    use erp_core::common::time::Instant;
    use erp_core::ids::{CustomerAcceptanceId, SalesOrderId};
    use erp_fulfillment::entity::fulfillment::{AcceptanceResult, CustomerAcceptanceData};
    use erp_workflow::service::approval::binding::binding_from_published;
    use erp_workflow::service::document_registry::new_registered_document;

    use super::{
        BindingDecision, CustomerAcceptance, DocumentApprovalPolicy, DocumentType,
        apply_customer_acceptance_create_binding, customer_acceptance_bind_command,
        customer_acceptance_create_binding_decision, ensure_customer_acceptance_has_no_adapter,
        ensure_customer_acceptance_skips_approval_binding, policy_of,
    };

    fn draft_acceptance() -> CustomerAcceptance {
        CustomerAcceptance::new(
            CustomerAcceptanceId::new("ca-1"),
            CustomerAcceptanceData {
                acceptance_no: "CA-1".into(),
                sales_order_id: SalesOrderId::new("so-1"),
                accepted_at: Instant::from_unix_secs(1_700_000_000),
                result: AcceptanceResult::Passed,
            },
        )
        .expect("草稿必须可构造")
    }

    /// 政策仅含 document_type、approval_requirement、process_kind，不得注册空 Adapter。
    #[test]
    fn customer_acceptance_policy_is_no_approval_identity_only() {
        let policy = policy_of(DocumentType::CustomerAcceptance).expect("客户验收政策必须存在");
        let DocumentApprovalPolicy::NoApproval(no_approval) = &policy else {
            panic!("客户验收必须是 NO_APPROVAL");
        };
        assert_eq!(no_approval.document_type, DocumentType::CustomerAcceptance);
        assert_eq!(no_approval.process_kind, ProcessKind::CustomerAcceptance);
        assert_eq!(
            customer_acceptance_create_binding_decision().expect("绑定决定"),
            BindingDecision::SkipNoApproval
        );
        assert_eq!(
            ensure_customer_acceptance_skips_approval_binding().expect("必须跳过"),
            BindingDecision::SkipNoApproval
        );
        ensure_customer_acceptance_has_no_adapter().expect("不得注册空适配器");
    }

    /// 创建必须注册 BusinessDocument，绑定端口返回空，禁止写入绑定。
    #[test]
    fn create_registers_document_and_returns_empty_binding() {
        let acceptance = draft_acceptance();
        let command = customer_acceptance_bind_command(&acceptance, "admin-1").expect("绑定命令");
        assert_eq!(command.document_type, DocumentType::CustomerAcceptance);
        assert_eq!(command.business_object_id, acceptance.base.id);
        assert_eq!(command.context.organization_id, "so-1");

        let mut document = new_registered_document(
            &acceptance.base.id,
            DocumentType::CustomerAcceptance,
            acceptance.acceptance_no.clone(),
        )
        .expect("可注册");
        assert!(document.approval_binding.is_none());
        let empty = apply_customer_acceptance_create_binding(&mut document, None).expect("空绑定");
        assert!(empty.is_none());
        assert!(document.approval_binding.is_none());

        let forged =
            binding_from_published(ApprovalProcessDefinitionId::new("def-1"), 1, Instant::from_unix_secs(10))
                .expect("测试绑定");
        assert!(apply_customer_acceptance_create_binding(&mut document, Some(forged)).is_err());
    }
}
