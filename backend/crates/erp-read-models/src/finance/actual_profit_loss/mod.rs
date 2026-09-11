//! 非卡券实际经营盈亏：正式收入、成本归属和完整性的一致快照查询。
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

/// 服务端授权上下文；客户端不能提供或扩大客户集合。
pub struct ProfitLossAccess {
    pub customer_ids: Option<Vec<String>>,
    pub can_drill_cost: bool,
}
/// 混合读取仅通过各拥有领域仓储执行。
pub struct ActualProfitLossReadModel {
    db: Database,
}
impl ActualProfitLossReadModel {
    /// 复用应用数据库，不持有独立连接或缓存旧权限。
    pub fn new(db: Database) -> Self {
        Self { db }
    }
    /// 提供已实现的口径，由用户显式选择，不伪装已配置财务收入确认规则。
    pub fn period_basis() -> PeriodBasisConfig {
        PeriodBasisConfig { configuration_version: query::FORMULA_VERSION.into(), allowed_period_bases: vec![PeriodBasisOption { code: query::PERIOD_BASIS.into(), label: query::BASIS_LABEL.into(), explanation: "按上海自然日的销售单首次生效日期选单，采用当前正式收入与查询时点累计实际成本；后补费用会更新原订单盈亏。".into() }] }
    }
    /// 分页视图包含同一筛选下的全量指标、趋势和覆盖率。
    pub async fn view(&self, query: ProfitLossQuery, access: ProfitLossAccess) -> Result<ProfitLossView> {
        self.read(query, access, false).await
    }
    /// 重新查询当前授权下全部匹配行并同步生成 CSV；不接受客户端水印和金额。
    pub async fn export(&self, query: ProfitLossQuery, access: ProfitLossAccess) -> Result<ProfitLossExport> {
        Ok(export::export(self.read(query, access, true).await?))
    }
    /// 跨集合读取使用统一 snapshot 事务，不写入任何业务事实。
    async fn read(
        &self,
        query: ProfitLossQuery,
        access: ProfitLossAccess,
        export: bool,
    ) -> Result<ProfitLossView> {
        let bounds = query.validate()?;
        let db = self.db.clone();
        let as_of = chrono::Utc::now();
        let scope_label = if access.customer_ids.is_none() {
            "全部授权客户"
        } else {
            "当前有效归属客户"
        };
        let sources = db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    source::Sources::load(&db, &query, bounds, access.customer_ids, executor)
                        .await
                        .map(|sources| (sources, query))
                })
            })
            .await?;
        let orders = calculation::calculate(&sources.0, as_of.timestamp(), access.can_drill_cost)?;
        projection::project(orders, &sources.1, &as_of.to_rfc3339(), export, scope_label)
    }
}

#[cfg(test)]
mod tests;
