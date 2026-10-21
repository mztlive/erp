//! 合同列表所需账号显示名的消费方端口。

use std::collections::HashMap;

use async_trait::async_trait;
use persistence_core::Executor;

use crate::error::{Error, Result};

/// Port contract uses to resolve owner display names without depending on `erp-identity`.
#[async_trait]
pub trait AccountNamePort: Send + Sync {
    /// 在原写入事务中证明共享目标账号存在且启用。
    ///
    /// # 参数
    /// * `account_ids` - 共享人员账号
    /// * `executor` - 调用方事务执行器
    /// # 返回
    /// 所有账号均存在且可登录时成功。
    /// # 错误
    /// 未接线或任一账号失效时拒绝。
    async fn ensure_enabled(&self, _account_ids: &[String], _executor: &mut dyn Executor) -> Result<()> {
        Err(Error::Internal("共享账号校验端口未接线".into()))
    }

    /// 按账号 ID 返回显示名。缺失的账号省略。
    ///
    /// # 参数
    /// * `account_ids` - 从归属事实收集的负责人账号 ID。
    ///
    /// # 返回
    /// 键为账号 ID、值为显示名的映射。未找到的账号不出现。
    ///
    /// # 错误
    /// 适配器查询失败时返回对应错误。
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
