//! 供应商列表展示与交接资格所需的账号事实 Port。

use std::collections::HashMap;

use async_trait::async_trait;

use crate::error::{Error, Result};

/// 供应商域读取账号显示名与登录资格。
#[async_trait]
pub trait AccountFactPort: Send + Sync {
    /// 生成业务查询用途的人员候选，不用于任务改派。
    ///
    /// # 参数
    /// * `ids` - 调用方按业务可见对象限定的人员 ID 集合
    ///
    /// # 返回
    /// 返回稳定 ID 与可区分同名、停用状态的显示标签。
    ///
    /// # 错误
    /// 未接线或查询失败必须报错，不得退化为全账号候选。
    async fn filter_options(&self, _ids: &[String]) -> Result<Vec<application_core::FilterOption>> {
        Err(Error::Internal("人员候选端口未接线".into()))
    }

    /// 账号不存在或不能登录时拒绝。
    ///
    /// # 参数
    /// * `user_id` - 账号 ID
    ///
    /// # 返回
    /// 账号有效时可登录时成功。
    ///
    /// # 错误
    /// 缺失映射为 `NotFound`；停用映射为业务错误。
    async fn ensure_can_login(&self, user_id: &str) -> Result<()>;

    /// 按账号 ID 批量读取显示名；缺失账号省略。
    ///
    /// # 参数
    /// * `account_ids` - 账号 ID
    ///
    /// # 返回
    /// 返回存在账号的显示名。
    ///
    /// # 错误
    /// 未接线或查询失败时拒绝。
    async fn names_by_ids(&self, account_ids: &[String]) -> Result<HashMap<String, String>>;
}

/// 未接线时失败关闭。
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
