//! 升级主体事实由 [`crate::ports::UpgradeSubjectPort`] 适配器加载。

use persistence_core::Executor;

use crate::entity::document_registry::DocumentType;
use crate::error::Result;
pub use crate::ports::ApprovalUpgradeSubjectFacts;
use crate::ports::UpgradeSubjectPort;

/// 经注入端口加载强业务主体事实。
/// 返回端口读到的 `ApprovalUpgradeSubjectFacts`。
///
/// 端口加载失败时返回对应错误。
pub async fn load_approval_upgrade_subject_facts(
    port: &dyn UpgradeSubjectPort,
    document_type: DocumentType,
    document_id: &str,
    executor: &mut dyn Executor,
) -> Result<ApprovalUpgradeSubjectFacts> {
    port.load(document_type, document_id, executor).await
}

/// 经注入端口执行仅 Fresh 路径的未提交门禁。
/// 门禁通过时无返回值。
///
/// 端口判定不满足未提交门禁时返回对应错误。
///
/// `facts`：已加载的强业务主体事实。
pub async fn ensure_initial_unsubmitted_approval_upgrade_subject(
    port: &dyn UpgradeSubjectPort,
    facts: &ApprovalUpgradeSubjectFacts,
    executor: &mut dyn Executor,
) -> Result<()> {
    port.ensure_initial_unsubmitted(facts, executor).await
}

impl ApprovalUpgradeSubjectFacts {
    /// 由强业务主体事实构造绑定重验上下文。
    /// 返回带责任组织与创建人的 `BindingRevalidationContext`。
    pub fn binding_context(&self) -> crate::service::approval::business_adapter::BindingRevalidationContext {
        crate::service::approval::business_adapter::BindingRevalidationContext::new(
            self.responsible_org_id.clone(),
            self.creator_id.clone(),
        )
    }
}
