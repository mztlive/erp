//! 历史候选不依赖报表投影、分页和结果筛选。
use application_core::AuditActor;
use persistence_core::Transactional;

use super::source::{CurrentSources, quality_order_filter, version};
use super::{CustomerQualityReadModel, HistoryQualityQuery, QualityAccess};
use crate::Result;
use crate::historical_directory::{HistoricalDirectoryQuery, HistoricalDirectoryView};

impl CustomerQualityReadModel {
    /// 独立读取历史贡献候选，返回前重验授权与来源版本。
    /// # 参数
    /// `input` 只含期间和客户上下文；`actor` 为认证人。
    /// # 返回
    /// 完整冻结身份目录及独立范围版本。
    /// # 错误
    /// 非法期间、无动作、超限、撤权或版本变化时拒绝。
    pub async fn history_directory(
        &self,
        input: HistoricalDirectoryQuery,
        actor: &AuditActor,
    ) -> Result<HistoricalDirectoryView> {
        let query = HistoryQualityQuery {
            from: input.from,
            to: input.to,
            customer_id: input.customer_id,
            scope_version: input.scope_version.clone(),
            dimension: "attribution_user".into(),
            sort: "orderCount:desc".into(),
            page: 1,
            page_size: 20,
            ..Default::default()
        };
        let bounds = query.validate()?;
        let this = self.clone();
        let tx_actor = actor.clone();
        let tx_query = query.clone();
        let view = self
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let access =
                        QualityAccess::new(this.db.clone(), this.rbac.clone(), this.customer_scope.clone());
                    let (context, scope) = access.resolve_history(&tx_actor, executor).await?;
                    let no_scope = scope.is_empty();
                    let filter =
                        quality_order_filter(&bounds, tx_query.customer_id.map(|id| vec![id]), &scope);
                    let orders = CurrentSources::load_orders(&this.db, filter, executor).await?;
                    let expected = version(&context.scope_version, &orders);
                    super::ensure_version(input.scope_version.as_deref(), &expected)?;
                    HistoricalDirectoryView::from_attributions(
                        orders.iter().filter_map(|row| row.attribution.as_ref()),
                        expected,
                        no_scope,
                    )
                })
            })
            .await?;
        self.recheck_history(&query, actor, &view.scope_version).await?;
        Ok(view)
    }
}
