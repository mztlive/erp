//! SalesReturnCase 详情与分页视图装配。
use std::sync::Arc;

use application_core::{AuditActor, PageView, SortDir};
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::returns;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::ReturnsService;
use crate::repository::ReturnsExt;
use crate::repository::prelude::*;

/// 保持现有功能授权政策的销售退货单域读取入口。
#[derive(Clone)]
pub struct SalesReturnQueryService {
    db: Database,
    authorization: Arc<AuthorizationService>,
}
impl SalesReturnQueryService {
    /// 绑定退货本域数据库与授权服务。
    /// # 参数
    /// db 为本域数据库，authorization 为组合根共享授权服务。
    /// # 返回
    /// 不缓存授权结果的单域查询服务。
    /// # 错误
    /// 无。
    pub fn new(db: Database, authorization: Arc<AuthorizationService>) -> Self {
        Self { db, authorization }
    }
}

use crate::dto::{SalesReturnCaseListParams, SalesReturnCaseView, SalesReturnLineView};
use crate::{Error, Result};
/// 销售退货处理单列表筛选条件类型（经 `ReturnsExt` 关联类型跨 crate 可达）。
type SalesReturnCaseFilter = <Database as ReturnsExt>::SalesReturnCaseFilter;

impl SalesReturnQueryService {
    // -----------------------------------------------------------------------

    /// 分页查询销售退货/拒收处理单列表。
    ///
    /// # 参数
    /// * `actor` - 本次已认证账号，按固定列表入口检查功能资格。
    /// * `params` - 查询参数（`return_no`/`sales_order_id`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// 参数校验或排序白名单失败时返回 `ValidationError`。处理单或明细读取失败时返回对应错误。
    pub async fn sales_return_case_list(
        &self,
        actor: &AuditActor,
        params: &SalesReturnCaseListParams,
    ) -> Result<PageView<SalesReturnCaseView>> {
        let this = self.clone();
        let actor = actor.clone();
        let params = params.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { this.sales_return_case_list_snapshot(&actor, &params, executor).await })
            })
            .await
    }

    /// 功能资格、分页计数及每条明细沿用调用方同一快照。
    async fn sales_return_case_list_snapshot(
        &self,
        actor: &AuditActor,
        params: &SalesReturnCaseListParams,
        executor: &mut dyn Executor,
    ) -> Result<PageView<SalesReturnCaseView>> {
        self.authorization
            .require(actor, &returns::SALES_RETURN_SALES_RETURN_CASE_LIST, AccessTarget::Operation, executor)
            .await?;

        params.validate()?;
        let query = params.normalized()?;
        let filter = SalesReturnCaseFilter {
            return_no: query.return_no,
            sales_order_id: query.sales_order_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self.db.sales_return_cases().search_sales_return_cases(&filter, executor).await?;

        let domain = ReturnsService::new(self.db.clone());
        let mut views = Vec::with_capacity(page.items.len());
        for row in page.items {
            views.push(domain.load_sales_return_case_view(&row.id, executor).await?);
        }
        Ok(PageView { items: views, total: page.total, page: filter.page, page_size: filter.page_size })
    }

    /// 查询销售退货/拒收处理单详情（处理单 + 明细行）。
    ///
    /// # 参数
    /// * `actor` - 本次已认证账号，按固定详情入口检查功能资格。
    /// * `id` - 处理单 ID
    ///
    /// # 返回
    /// 返回完整处理单视图。
    ///
    /// # 错误
    /// * `NotFound` - 处理单不存在
    ///
    /// 仓储读取失败时返回对应错误。
    pub async fn sales_return_case_detail(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<SalesReturnCaseView> {
        let this = self.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { this.sales_return_case_detail_snapshot(&id, &actor, executor).await })
            })
            .await
    }

    /// 功能资格、处理单和明细沿用调用方同一快照。
    async fn sales_return_case_detail_snapshot(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<SalesReturnCaseView> {
        self.authorization
            .require(
                actor,
                &returns::SALES_RETURN_SALES_RETURN_CASE_DETAIL,
                AccessTarget::Operation,
                executor,
            )
            .await?;
        ReturnsService::new(self.db.clone()).load_sales_return_case_view(id, executor).await
    }

    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // 私有视图装配
    // -----------------------------------------------------------------------
}

impl ReturnsService {
    /// 调用方必须先完成所属命令授权，并传入原执行器。
    /// 装配销售退货/拒收处理单视图。
    ///
    /// # 参数
    /// * `executor` - 已完成所属命令授权的原读取或事务执行器。
    /// * `id` - 处理单 ID
    ///
    /// # 返回
    /// 返回完整处理单视图。
    ///
    /// # 错误
    /// * `NotFound` - 处理单不存在
    ///
    /// 仓储读取失败时返回对应错误。
    pub async fn load_sales_return_case_view(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SalesReturnCaseView> {
        let case = self
            .db
            .sales_return_cases()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("销售退货处理单不存在".to_string()))?;

        let lines = self
            .db
            .sales_return_lines()
            .find_lines_by_cases(&[case.base.id.clone().into()], executor)
            .await?
            .into_iter()
            .map(|line| SalesReturnLineView {
                id: line.base.id,
                sales_order_line_id: line.sales_order_line_id.to_string(),
                requested_quantity: line.requested_quantity,
                received_quantity: line.received_quantity,
                quality_result: line.quality_result.map(|result| result.as_str().to_string()),
                restockable_quantity: line.restockable_quantity,
            })
            .collect();

        Ok(SalesReturnCaseView {
            id: case.base.id,
            return_no: case.return_no,
            sales_order_id: case.sales_order_id.to_string(),
            acceptance_id: case.acceptance_id.map(|id| id.to_string()),
            case_type: case.case_type,
            reason: case.reason,
            discovered_at: case.discovered_at,
            return_route: case.return_route,
            status: case.stable.status(),
            version: case.base.version,
            created_at: case.base.created_at,
            lines,
        })
    }
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use mongodb::Client;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    /// 授权设施故障必须中止原快照中的列表和详情，不继续读取业务集合。
    #[tokio::test]
    async fn authorization_failure_stops_list_and_detail_before_business_reads() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("sales_return_snapshot_test");
        let service = SalesReturnQueryService::new(db, unavailable_authorization());
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);

        let list_error = service
            .sales_return_case_list_snapshot(
                &actor,
                &SalesReturnCaseListParams::default(),
                &mut SnapshotExecutor::default(),
            )
            .await
            .unwrap_err();
        let detail_error = service
            .sales_return_case_detail_snapshot("return", &actor, &mut SnapshotExecutor::default())
            .await
            .unwrap_err();

        for error in [list_error, detail_error] {
            assert!(matches!(error, Error::Internal(reason) if reason == "测试授权设施不可用"));
        }
    }
}
