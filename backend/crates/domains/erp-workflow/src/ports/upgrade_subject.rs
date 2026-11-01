//! 审批绑定升级使用的强业务对象事实。

use async_trait::async_trait;
use persistence_core::Executor;

use crate::entity::document_registry::DocumentType;
use crate::error::Result;

/// Approval binding upgrade facts from the strong business object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalUpgradeSubjectFacts {
    /// Request type after business-nature verification.
    pub document_type: DocumentType,
    /// Strong business-object id.
    pub document_id: String,
    /// Strong business-object `BaseModel.version`.
    pub business_object_version: u64,
    /// Formal document no when assigned.
    pub document_no: String,
    /// Responsible organization from the object or parent chain.
    pub responsible_org_id: String,
    /// Immutable creator.
    pub creator_id: String,
}

impl ApprovalUpgradeSubjectFacts {
    /// 拒绝客户端提交的过期业务对象版本。
    ///
    /// # 参数
    /// * `expected` - 客户端提交的期望版本。
    ///
    /// # 返回
    /// 与强对象 `business_object_version` 一致时成功。
    ///
    /// # 错误
    /// 版本不一致时返回业务对象版本冲突。
    pub fn ensure_expected_business_object_version(&self, expected: u64) -> crate::error::Result<()> {
        if self.business_object_version != expected {
            return Err(crate::error::Error::version_conflict("业务对象"));
        }
        Ok(())
    }
}

/// Loads upgrade facts from owning business domains.
#[async_trait]
pub trait UpgradeSubjectPort: Send + Sync {
    /// 加载需走流程的单据的强业务对象事实。
    ///
    /// # 参数
    /// * `document_type` - 精确单据类型。
    /// * `document_id` - 精确业务对象 ID。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 返回身份、版本、单据编号、责任组织和创建人。
    ///
    /// # 错误
    /// 实体缺失、类型不匹配，或创建人、组织事实不完整时返回错误。
    async fn load(
        &self,
        document_type: DocumentType,
        document_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<ApprovalUpgradeSubjectFacts>;

    /// 对已加载事实执行仅适用于未提交新单的门禁。
    ///
    /// # 参数
    /// * `facts` - 已经加载的强业务对象事实。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 事实仍处于允许初次绑定的未提交状态时成功。
    ///
    /// # 错误
    /// 事实不满足未提交门禁，或实现无法完成判定时返回错误。
    async fn ensure_initial_unsubmitted(
        &self,
        facts: &ApprovalUpgradeSubjectFacts,
        executor: &mut dyn Executor,
    ) -> Result<()>;
}

/// Fail-closed upgrade port used when composition has not injected a domain adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedUpgradeSubjectPort;

#[async_trait]
impl UpgradeSubjectPort for FailClosedUpgradeSubjectPort {
    async fn load(
        &self,
        _document_type: DocumentType,
        _document_id: &str,
        _executor: &mut dyn Executor,
    ) -> Result<ApprovalUpgradeSubjectFacts> {
        Err(crate::error::Error::ValidationError("审批升级对象读取未接线，已按安全策略拒绝".to_string()))
    }

    async fn ensure_initial_unsubmitted(
        &self,
        _facts: &ApprovalUpgradeSubjectFacts,
        _executor: &mut dyn Executor,
    ) -> Result<()> {
        Err(crate::error::Error::ValidationError("审批升级对象读取未接线，已按安全策略拒绝".to_string()))
    }
}
