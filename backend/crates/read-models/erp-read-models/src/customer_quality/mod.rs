//! S3-05 M10 客户经营质量双口径：当前负责与历史贡献各自独立的查询与导出。
//!
//! 两个口径永不合并排名或合计：当前口径按客户现任主责归属分组，历史口径按
//! 销售单首次生效冻结归属分组。历史分组只读快照身份与祖先路径，不回填现任。

use std::sync::Arc;

use erp_identity::AuthorizationService;
mod access;
mod current;
mod directory;
mod dto;
mod export;
mod history;
mod query;
mod source;

pub use access::QualityAccess;
use application_core::{AuditActor, EmptyScopeVersion, ScopeVersion};
pub use dto::{
    CurrentQualityQuery, CurrentQualityView, HistoryQualityQuery, HistoryQualityView, QualityExport,
};
use erp_customer::CustomerAccess;
use mongodb::Database;
use persistence_core::Transactional;

use crate::Result;

/// 客户经营质量只读用例；授权与业务事实在同一事务快照内读取。
#[derive(Clone)]
pub struct CustomerQualityReadModel {
    db: Database,
    rbac: Arc<AuthorizationService>,
    customer_scope: CustomerAccess,
}

impl CustomerQualityReadModel {
    /// 装配客户经营质量时，复用应用数据库、RBAC 与组合层注入的客户授权服务。
    ///
    /// # 参数
    /// * `rbac` - 当前 RBAC 快照服务
    /// * `customer_scope` - 已装配的客户授权服务
    ///
    /// # 返回
    /// 返回执行双口径一致快照查询的读模型。
    pub fn new(db: Database, rbac: Arc<AuthorizationService>, customer_scope: CustomerAccess) -> Self {
        Self { db, rbac, customer_scope }
    }

    /// 查看现任负责口径时，按当前归属返回可读客户的分页和期间订单汇总。
    ///
    /// # 参数
    /// * `query` - 现任负责人／组织／业务筛选
    ///
    /// # 返回
    /// 返回现任口径分页视图。
    ///
    /// # 错误
    /// 参数非法、无动作权限、范围版本变化或来源读取失败时返回错误。
    pub async fn view_current(
        &self,
        query: CurrentQualityQuery,
        actor: &AuditActor,
    ) -> Result<CurrentQualityView> {
        self.read_current(query, actor, false).await
    }

    /// 查看历史贡献口径时，按冻结人员或组织归属分组返回期间订单汇总，不使用客户现任负责人回填。
    ///
    /// # 参数
    /// * `query` - 历史人员／组织／业务筛选
    ///
    /// # 返回
    /// 返回历史口径分页视图。
    ///
    /// # 错误
    /// 参数非法、无动作权限、范围版本变化或来源读取失败时返回错误。
    pub async fn view_history(
        &self,
        query: HistoryQualityQuery,
        actor: &AuditActor,
    ) -> Result<HistoryQualityView> {
        self.read_history(query, actor, false).await
    }

    /// 导出现任负责报表时沿用原查询和范围版本，对完整匹配集合生成 CSV，而不是导出当前页。
    ///
    /// # 参数
    /// * `query` - 与列表相同的现任筛选
    ///
    /// # 返回
    /// 返回当前口径全量 CSV。
    ///
    /// # 错误
    /// 与 [`Self::view_current`] 相同；返回前授权重验失败时不得交付文件。
    pub async fn export_current(
        &self,
        query: CurrentQualityQuery,
        actor: &AuditActor,
    ) -> Result<QualityExport> {
        Ok(export::current(self.read_current(query, actor, true).await?))
    }

    /// 导出历史贡献报表时用同一筛选和授权范围读取全部匹配行，将冻结归属写入 CSV，不采用客户端提供的金额。
    ///
    /// # 参数
    /// * `query` - 与列表相同的历史筛选
    ///
    /// # 返回
    /// 返回历史口径全量 CSV。
    ///
    /// # 错误
    /// 与 [`Self::view_history`] 相同；返回前授权重验失败时不得交付文件。
    pub async fn export_history(
        &self,
        query: HistoryQualityQuery,
        actor: &AuditActor,
    ) -> Result<QualityExport> {
        Ok(export::history(self.read_history(query, actor, true).await?))
    }

    // 现任负责报表在首个快照内组合完整授权、业务筛选和投影；交付列表或导出前在新事务复验范围指纹。
    async fn read_current(
        &self,
        query: CurrentQualityQuery,
        actor: &AuditActor,
        export: bool,
    ) -> Result<CurrentQualityView> {
        let bounds = query.validate()?;

        let this = self.clone();
        let tx_actor = actor.clone();
        let (mut view, expected) = self
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                let query = query.clone();
                let actor = tx_actor.clone();
                Box::pin(async move { this.snapshot_current(&query, &actor, bounds, export, executor).await })
            })
            .await?;

        self.recheck_current(&view.query, actor, &expected).await?;
        view.view.scope_version.clone_from(&expected);
        view.view.scope.permission_version.clone_from(&expected);
        Ok(view.view)
    }

    // 历史贡献报表在首个快照内按冻结归属投影；交付列表或导出前在新事务复验范围指纹。
    async fn read_history(
        &self,
        query: HistoryQualityQuery,
        actor: &AuditActor,
        export: bool,
    ) -> Result<HistoryQualityView> {
        let bounds = query.validate()?;

        let this = self.clone();
        let tx_actor = actor.clone();
        let (mut view, expected) = self
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                let query = query.clone();
                let actor = tx_actor.clone();
                Box::pin(async move { this.snapshot_history(&query, &actor, bounds, export, executor).await })
            })
            .await?;

        self.recheck_history(&view.query, actor, &expected).await?;
        view.view.scope_version.clone_from(&expected);
        view.view.scope.permission_version.clone_from(&expected);
        Ok(view.view)
    }
}

// 客户经营质量列表翻页或导出前，用本次快照的指纹检查已携带的范围版本，避免拼接授权变化前后的结果。
fn ensure_version(expected: Option<&str>, current: &str) -> Result<()> {
    ScopeVersion::new(expected, EmptyScopeVersion::Provided)
        .verify(current)
        .map_err(|error| crate::support::data_scope_changed(error.detail()))
}

mod repository;
pub use repository::authorization::QualitySalesMapping;
