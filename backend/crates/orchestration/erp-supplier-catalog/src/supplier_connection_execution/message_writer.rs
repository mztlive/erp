//! 消息补查与目录写入之间的窄业务入口。

use std::sync::Arc;

use application_core::AuditActor;
use erp_catalog::CatalogService;
use erp_identity::AuthorizationService;
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
    authorization: Arc<AuthorizationService>,
}

impl CatalogMessageWriter {
    /// 绑定原应用数据库及已装配的商品服务。
    ///
    /// 数据库、商品领域服务和应用共享授权服务。
    ///
    /// 不执行读取或写入的目录写入器。
    pub fn new(db: Database, catalog: CatalogService, authorization: Arc<AuthorizationService>) -> Self {
        Self { db, catalog, authorization }
    }

    /// 消息消费者补查目录前，按服务端连接找回原采集授权的有效操作人；不能把消息本身作为授权依据。
    ///
    /// 服务端连接身份。
    ///
    /// 原采集授权对应的当前有效操作人。
    ///
    /// 缺少授权、账号失效或读取失败时拒绝。
    pub async fn actor(&self, connection: &str) -> Result<AuditActor> {
        catalog_refresh_actor(&self.db, connection).await
    }

    /// 外部目录补查完成后，在同一结果事务用补查前的连接快照重验配置和授权，再提交来源观察、供给与公司资料。
    ///
    /// 补查前连接快照、已核验的观察和原采集授权操作人。
    ///
    /// 原子应用已经完成。
    ///
    /// 配置、授权或数据版本变化时拒绝；保留原持久化错误分类。
    pub async fn save(
        &self,
        connection: &SupplierApiConnection,
        item: SupplierCatalogItem,
        actor: &AuditActor,
    ) -> Result<()> {
        let authorization = self.authorization.clone();
        let (db, catalog, connection, actor) =
            (self.db.clone(), self.catalog.clone(), connection.clone(), actor.clone());
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    guard_catalog_refresh(&db, &connection, &actor, &authorization, executor).await?;
                    save_catalog_observation(&db, &catalog, item, &actor, executor).await
                })
            })
            .await
    }
}
