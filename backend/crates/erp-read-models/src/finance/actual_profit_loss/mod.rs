//! 非卡券实际经营盈亏：正式收入、成本归属和完整性的一致快照查询。
mod access;
mod attribution;
mod calculation;
pub mod dto;
mod export;
mod projection;
mod query;
mod source;

use crate::Result;
use dto::{PeriodBasisConfig, PeriodBasisOption, ProfitLossExport, ProfitLossQuery, ProfitLossView};
use mongodb::Database;
use persistence_core::Transactional;

/// 入口复核的成本详情动作资格；销售单数据范围仍由读取事务独立解析。
pub struct ProfitLossAccess {
    pub can_drill_cost: bool,
}
/// 混合读取仅通过各拥有领域仓储执行。
#[derive(Clone)]
pub struct ActualProfitLossReadModel {
    db: Database,
    rbac: erp_identity::SharedRbacService,
}
impl ActualProfitLossReadModel {
    /// 复用应用数据库与 RBAC，不持有独立连接或缓存旧权限。
    ///
    /// # 参数
    /// `db`、`rbac` 必须来自同一应用组合根。
    ///
    /// # 返回
    /// 返回执行一致快照查询的读模型。
    pub fn new(db: Database, rbac: erp_identity::SharedRbacService) -> Self {
        Self { db, rbac }
    }
    /// 提供已实现的口径，由用户显式选择，不伪装已配置财务收入确认规则。
    pub fn period_basis() -> PeriodBasisConfig {
        PeriodBasisConfig { configuration_version: query::FORMULA_VERSION.into(), allowed_period_bases: vec![PeriodBasisOption { code: query::PERIOD_BASIS.into(), label: query::BASIS_LABEL.into(), explanation: "按上海自然日的销售单首次生效日期选单，采用当前正式收入与查询时点累计实际成本；后补费用会更新原订单盈亏。".into() }] }
    }
    /// 分页视图包含同一筛选下的全量指标、趋势和覆盖率。
    ///
    /// # 参数
    /// `query` 为业务筛选；`actor` 为认证用户；`access` 不能替代事务内的对象授权。
    ///
    /// # 错误
    /// 参数非法、缺少同角色完整权限、范围版本变化或来源读取失败时返回错误。
    pub async fn view(
        &self,
        query: ProfitLossQuery,
        access: ProfitLossAccess,
        actor: &application_core::AuditActor,
    ) -> Result<ProfitLossView> {
        self.read(query, access, actor, false).await
    }
    /// 重新查询当前授权下全部匹配行并同步生成 CSV；不接受客户端水印和金额。
    ///
    /// # 返回
    /// 返回当前筛选的全量 CSV，分页参数不截断行集合。
    ///
    /// # 错误
    /// 与 [`Self::view`] 相同；返回前授权重验失败时不得交付文件。
    pub async fn export(
        &self,
        query: ProfitLossQuery,
        access: ProfitLossAccess,
        actor: &application_core::AuditActor,
    ) -> Result<ProfitLossExport> {
        Ok(export::export(self.read(query, access, actor, true).await?))
    }
    /// 在同一授权快照产生全部投影，返回前再检查撤权。
    async fn read(
        &self,
        query: ProfitLossQuery,
        access: ProfitLossAccess,
        actor: &application_core::AuditActor,
        export: bool,
    ) -> Result<ProfitLossView> {
        let (sources, query, context, no_scope) = self.snapshot(query, actor).await?;
        let as_of = context.as_of.as_utc();
        let orders = calculation::calculate(&sources, as_of.timestamp(), access.can_drill_cost)?;
        let mut view = projection::project(
            orders,
            &query,
            &as_of.to_rfc3339(),
            export,
            "销售单当前授权范围；贡献按首次生效归属",
        )?;
        self.recheck(&query, actor, &context.scope_version).await?;
        view.scope.permission_version = context.scope_version.clone();
        view.scope_version = context.scope_version;
        view.policy_version = context.policy_version;
        view.organization_version = context.organizations.version;
        if no_scope {
            view.empty_reason = Some("no_scope".into());
        }
        Ok(view)
    }

    /// 在新的读取事务内重验账号、组织、协作及业务责任集合，防止返回旧宽范围文件。
    async fn recheck(
        &self,
        query: &ProfitLossQuery,
        actor: &application_core::AuditActor,
        expected: &str,
    ) -> Result<()> {
        let this = self.clone();
        let query = query.clone();
        let actor = actor.clone();
        let current = self
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let (context, scope) = this.authorized_scope(&actor, executor).await?;
                    let orders =
                        source::authorized_orders(&this.db, &query, query.validate()?, scope, executor)
                            .await?;
                    Ok::<_, crate::Error>(source::version(&context.scope_version, &orders))
                })
            })
            .await?;
        ensure_version(Some(expected), &current)
    }

    /// 授权和销售、成本事实由同一只读事务装载，期间超限整体拒绝。
    async fn snapshot(
        &self,
        query: ProfitLossQuery,
        actor: &application_core::AuditActor,
    ) -> Result<(
        source::Sources,
        ProfitLossQuery,
        erp_identity::service::access_control::resolve::AuthorizedDataScope,
        bool,
    )> {
        let bounds = query.validate()?;
        let this = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let (mut context, scope) = this.authorized_scope(&actor, executor).await?;
                    let no_scope = scope.is_empty();
                    let sources = source::Sources::load(&this.db, &query, bounds, scope, executor).await?;
                    context.scope_version = source::version(&context.scope_version, &sources.orders);
                    ensure_version(query.scope_version.as_deref(), &context.scope_version)?;
                    Ok((sources, query, context, no_scope))
                })
            })
            .await
    }
}

/// 跨页与生成返回前必须使用相同授权版本；不拼接撤权前后的结果。
fn ensure_version(expected: Option<&str>, current: &str) -> Result<()> {
    if expected.is_some_and(|version| version != current) {
        return Err(crate::Error::ConflictError(
            "DATA_SCOPE_CHANGED：数据范围已变化，请从第一页刷新".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
