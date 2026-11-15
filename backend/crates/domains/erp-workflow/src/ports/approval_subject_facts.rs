//! 审批授权所需真实业务源由组合根提供；工作流不反向依赖业务领域。
use async_trait::async_trait;
use erp_identity::Result;
use erp_identity::entity::authorization::{AccessContext, RelatedObjects};
use persistence_core::Executor;

/// 按冻结单据类型和业务主键读取真实授权源及完整强外键。
#[async_trait]
pub trait ApprovalSubjectFactsPort: Send + Sync {
    /// 读取审批管理要求的完整来源。
    /// # 参数
    /// context 为同次授权身份；document_type/business_id 来自不可变审批快照；executor 沿用原事务。
    /// # 返回
    /// expected_ids 为业务强外键，不得从成功读取的对象集合反推；objects 保留实际资源和版本。
    /// # 错误
    /// 类型未接入、源身份不完整或持久化失败时返回错误，不使用展示根或结算主体替代。
    async fn load(
        &self,
        context: &AccessContext,
        document_type: &str,
        business_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<RelatedObjects>;
}
