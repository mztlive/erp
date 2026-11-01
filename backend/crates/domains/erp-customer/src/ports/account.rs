//! 客户命令所需的账号登录事实消费端口。

use std::collections::HashMap;

use async_trait::async_trait;

use crate::error::{Error, Result};

/// Port customer uses to validate sales accounts and resolve display names.
#[async_trait]
pub trait AccountFactPort: Send + Sync {
    /// 账号不存在或不能登录时拒绝。
    ///
    /// # 参数
    /// * `user_id` - 账号 ID。
    ///
    /// # 返回
    /// 账号存在且可以登录时无返回值。
    ///
    /// # 错误
    /// 账号不存在时返回 `NotFound`；账号不能登录时返回 `BusinessLogicError`。适配器读取失败时返回对应错误。
    async fn ensure_can_login(&self, user_id: &str) -> Result<()>;

    /// 按账号 ID 返回展示名。缺失账号省略。
    ///
    /// # 参数
    /// * `account_ids` - 待解析的账号 ID。
    ///
    /// # 返回
    /// 返回以账号 ID 为键的展示名；不存在的账号不出现在映射中。
    ///
    /// # 错误
    /// 账号事实读取失败时返回对应错误。
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
