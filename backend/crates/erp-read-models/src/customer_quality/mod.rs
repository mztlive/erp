//! S3-05 M10 客户经营质量双口径：当前负责与历史贡献各自独立的查询与导出。
//!
//! 两个口径永不合并排名或合计：当前口径按客户现任主责归属分组，历史口径按
//! 销售单首次生效冻结归属分组。历史分组只读快照身份与祖先路径，不回填现任。

mod access;
mod current;
mod dto;
mod export;
mod history;
mod query;
mod source;

use std::sync::Arc;

pub use access::QualityAccess;
use application_core::AuditActor;
pub use dto::{
    CurrentQualityQuery, CurrentQualityView, HistoryQualityQuery, HistoryQualityView, QualityExport,
};
use erp_customer::CustomerDataScopePort;
use mongodb::Database;
use persistence_core::Transactional;

use crate::Result;

/// 客户经营质量只读用例；授权与业务事实在同一事务快照内读取。
#[derive(Clone)]
pub struct CustomerQualityReadModel {
    db: Database,
    rbac: erp_identity::SharedRbacService,
    customer_scope: Arc<dyn CustomerDataScopePort>,
}

impl CustomerQualityReadModel {
    /// 复用应用数据库、RBAC 与组合层注入的客户范围 Port。
    ///
    /// # 参数
    /// * `db` - 应用数据库
    /// * `rbac` - 当前 RBAC 快照服务
    /// * `customer_scope` - 客户域 Port，生产环境由组合层 adapter 实现
    ///
    /// # 返回
    /// 返回执行双口径一致快照查询的读模型。
    pub fn new(
        db: Database,
        rbac: erp_identity::SharedRbacService,
        customer_scope: Arc<dyn CustomerDataScopePort>,
    ) -> Self {
        Self { db, rbac, customer_scope }
    }

    /// 当前负责口径分页视图：现任主责客户及其期间订单汇总。
    ///
    /// # 参数
    /// * `query` - 现任负责人／组织／业务筛选
    /// * `actor` - 已认证操作人
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

    /// 历史贡献口径分页视图：冻结归属分组的期间订单汇总。
    ///
    /// # 参数
    /// * `query` - 历史人员／组织／业务筛选
    /// * `actor` - 已认证操作人
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

    /// 当前口径全量 CSV；分页参数不截断行集合，版本绑定首个响应。
    ///
    /// # 参数
    /// * `query` - 与列表相同的现任筛选
    /// * `actor` - 已认证操作人
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

    /// 历史口径全量 CSV；冻结归属列随行导出，不使用客户端金额。
    ///
    /// # 参数
    /// * `query` - 与列表相同的历史筛选
    /// * `actor` - 已认证操作人
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

    /// 在同一授权快照产生当前口径全部投影，返回前再检查撤权。
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

    /// 在同一授权快照产生历史口径全部投影，返回前再检查撤权。
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

/// 跨页与生成返回前必须使用相同授权版本；不拼接撤权前后的结果。
fn ensure_version(expected: Option<&str>, current: &str) -> Result<()> {
    if expected.is_some_and(|version| version != current) {
        return Err(crate::Error::ConflictError("DATA_SCOPE_CHANGED：数据范围已变化，请从第一页刷新".into()));
    }
    Ok(())
}
