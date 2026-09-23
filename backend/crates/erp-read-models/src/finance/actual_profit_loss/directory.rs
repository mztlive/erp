//! 实际盈亏历史目录只装载已授权订单，不计算成本与报表结果。
use application_core::AuditActor;
use persistence_core::Transactional;

use super::dto::ProfitLossQuery;
use super::{ActualProfitLossReadModel, source};
use crate::Result;
use crate::historical_directory::{HistoricalDirectoryView, ProfitLossDirectoryQuery};

impl ActualProfitLossReadModel {
    /// 独立读取冻结归属目录，沿用报表同角色销售与成本权限证明。
    /// # 参数
    /// `input` 只含期间口径和客户上下文；`actor` 为认证人。
    /// # 返回
    /// 完整冻结身份候选，不携带金额或当前人员信息。
    /// # 错误
    /// 非法期间、权限不足、超限、来源或授权版本变化时拒绝。
    pub async fn history_directory(
        &self,
        input: ProfitLossDirectoryQuery,
        actor: &AuditActor,
    ) -> Result<HistoricalDirectoryView> {
        let query = ProfitLossQuery {
            from: input.from,
            to: input.to,
            period_basis: input.period_basis,
            customer_id: input.customer_id,
            scope_version: input.scope_version.clone(),
            coverage: "all".into(),
            dimension: "sales_order".into(),
            sort: "actualProfitLossNet:asc".into(),
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
                    let (context, scope) = this.authorized_scope(&tx_actor, executor).await?;
                    let no_scope = scope.is_empty();
                    let orders =
                        source::authorized_orders(&this.db, &tx_query, bounds, scope, executor).await?;
                    let expected = source::version(&context.scope_version, &orders);
                    super::ensure_version(input.scope_version.as_deref(), &expected)?;
                    HistoricalDirectoryView::from_attributions(
                        orders.iter().filter_map(|row| row.attribution.as_ref()),
                        expected,
                        no_scope,
                    )
                })
            })
            .await?;
        self.recheck(&query, actor, &view.scope_version).await?;
        Ok(view)
    }
}
