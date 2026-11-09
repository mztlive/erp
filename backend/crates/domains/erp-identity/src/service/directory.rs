//! 已授权业务对象引用的账号显示资料。
use std::collections::HashMap;

use mongodb::Database;
use persistence_core::Executor;

use crate::repository::prelude::*;
use crate::{AccessControlExt, Result};
/// 仅按已知账号身份批量解析显示名，不执行姓名检索。
pub struct AccountDirectoryService {
    db: Database,
}
impl AccountDirectoryService {
    /// 绑定账号仓储。
    /// # 参数
    /// `db` 为应用数据库。
    /// # 返回
    /// 返回无缓存的目录服务。
    /// # 错误
    /// 无。
    pub fn new(db: Database) -> Self {
        Self { db }
    }
    /// 批量读取调用方已授权对象引用的账号姓名。
    /// # 参数
    /// `ids` 为对象引用的账号 ID，`executor` 沿用调用方快照。
    /// # 返回
    /// 仅返回未删除账号的 ID 与姓名；不存在的账号不补造。
    /// # 错误
    /// 账号仓储读取失败时返回错误。
    pub async fn names(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        Ok(self.db.accounts().names_by_ids(ids, executor).await?)
    }
}
