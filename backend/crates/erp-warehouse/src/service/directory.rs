//! Warehouse 独立目录：资格、授权、搜索与回显采用同一事务路径。
use std::sync::Arc;

use application_core::directory::{DirectoryPage, DirectoryQuery, ScopedPage};
use application_core::{AuditActor, PageView};
use mongodb::Database;
use persistence_core::Transactional;
use validator::Validate;

use crate::ports::WarehouseDirectoryAccess;
use crate::repository::WarehouseExt;
use crate::repository::directory::snapshot;
use crate::repository::prelude::*;
use crate::{Error, Result, WarehouseListParams, WarehouseView};

/// 本域对象目录用例。
pub struct WarehouseDirectoryService {
    db: Database,
    access: Arc<dyn WarehouseDirectoryAccess>,
}
impl WarehouseDirectoryService {
    /// 装配数据库与目录授权。
    /// # 参数
    /// `db` 为本域数据库，`access` 为独立目录授权。
    /// # 返回
    /// 不执行I/O的服务。
    /// # 错误
    /// 无。
    pub fn new(db: Database, access: Arc<dyn WarehouseDirectoryAccess>) -> Self {
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

impl WarehouseDirectoryService {
    /// 按同一 warehouse:list 范围读取原仓库列表，防止绕过目录授权。
    /// # 参数
    /// `actor` 为操作人，`params` 保留原业务条件及范围版本。
    /// # 返回
    /// 原仓库行和独立范围元信息。
    /// # 错误
    /// 无权限、非法请求、旧范围版本或仓储失败时拒绝。
    pub async fn warehouses(
        &self,
        actor: AuditActor,
        params: WarehouseListParams,
    ) -> Result<ScopedPage<WarehouseView>> {
        params.validate()?;
        if params.page.unwrap_or(1) > 1 && params.scope_version.as_deref().is_none_or(|v| v.trim().is_empty())
        {
            return Err(Error::ConflictError("DATA_SCOPE_CHANGED：后续页必须提供范围版本".into()));
        }
        let db = self.db.clone();
        let access = self.access.clone();
        db.client()
            .clone()
            .with_transaction(move |executor| {
                let db = db.clone();
                let access = access.clone();
                let actor = actor.clone();
                let params = params.clone();
                Box::pin(async move {
                    let scope = access.resolve(&actor, executor).await?;
                    if params.scope_version.as_ref().is_some_and(|v| v != &scope.scope_version) {
                        return Err(Error::ConflictError("DATA_SCOPE_CHANGED：仓库范围已变化".into()));
                    }
                    let mut filter = params.normalized()?.into_filter();
                    filter.authorized_ids = scope.ids.clone();
                    let result = db.warehouses().search_warehouses(&filter, executor).await?;
                    let page = PageView {
                        items: result.items.into_iter().map(WarehouseView::from_row).collect(),
                        total: result.total,
                        page: filter.page,
                        page_size: filter.page_size,
                    };
                    Ok(ScopedPage::new(page, scope))
                })
            })
            .await
    }
}
