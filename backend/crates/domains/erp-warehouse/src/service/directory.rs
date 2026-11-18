//! Warehouse 独立目录：资格、授权、搜索与回显采用同一事务路径。
use application_core::directory::{DirectoryPage, DirectoryQuery, DirectoryScope, ScopedPage};
use application_core::{AuditActor, PageView};
use mongodb::Database;
use persistence_core::Transactional;
use validator::Validate;

use crate::authorization::{WarehouseAuthorization, WarehouseQuery};
use crate::repository::directory::{snapshot, warehouses};
use crate::{Error, Result, WarehouseListParams, WarehouseView};

/// 本域对象目录用例。
pub struct WarehouseDirectoryService {
    db: Database,
    access: WarehouseAuthorization,
}
impl WarehouseDirectoryService {
    /// 将仓库数据库与独立目录授权装配为查询服务，构造不执行 I/O。
    pub fn new(db: Database, access: WarehouseAuthorization) -> Self {
        Self { db, access }
    }

    /// 用当前认证账号读取目录或已选 ID 的回显，不接受业务列表条件；只交付仍可读的目录项和独立范围版本。无权限、请求非法、超过目录上限、版本过期或持久化失败时拒绝。
    pub async fn list(&self, actor: AuditActor, query: DirectoryQuery) -> Result<DirectoryPage> {
        let query = query.normalized()?;

        let db = self.db.clone();
        let access = self.access.clone();
        db.client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let read = access.query(&actor, executor).await?;
                    let rows = snapshot(&db, &read, &query, executor).await?;
                    let scope = directory_scope(&read, rows.iter().map(|row| row.id.clone()).collect());
                    let page = DirectoryPage::from_snapshot(rows, &query, scope)?;

                    if query.scope_version.as_ref().is_some_and(|v| v != &page.scope_version) {
                        return Err(Error::ConflictError(
                            "DATA_SCOPE_CHANGED：目录已变化，请重新查询".into(),
                        ));
                    }
                    read.validate(executor).await?;
                    Ok(page)
                })
            })
            .await
    }
}

impl WarehouseDirectoryService {
    /// 在同一 `warehouse:list` 授权范围内执行原仓库列表筛选，返回原分页形状与独立范围元信息。第二页起须传范围版本；无权、非法请求、版本过期或仓储失败均拒绝。
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
                Box::pin(async move {
                    let read = access.query(&actor, executor).await?;
                    if params.scope_version.as_ref().is_some_and(|v| v != &read.version()) {
                        return Err(Error::ConflictError("DATA_SCOPE_CHANGED：仓库范围已变化".into()));
                    }

                    let filter = params.normalized()?.into_filter();
                    let result = warehouses(&db, &read, &filter, executor).await?;
                    let scope =
                        directory_scope(&read, result.items.iter().map(|row| row.id.clone()).collect());
                    let page = PageView {
                        items: result.items.into_iter().map(WarehouseView::from_row).collect(),
                        total: result.total,
                        page: filter.page,
                        page_size: filter.page_size,
                    };

                    read.validate(executor).await?;
                    Ok(ScopedPage::new(page, scope))
                })
            })
            .await
    }
}

// 用本次授权结果 ID 和冻结版本生成响应元信息，供调用方识别范围变化；不将这些 ID 反向当作仓储授权。
fn directory_scope(read: &WarehouseQuery, ids: Vec<String>) -> DirectoryScope {
    let (policy_version, organization_version, as_of) = read.metadata();
    DirectoryScope {
        ids: Some(ids),
        scope_version: read.version(),
        policy_version,
        organization_version,
        as_of: as_of.as_utc().to_rfc3339(),
        no_scope: read.is_empty(),
    }
}
