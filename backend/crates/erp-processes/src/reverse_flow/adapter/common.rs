//! 四类逆向单据审批 Adapter 共用的规格校验。
//!
//! 客户退款、供应商退款、回款冲正与付款冲正的规格校验仅单据类型、三类动作、
//! 责任角色与错误各不相同，共用字段（版本来源、组织来源、读取范围、金额快照）
//! 由本模块统一校验；各单据的 `*_from_spec` 只声明差异并构造自有结构体。

use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::service::approval::business_adapter::{AdapterReadScope, ApprovalAdapterSpec};
use erp_workflow::service::approval::policy::{
    ApprovalDomainAction, ApprovalSubjectSnapshotField, ApprovalSubjectVersionSource, OwnerOrganizationSource,
};
use erp_workflow::service::approval::process_kind::process_kind_of;

use crate::{Error, Result};

/// 逆向单据 Adapter 随单据种类变化的期望登记值。
///
/// 共用规格（版本来源、组织来源、读取范围、金额快照字段）不在本结构声明；
/// 新增逆向单据类型只需新增一组期望声明并复用 [`ensure_reverse_adapter_spec`]。
pub struct ExpectedReverseAdapterContracts {
    /// 单据类型。
    pub document_type: DocumentType,
    /// 提交并启动动作。
    pub on_approval_start: ApprovalDomainAction,
    /// 最终通过动作。
    pub on_final_approve: ApprovalDomainAction,
    /// 撤回与受阻取消动作。
    pub cancel_action: ApprovalDomainAction,
    /// WorkItem 责任角色。
    pub owner_role: &'static str,
    /// 登记不完整时的内部错误文案。
    pub mismatch_message: &'static str,
}

/// 校验逆向单据审批适配器登记的共用规格。
///
/// # 参数
/// * `spec` - 政策登记的适配器规格
/// * `expected` - 随单据种类变化的期望登记值
///
/// # 返回
/// 登记完整时返回 `()`。
///
/// # 错误
/// 共用字段或期望字段与合同签署值不一致时返回部署不变量错误。
pub fn ensure_reverse_adapter_spec(
    spec: &ApprovalAdapterSpec,
    expected: &ExpectedReverseAdapterContracts,
) -> Result<()> {
    if spec.document_type != expected.document_type
        || spec.process_kind != process_kind_of(expected.document_type)
        || spec.subject_version_source != ApprovalSubjectVersionSource::EntityApprovalSubjectVersion
        || spec.on_approval_start != expected.on_approval_start
        || spec.on_final_approve != expected.on_final_approve
        || spec.cancel_action != expected.cancel_action
        || spec.owner_role.as_str() != expected.owner_role
        || spec.owner_organization_source != OwnerOrganizationSource::SubjectSnapshotResponsibleOrgId
        || spec.read_scope != AdapterReadScope::DocumentOrganizationAndCreator
        || !spec.subject_snapshot_fields.contains(&ApprovalSubjectSnapshotField::TotalAmount)
    {
        return Err(Error::Internal(expected.mismatch_message.to_string()));
    }
    Ok(())
}
