//! Consumer port for account login facts required by customer commands.

use std::collections::HashMap;

use async_trait::async_trait;

use crate::error::{Error, Result};

/// Port customer uses to validate sales accounts and resolve display names.
#[async_trait]
pub trait AccountFactPort: Send + Sync {
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

    /// Reject when the account does not exist or cannot log in.
    ///
    /// # Parameters
    /// * `user_id` - account id
    ///
    /// # Errors
    /// Missing account maps to `NotFound`; disabled account maps to `BusinessLogicError`.
    async fn ensure_can_login(&self, user_id: &str) -> Result<()>;

    /// Return display names keyed by account id. Missing accounts are omitted.
    async fn names_by_ids(&self, account_ids: &[String]) -> Result<HashMap<String, String>>;
}

/// Fail-closed account fact port used when composition has not injected an adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedAccountFactPort;

#[async_trait]
impl AccountFactPort for FailClosedAccountFactPort {
    async fn ensure_can_login(&self, _user_id: &str) -> Result<()> {
        Err(Error::Internal("账号端口未接线".to_string()))
    }

    async fn names_by_ids(&self, _account_ids: &[String]) -> Result<HashMap<String, String>> {
        Err(Error::Internal("账号端口未接线".to_string()))
    }
}
