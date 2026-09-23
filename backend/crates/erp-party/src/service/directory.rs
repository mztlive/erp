//! SettlementParty 独立目录：资格、授权、搜索与回显采用同一事务路径。
use std::sync::Arc;

use application_core::AuditActor;
use application_core::directory::{DirectoryPage, DirectoryQuery};
use mongodb::Database;
use persistence_core::Transactional;

use crate::ports::SettlementPartyDirectoryAccess;
use crate::repository::directory::snapshot;
use crate::{Error, Result};

/// 本域对象目录用例。
pub struct SettlementPartyDirectoryService {
    db: Database,
    access: Arc<dyn SettlementPartyDirectoryAccess>,
}
impl SettlementPartyDirectoryService {
    /// 装配数据库与目录授权。
    /// # 参数
    /// `db` 为本域数据库，`access` 为独立目录授权。
    /// # 返回
    /// 不执行I/O的服务。
    /// # 错误
    /// 无。
    pub fn new(db: Database, access: Arc<dyn SettlementPartyDirectoryAccess>) -> Self {
        Self { db, access }
    }

    /// 查询独立目录或已选身份。
    /// # 参数
    /// `actor` 为已认证操作人，`query` 不得包含业务列表条件。
    /// # 返回
    /// 当前仍可读的目录项和独立版本。
    /// # 错误
    /// 缺动作、非法请求、目录超限、旧版本或持久化失败时拒绝。
    pub async fn list(&self, actor: AuditActor, query: DirectoryQuery) -> Result<DirectoryPage> {
        let query = query.normalized()?;
        let db = self.db.clone();
        let access = self.access.clone();
        db.client()
            .clone()
            .with_transaction(move |executor| {
                let db = db.clone();
                let access = access.clone();
                let actor = actor.clone();
                let query = query.clone();
                Box::pin(async move {
                    let scope = access.resolve(&actor, executor).await?;
                    let rows = snapshot(&db, &scope, &query, executor).await?;
                    let page = DirectoryPage::from_snapshot(rows, &query, scope)?;
                    if query.scope_version.as_ref().is_some_and(|v| v != &page.scope_version) {
                        return Err(Error::ConflictError(
                            "DATA_SCOPE_CHANGED：目录已变化，请重新查询".into(),
                        ));
                    }
                    Ok(page)
                })
            })
            .await
    }
}
