//! 客户存在性、编号与创建资格的消费方端口。

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::ids::{CustomerAccountId, PartyId};
use erp_identity::entity::authorization::ObjectFacts;
use persistence_core::Executor;

use crate::error::{Error, Result};

/// 合同创建所需的真实客户来源资格；在组合层完成跨域查询。
#[async_trait]
pub trait ContractCreationPort: Send + Sync {
    /// 在原事务内读取指定客户并构造合同候选归属。
    /// 从实际客户当前负责人和组织构造、无持久化版本的合同候选事实。
    /// actor 为真实操作人，customer_id 为完整客户来源，executor 为原执行器。
    /// 来源不存在、无权限、版本变化或事实读取失败时拒绝。
    async fn candidate(
        &self,
        actor: &AuditActor,
        customer_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<ObjectFacts>;
}

/// Minimum customer facts required to archive or list contracts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomerAccountFact {
    /// Customer role stable id.
    pub id: String,
    /// Customer number shown on contract lists.
    pub customer_no: String,
    /// Settlement-party default when upload omits `settlement_party_id`.
    pub party_id: PartyId,
    /// Whether the customer may receive new archives.
    pub is_active: bool,
}

/// Port contract uses to read customer identity without depending on `erp-customer`.
#[async_trait]
pub trait CustomerFactsPort: Send + Sync {
    /// 读取一条未删除的客户角色。
    /// 客户不存在或已删除时返回 `None`。
    /// `customer_id` 为客户角色 ID。
    /// 传播适配器查询错误。
    async fn find_by_id(&self, customer_id: &CustomerAccountId) -> Result<Option<CustomerAccountFact>>;

    /// 按 ID 读取未删除的客户角色。缺失的 ID 省略。
    /// 仍存在的客户事实。缺失的 ID 不出现。
    /// `customer_ids` 为客户角色 ID；`executor` 为调用方执行器，用于核对来源是否存在。
    /// 传播适配器查询错误。
    async fn find_by_ids(
        &self,
        customer_ids: &[CustomerAccountId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<CustomerAccountFact>>;
}

/// Empty customer lookup used by isolated unit tests.
#[derive(Debug, Default, Clone, Copy)]
pub struct EmptyCustomers;

#[async_trait]
impl CustomerFactsPort for EmptyCustomers {
    async fn find_by_id(&self, _customer_id: &CustomerAccountId) -> Result<Option<CustomerAccountFact>> {
        Ok(None)
    }

    async fn find_by_ids(
        &self,
        _customer_ids: &[CustomerAccountId],
        _executor: &mut dyn Executor,
    ) -> Result<Vec<CustomerAccountFact>> {
        Ok(Vec::new())
    }
}

/// Fail-closed customer facts used when composition has not injected an adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedCustomerFactsPort;

#[async_trait]
impl CustomerFactsPort for FailClosedCustomerFactsPort {
    async fn find_by_id(&self, _customer_id: &CustomerAccountId) -> Result<Option<CustomerAccountFact>> {
        Err(Error::Internal("客户端口未接线".to_string()))
    }

    async fn find_by_ids(
        &self,
        _customer_ids: &[CustomerAccountId],
        _executor: &mut dyn Executor,
    ) -> Result<Vec<CustomerAccountFact>> {
        Err(Error::Internal("客户端口未接线".to_string()))
    }
}
