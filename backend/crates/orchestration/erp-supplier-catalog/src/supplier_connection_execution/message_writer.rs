//! 消息补查与目录写入之间的窄业务入口。

use application_core::AuditActor;
use erp_catalog::CatalogService;
use erp_supply::entity::supplier_api::SupplierApiConnection;
use erp_supply::entity::supplier_catalog::SupplierCatalogItem;
use mongodb::Database;
use persistence_core::Transactional;

use super::{catalog_refresh_actor, guard_catalog_refresh, save_catalog_observation};
use crate::Result;

/// 接收已经补查并核验身份的目录观察，不负责消息领取或应答。
pub struct CatalogMessageWriter {
    db: Database,
    catalog: CatalogService,
}

impl CatalogMessageWriter {
    /// 绑定原应用数据库及已装配的商品服务。
    /// # 参数
    /// 数据库与商品领域服务。
    /// # 返回
    /// 不执行读取或写入的目录写入器。
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database, catalog: CatalogService) -> Self {
        Self { db, catalog }
    }

    /// 在外部补查前沿用本连接原目录采集授权。
    /// # 参数
    /// 服务端连接身份。
    /// # 返回
    /// 原采集授权对应的当前有效操作人。
    /// # 错误
    /// 缺少授权、账号失效或读取失败时拒绝。
    pub async fn actor(&self, connection: &str) -> Result<AuditActor> {
        catalog_refresh_actor(&self.db, connection).await
    }

    /// 同事务重验补查连接与授权，并保存来源、供给与公司资料。
    /// # 参数
    /// 补查前连接快照、已核验的观察和原采集授权操作人。
    /// # 返回
    /// 原子应用已经完成。
    /// # 错误
    /// 配置、授权或数据版本变化时拒绝；保留原持久化错误分类。
    pub async fn save(
        &self,
        connection: &SupplierApiConnection,
        item: SupplierCatalogItem,
        actor: &AuditActor,
    ) -> Result<()> {
        let (db, catalog, connection, actor) =
            (self.db.clone(), self.catalog.clone(), connection.clone(), actor.clone());
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    guard_catalog_refresh(&db, &connection, &actor, executor).await?;
                    save_catalog_observation(&db, &catalog, item, &actor, executor).await
                })
            })
            .await
    }
}
