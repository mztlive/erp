//! Consumer ports for customer existence, numbers and assignment visibility.

use std::collections::HashMap;

use async_trait::async_trait;
use erp_core::common::time::BusinessDate;
use erp_core::ids::{CustomerAccountId, PartyId};
use persistence_core::Executor;

use crate::error::{Error, Result};

/// 合同域所需的客户归属事实；主责与协作分开，不含身份域类型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractAssignmentFact {
    /// 客户稳定身份。
    pub customer_id: String,
    /// 归属人员。
    pub user_id: String,
    /// 是否为当前主负责人；false 表示协作。
    pub is_owner: bool,
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
    /// Load one undeleted customer role.
    ///
    /// # Parameters
    /// * `customer_id` - customer role id
    ///
    /// # Returns
    /// `None` when the customer does not exist or is deleted.
    ///
    /// # Errors
    /// Adapter query failures.
    async fn find_by_id(&self, customer_id: &CustomerAccountId) -> Result<Option<CustomerAccountFact>>;

    /// Load undeleted customer roles by id. Missing ids are omitted.
    ///
    /// # Parameters
    /// * `customer_ids` - customer role ids
    ///
    /// # Errors
    /// Adapter query failures.
    async fn find_by_ids(&self, customer_ids: &[CustomerAccountId]) -> Result<Vec<CustomerAccountFact>>;
}

/// Port contract uses to resolve current customer ownership without depending on `erp-customer`.
#[async_trait]
pub trait CustomerAssignmentFactsPort: Send + Sync {
    /// 读取账号在指定业务日的有效主责与协作归属。
    ///
    /// # 参数
    /// * `user_id` - 当前账号
    /// * `as_of` - 客户归属自然日
    /// * `executor` - 与授权相同的执行器
    ///
    /// # 返回
    /// 返回有效归属行；无归属时为空向量。
    ///
    /// # 错误
    /// 未装配或读取失败时拒绝。
    ///
    /// # 关键业务约束
    /// 主责与协作必须分开；不得把签约经办当作当前负责人。
    async fn active_assignments_for_user(
        &self,
        user_id: &str,
        as_of: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ContractAssignmentFact>>;

    /// 读取指定负责人集合的当前主责客户。
    ///
    /// # 参数
    /// * `customer_ids` - 已授权客户；`None` 表示不按客户集合收窄
    /// * `owner_ids` - 主负责人 ID；`None` 表示不按人员收窄
    /// * `as_of` - 客户归属自然日
    /// * `executor` - 与授权相同的执行器
    ///
    /// # 返回
    /// 返回这些负责人当前主责的客户 ID。
    ///
    /// # 错误
    /// 未装配或读取失败时拒绝。
    ///
    /// # 关键业务约束
    /// 空负责人或空客户集合保持空结果，不得查询全部客户。
    async fn current_owner_customer_ids(
        &self,
        customer_ids: Option<&[String]>,
        owner_ids: Option<&[String]>,
        as_of: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>>;

    /// 读取当前主负责人，键为客户 ID。
    ///
    /// # 参数
    /// * `customer_ids` - 可见客户
    /// * `as_of` - 客户归属自然日
    /// * `executor` - 与授权相同的执行器
    ///
    /// # 返回
    /// 无主责的客户省略。
    ///
    /// # 错误
    /// 未装配或读取失败时拒绝。
    ///
    /// # 关键业务约束
    /// 当前跟进负责人只取客户当前主负责人，不得用协作或签约人兜底。
    async fn owner_user_ids_by_customer(
        &self,
        customer_ids: &[String],
        as_of: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>>;
}

/// Empty customer lookup used by isolated unit tests.
#[derive(Debug, Default, Clone, Copy)]
pub struct EmptyCustomers;

#[async_trait]
impl CustomerFactsPort for EmptyCustomers {
    async fn find_by_id(&self, _customer_id: &CustomerAccountId) -> Result<Option<CustomerAccountFact>> {
        Ok(None)
    }

    async fn find_by_ids(&self, _customer_ids: &[CustomerAccountId]) -> Result<Vec<CustomerAccountFact>> {
        Ok(Vec::new())
    }
}

/// Empty assignment lookup used by isolated unit tests.
#[derive(Debug, Default, Clone, Copy)]
pub struct EmptyAssignments;

#[async_trait]
impl CustomerAssignmentFactsPort for EmptyAssignments {
    async fn active_assignments_for_user(
        &self,
        _user_id: &str,
        _as_of: BusinessDate,
        _executor: &mut dyn Executor,
    ) -> Result<Vec<ContractAssignmentFact>> {
        Ok(Vec::new())
    }

    async fn current_owner_customer_ids(
        &self,
        _customer_ids: Option<&[String]>,
        _owner_ids: Option<&[String]>,
        _as_of: BusinessDate,
        _executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        Ok(Vec::new())
    }

    async fn owner_user_ids_by_customer(
        &self,
        _customer_ids: &[String],
        _as_of: BusinessDate,
        _executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        Ok(HashMap::new())
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

    async fn find_by_ids(&self, _customer_ids: &[CustomerAccountId]) -> Result<Vec<CustomerAccountFact>> {
        Err(Error::Internal("客户端口未接线".to_string()))
    }
}

/// Fail-closed assignment facts used when composition has not injected an adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedAssignmentFactsPort;

#[async_trait]
impl CustomerAssignmentFactsPort for FailClosedAssignmentFactsPort {
    async fn active_assignments_for_user(
        &self,
        _user_id: &str,
        _as_of: BusinessDate,
        _executor: &mut dyn Executor,
    ) -> Result<Vec<ContractAssignmentFact>> {
        Err(Error::Internal("客户归属端口未接线".to_string()))
    }

    async fn current_owner_customer_ids(
        &self,
        _customer_ids: Option<&[String]>,
        _owner_ids: Option<&[String]>,
        _as_of: BusinessDate,
        _executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        Err(Error::Internal("客户归属端口未接线".to_string()))
    }

    async fn owner_user_ids_by_customer(
        &self,
        _customer_ids: &[String],
        _as_of: BusinessDate,
        _executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        Err(Error::Internal("客户归属端口未接线".to_string()))
    }
}
