use super::dto::{
    CreateSalesReturnCaseRequest, PageView, SalesReturnCaseListParams, SalesReturnCaseView, SortDir,
};
use super::ReturnsService;
use crate::audit::AuditActor;
use crate::document_registry::{new_registered_document, persist_registered_document};
use crate::errors::{Error, Result};
use database::{AccessControlExt, NoTransaction, ReturnsExt, Transactional};
use entities::document_registry::DocumentType;
use entities::ids::{SalesReturnCaseId, SalesReturnLineId};
use entities::returns::{SalesReturnCase, SalesReturnCaseData, SalesReturnLine, SalesReturnLineData};
use id_generator::next_id;
use validator::Validate;

/// 销售退货处理单列表筛选条件类型（经 `ReturnsExt` 关联类型跨 crate 可达）。
type SalesReturnCaseFilter = <mongodb::Database as ReturnsExt>::SalesReturnCaseFilter;

impl ReturnsService {
    // -----------------------------------------------------------------------
    // 销售退货/拒收处理单
    // -----------------------------------------------------------------------

    /// 分页查询销售退货/拒收处理单列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`return_no`/`sales_order_id`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    pub async fn sales_return_case_list(
        &self,
        params: &SalesReturnCaseListParams,
    ) -> Result<PageView<SalesReturnCaseView>> {
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
        let page = self
            .db
            .sales_return_cases()
            .search_sales_return_cases(&filter, &mut NoTransaction)
            .await?;
        let mut views = Vec::with_capacity(page.items.len());
        for row in page.items {
            views.push(self.sales_return_case_view(row.id).await?);
        }
        Ok(PageView {
            items: views,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 查询销售退货/拒收处理单详情（处理单 + 明细行）。
    ///
    /// # 参数
    /// * `id` - 处理单 ID
    ///
    /// # 返回
    /// 返回完整处理单视图。
    ///
    /// # 错误
    /// * `NotFound` - 处理单不存在
    pub async fn sales_return_case_detail(&self, id: &str) -> Result<SalesReturnCaseView> {
        self.sales_return_case_view(id.to_string()).await
    }

    /// 建立销售退货/拒收处理单与明细行（跨集合事务写入）。
    ///
    /// `return_no` 全局唯一（唯一索引）构成幂等去重；同事务写入处理单与
    /// 明细行（`ReturnsRepository::create_sales_return_with_line`）。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建处理单视图。
    ///
    /// # 错误
    /// * `ConflictError` - 退货处理号重复
    pub async fn create_sales_return_case(
        &self,
        req: CreateSalesReturnCaseRequest,
        actor: &AuditActor,
    ) -> Result<SalesReturnCaseView> {
        req.validate()?;
        let case_id = SalesReturnCaseId::new(next_id());
        let case = SalesReturnCase::new(
            case_id.clone(),
            SalesReturnCaseData {
                return_no: req.return_no,
                sales_order_id: req.sales_order_id,
                acceptance_id: req.acceptance_id,
                case_type: req.case_type,
                reason: req.reason,
                discovered_at: req.discovered_at,
                return_route: req.return_route,
            },
            actor.id(),
        )?;
        let line = SalesReturnLine::new(
            SalesReturnLineId::new(next_id()),
            SalesReturnLineData {
                sales_return_case_id: case_id.clone(),
                sales_order_line_id: req.lines[0].sales_order_line_id.clone(),
                requested_quantity: req.lines[0].requested_quantity,
                received_quantity: None,
                quality_result: None,
                restockable_quantity: None,
            },
        )?;
        let audit = actor.clone().resource_log(
            "sales_return_case.create",
            "sales_return_case",
            case_id.to_string(),
        )?;
        let document =
            new_registered_document(&case_id, DocumentType::SalesReturnCase, case.return_no.clone())?;

        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.returns()
                        .create_sales_return_with_line(&case, &line, session)
                        .await?;
                    persist_registered_document(&db, &document, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        self.sales_return_case_detail(&case_id).await
    }

    // -----------------------------------------------------------------------
    // 私有视图装配
    // -----------------------------------------------------------------------

    /// 装配销售退货/拒收处理单视图。
    ///
    /// # 参数
    /// * `id` - 处理单 ID
    ///
    /// # 返回
    /// 返回完整处理单视图。
    ///
    /// # 错误
    /// * `NotFound` - 处理单不存在
    async fn sales_return_case_view(&self, id: String) -> Result<SalesReturnCaseView> {
        let case = self
            .db
            .sales_return_cases()
            .find_by_id(&id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售退货处理单不存在".to_string()))?;
        let lines = self
            .db
            .sales_return_lines()
            .find_lines_by_cases(&[case.base.id.clone().into()], &mut NoTransaction)
            .await?
            .into_iter()
            .map(|line| crate::returns::dto::SalesReturnLineView {
                id: line.base.id.clone(),
                sales_order_line_id: line.sales_order_line_id.to_string(),
                requested_quantity: line.requested_quantity,
                received_quantity: line.received_quantity,
                quality_result: line.quality_result.map(|result| result.as_str().to_string()),
                restockable_quantity: line.restockable_quantity,
            })
            .collect();
        Ok(SalesReturnCaseView {
            id: case.base.id.clone(),
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
