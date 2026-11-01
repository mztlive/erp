//! 审批绑定与重验使用的对象读取判定端口。

use crate::entity::document_registry::DocumentType;
use crate::error::Result;

/// Domain object-read decisions used by approval binding.
pub trait ApprovalObjectReadPort: Send + Sync {
    /// 返回 `assignee_user_id` 能否读取该主体。
    ///
    /// # 参数
    /// * `document_type` - 已冻结的单据类型。
    /// * `organization_id` - 单据责任组织。
    /// * `creator_id` - 单据创建人。
    /// * `assignee_user_id` - 候选审批人。
    ///
    /// # 返回
    /// 领域适配器已接线时返回 `Some(true)` 或 `Some(false)`；未接线时返回 `None`。
    ///
    /// # 错误
    /// 组织或审批人为空，或领域适配器失败时返回错误。
    fn object_read_decision(
        &self,
        document_type: DocumentType,
        organization_id: &str,
        creator_id: &str,
        assignee_user_id: &str,
    ) -> Result<Option<bool>>;
}

/// Fail-closed object-read port used when composition has not injected a domain adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedObjectReadPort;

impl ApprovalObjectReadPort for FailClosedObjectReadPort {
    fn object_read_decision(
        &self,
        _document_type: DocumentType,
        organization_id: &str,
        _creator_id: &str,
        assignee_user_id: &str,
    ) -> Result<Option<bool>> {
        if organization_id.trim().is_empty() || assignee_user_id.trim().is_empty() {
            return Err(crate::error::Error::ValidationError("单据组织或审批人不能为空".to_string()));
        }
        Ok(None)
    }
}
