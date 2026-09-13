//! Consumer port for account display names required by contract lists.

use std::collections::HashMap;

use async_trait::async_trait;

use crate::error::{Error, Result};

/// Port contract uses to resolve owner display names without depending on `erp-identity`.
#[async_trait]
pub trait AccountNamePort: Send + Sync {
    /// 生成业务查询用途的人员候选，不用于任务改派。
    ///
    /// # 参数
    /// * `ids` - 调用方按业务可见对象限定的人员 ID 集合
    ///
    /// # 返回值
    /// 返回稳定 ID 与可区分同名、停用状态的显示标签。
    ///
    /// # 错误
    /// 未接线或查询失败必须报错，不得退化为全账号候选。
    async fn filter_options(&self, _ids: &[String]) -> Result<Vec<application_core::FilterOption>> {
        Err(Error::Internal("人员候选端口未接线".into()))
    }

    /// Return display names keyed by account id. Missing accounts are omitted.
    ///
    /// # Parameters
    /// * `account_ids` - owner account ids collected from assignment facts
    ///
    /// # Errors
    /// Adapter query failures.
    async fn names_by_ids(&self, account_ids: &[String]) -> Result<HashMap<String, String>>;
}

/// Empty account-name lookup used by isolated unit tests.
#[derive(Debug, Default, Clone, Copy)]
pub struct EmptyAccountNames;

#[async_trait]
impl AccountNamePort for EmptyAccountNames {
    async fn names_by_ids(&self, _account_ids: &[String]) -> Result<HashMap<String, String>> {
        Ok(HashMap::new())
    }
}

/// Fail-closed account-name port used when composition has not injected an adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedAccountNamePort;

#[async_trait]
impl AccountNamePort for FailClosedAccountNamePort {
    async fn names_by_ids(&self, _account_ids: &[String]) -> Result<HashMap<String, String>> {
        Err(Error::Internal("账号端口未接线".to_string()))
    }
}
