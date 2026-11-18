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
    /// 装配使用退货库与共享授权服务的销售退货查询入口，不缓存请求级授权结果。
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

    /// 打开销售退货或拒收列表时，先在读取快照内检查固定列表权限，再按退货单号、销售单或状态筛选、计数并装配明细。
    /// 参数或排序字段非法返回 `ValidationError`；授权、表头或明细读取失败时不交付页面。
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

    // 现有读取事务内先检查固定列表权限，再用同一快照完成筛选、分页计数与逐单明细装配。
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

    /// 按处理单 ID 打开销售退货或拒收详情时，固定详情权限、表头和明细读取共用一次快照。
    /// 处理单不存在返回 `NotFound`，授权或读取失败时不交付视图。
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

    // 详情快照先检查固定功能权限，再沿同一执行器读取处理单及其明细。
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
    /// 给已完成所属命令授权的调用方装配销售退货或拒收的表头和明细；沿用原读取或写入执行器，本方法不另行授权。
    /// 处理单不存在返回 `NotFound`，仓储失败返回对应错误。
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

    // 验证授权设施故障必须中止原快照中的列表和详情，不继续读取业务集合。
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
