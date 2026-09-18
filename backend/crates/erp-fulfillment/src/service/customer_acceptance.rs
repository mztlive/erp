//! 客户验收单域列表、详情和视图映射。
use erp_core::ids::CustomerAcceptanceLineId;
use persistence_core::NoTransaction;
use validator::Validate;

use super::FulfillmentService;
use crate::Result;
use crate::dto::{
    AcceptanceAllocationView, CustomerAcceptanceDetailView, CustomerAcceptanceLineView,
    CustomerAcceptanceListParams, CustomerAcceptanceView,
};
use crate::entity::fulfillment::{
    AcceptanceFulfillmentAllocation, CustomerAcceptance, CustomerAcceptanceLine,
};
use crate::repository::FulfillmentExt;
use crate::repository::prelude::*;

/// 客户验收单列表筛选条件类型。
type CustomerAcceptanceFilter = <mongodb::Database as FulfillmentExt>::CustomerAcceptanceFilter;

impl FulfillmentService {
    // ---------------------------------------------------------- customer_acceptance

    /// 分页查询客户验收单列表（W06 验收历史视图）。
    ///
    /// # 参数
    /// * `params` - 查询参数（`sales_order_id`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    #[tracing::instrument(
        name = "fulfillment.customer_acceptance_list",
        skip_all,
        fields(layer = "service", domain = "fulfillment", operation = "customer_acceptance_list")
    )]
    pub async fn customer_acceptance_list(
        &self,
        params: &CustomerAcceptanceListParams,
    ) -> Result<crate::dto::PageView<CustomerAcceptanceView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = CustomerAcceptanceFilter {
            sales_order_id: query.sales_order_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: super::sort_ascending(query.paging.sort_dir),
        };
        super::map_search_page(
            self.db.customer_acceptances().search_customer_acceptances(&filter, &mut NoTransaction),
            |row| CustomerAcceptanceView {
                id: row.id,
                acceptance_no: row.acceptance_no,
                sales_order_id: row.sales_order_id.to_string(),
                accepted_at: row.accepted_at.unix_secs(),
                result: row.result,
                status: row.status,
                reversal_of_acceptance_id: row.reversal_of_acceptance_id.map(|id| id.to_string()),
                version: row.version,
                created_at: row.created_at,
            },
            filter.page,
            filter.page_size,
        )
        .await
    }

    /// 查询客户验收单详情（表头 + 行 + 分配）。
    ///
    /// # 参数
    /// * `id` - 验收单主键
    ///
    /// # 返回
    /// 返回验收单详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 验收单不存在
    /// * `RepositoryError` - 数据库查询失败
    #[tracing::instrument(
        name = "fulfillment.customer_acceptance_detail",
        skip_all,
        fields(layer = "service", domain = "fulfillment", operation = "customer_acceptance_detail")
    )]
    pub async fn customer_acceptance_detail(&self, id: &str) -> Result<CustomerAcceptanceDetailView> {
        let acceptance = super::find_header_or_not_found(
            self.db.customer_acceptances().find_by_id(id, &mut NoTransaction),
            "客户验收单不存在",
        )
        .await?;
        let lines = self
            .db
            .fulfillment()
            .acceptance_lines_by_acceptance_ids(&[acceptance.base.id.clone().into()], &mut NoTransaction)
            .await?;
        let line_ids: Vec<CustomerAcceptanceLineId> =
            lines.iter().map(|line| line.base.id.clone().into()).collect();
        let allocations =
            self.db.fulfillment().allocations_by_acceptance_lines(&line_ids, &mut NoTransaction).await?;
        Ok(CustomerAcceptanceDetailView {
            acceptance: acceptance.into(),
            lines: lines.into_iter().map(Into::into).collect(),
            allocations: allocations.into_iter().map(Into::into).collect(),
        })
    }
}

impl From<CustomerAcceptance> for CustomerAcceptanceView {
    /// 从验收单实体构造视图。
    fn from(acceptance: CustomerAcceptance) -> Self {
        Self {
            id: acceptance.base.id,
            acceptance_no: acceptance.acceptance_no,
            sales_order_id: acceptance.sales_order_id.to_string(),
            accepted_at: acceptance.accepted_at.unix_secs(),
            result: acceptance.result,
            status: acceptance.status,
            reversal_of_acceptance_id: acceptance.reversal_of_acceptance_id.map(|id| id.to_string()),
            version: acceptance.base.version,
            created_at: acceptance.base.created_at,
        }
    }
}

impl From<CustomerAcceptanceLine> for CustomerAcceptanceLineView {
    /// 从验收行实体构造视图。
    fn from(line: CustomerAcceptanceLine) -> Self {
        Self {
            id: line.base.id,
            line_no: line.line_no,
            sales_order_line_id: line.sales_order_line_id.to_string(),
            accepted_quantity: line.accepted_quantity,
            short_quantity: line.short_quantity,
            rejected_quantity: line.rejected_quantity,
            reason: line.reason,
        }
    }
}

impl From<AcceptanceFulfillmentAllocation> for AcceptanceAllocationView {
    /// 从验收履约分配实体构造视图。
    fn from(allocation: AcceptanceFulfillmentAllocation) -> Self {
        Self {
            id: allocation.base.id,
            customer_acceptance_line_id: allocation.customer_acceptance_line_id.to_string(),
            fulfillment_fact_type: allocation.fulfillment_fact_type,
            fulfillment_line_id: allocation.fulfillment_line_id,
            allocation_action: allocation.allocation_action,
            allocated_quantity: allocation.allocated_quantity,
            reverses_allocation_id: allocation.reverses_allocation_id.map(|id| id.to_string()),
        }
    }
}

/// 按创建入口原顺序构造验收表头及验收行；单号由组合层在原时点取得。
///
/// # 错误
/// 表头或行数量无效时返回原领域错误。
pub fn prepare_customer_acceptance_draft(
    id: erp_core::ids::CustomerAcceptanceId,
    acceptance_no: String,
    req: crate::dto::CreateCustomerAcceptanceRequest,
) -> Result<(CustomerAcceptance, Vec<CustomerAcceptanceLine>)> {
    let acceptance = CustomerAcceptance::new(
        id.clone(),
        crate::entity::fulfillment::CustomerAcceptanceData {
            acceptance_no,
            sales_order_id: req.sales_order_id,
            accepted_at: erp_core::common::time::Instant::from_unix_secs(req.accepted_at),
            result: req.result,
        },
    )?;
    let lines = super::FulfillmentService::build_customer_acceptance_lines(id, &req.lines)?;
    Ok((acceptance, lines))
}
#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::ids::{CustomerAcceptanceId, SalesOrderLineId};
    use erp_core::money::Quantity;

    use crate::dto::AcceptanceLineInput;
    use crate::entity::fulfillment::CustomerAcceptanceLineBatch;
    use crate::service::customer_acceptance_lines::acceptance_line_specs;

    #[test]
    fn acceptance_lines_are_built_and_validated() {
        let lines = CustomerAcceptanceLineBatch::build(
            CustomerAcceptanceId::new("acc-1"),
            acceptance_line_specs(&[AcceptanceLineInput {
                sales_order_line_id: SalesOrderLineId::new("so-line-1"),
                accepted_quantity: Quantity::from_str("9").unwrap(),
                short_quantity: Quantity::from_str("1").unwrap(),
                rejected_quantity: Quantity::from_str("0").unwrap(),
                reason: None,
                allocations: vec![],
            }]),
        )
        .unwrap();
        assert_eq!(lines[0].accepted_quantity, Quantity::from_str("9").unwrap());
    }
}
