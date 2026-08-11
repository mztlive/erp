use database::{AccessControlExt, FulfillmentExt, NoTransaction, Transactional};
use entities::common::time::Instant;
use entities::fulfillment::{
    AcceptanceFulfillmentAllocation, CustomerAcceptance, CustomerAcceptanceData, CustomerAcceptanceLine,
    CustomerAcceptanceLineData,
};
use entities::ids::{CustomerAcceptanceId, CustomerAcceptanceLineId};
use id_generator::next_id;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

use super::dto::SortDir;
use super::{
    AcceptanceAllocationView, AcceptanceLineInput, CreateCustomerAcceptanceRequest,
    CustomerAcceptanceDetailView, CustomerAcceptanceLineView, CustomerAcceptanceListParams,
    CustomerAcceptanceView, FulfillmentService, PageView,
};

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
    pub async fn customer_acceptance_list(
        &self,
        params: &CustomerAcceptanceListParams,
    ) -> Result<PageView<CustomerAcceptanceView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = CustomerAcceptanceFilter {
            sales_order_id: query.sales_order_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .customer_acceptances()
            .search_customer_acceptances(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| CustomerAcceptanceView {
                id: row.id,
                acceptance_no: row.acceptance_no,
                sales_order_id: row.sales_order_id.to_string(),
                accepted_at: row.accepted_at.unix_secs(),
                result: row.result,
                status: row.status,
                reversal_of_acceptance_id: row.reversal_of_acceptance_id.map(|id| id.to_string()),
                version: row.version,
                created_at: row.created_at,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
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
    pub async fn customer_acceptance_detail(&self, id: &str) -> Result<CustomerAcceptanceDetailView> {
        let acceptance = self
            .db
            .customer_acceptances()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("客户验收单不存在".to_string()))?;
        let lines = self
            .db
            .fulfillment()
            .acceptance_lines_by_acceptance_ids(&[acceptance.base.id.clone().into()], &mut NoTransaction)
            .await?;
        let line_ids: Vec<CustomerAcceptanceLineId> =
            lines.iter().map(|line| line.base.id.clone().into()).collect();
        let allocations = self
            .db
            .fulfillment()
            .allocations_by_acceptance_lines(&line_ids, &mut NoTransaction)
            .await?;
        Ok(CustomerAcceptanceDetailView {
            acceptance: acceptance.into(),
            lines: lines.into_iter().map(Into::into).collect(),
            allocations: allocations.into_iter().map(Into::into).collect(),
        })
    }

    /// 创建客户验收单（草稿，跨集合：表头 + 行 + 审计）。
    ///
    /// 创建阶段不写验收分配；分配在过账时按行守恒与履约事实上限校验后写入。
    ///
    /// # 参数
    /// * `req` - 创建请求（表头 + 行）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建验收单的响应视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `ConflictError` - 单号重复（唯一索引透出）
    /// * `RepositoryError` - 数据库写入失败
    pub async fn create_customer_acceptance(
        &self,
        req: CreateCustomerAcceptanceRequest,
        actor: &AuditActor,
    ) -> Result<CustomerAcceptanceView> {
        req.validate()?;
        let id = CustomerAcceptanceId::new(next_id());
        let acceptance = CustomerAcceptance::new(
            id.clone(),
            CustomerAcceptanceData {
                acceptance_no: req.acceptance_no,
                sales_order_id: req.sales_order_id,
                accepted_at: Instant::from_unix_secs(req.accepted_at),
                result: req.result,
            },
        )?;
        let lines = build_acceptance_lines(&id, &req.lines)?;
        let audit = actor.clone().resource_log(
            "customer_acceptance.create",
            "customer_acceptance",
            id.to_string(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let acceptance_for_tx = acceptance.clone();
        let lines_for_tx = lines.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.fulfillment()
                        .create_customer_acceptance_with_lines(&acceptance_for_tx, &lines_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;
        Ok(acceptance.into())
    }
}

/// 构建验收行实体集合（行号从 1 递增）。
///
/// # 参数
/// * `acceptance_id` - 验收单主键
/// * `inputs` - 行输入
///
/// # 返回
/// 返回行实体集合。
///
/// # 错误
/// 行数量为负或说明超长时返回错误（实体构造）。
fn build_acceptance_lines(
    acceptance_id: &CustomerAcceptanceId,
    inputs: &[AcceptanceLineInput],
) -> Result<Vec<CustomerAcceptanceLine>> {
    let mut lines = Vec::with_capacity(inputs.len());
    for (index, input) in inputs.iter().enumerate() {
        lines.push(
            CustomerAcceptanceLine::new(
                CustomerAcceptanceLineId::new(next_id()),
                CustomerAcceptanceLineData {
                    customer_acceptance_id: acceptance_id.clone(),
                    line_no: index as u32 + 1,
                    sales_order_line_id: input.sales_order_line_id.clone(),
                    accepted_quantity: input.accepted_quantity,
                    short_quantity: input.short_quantity,
                    rejected_quantity: input.rejected_quantity,
                    reason: input.reason.clone(),
                    evidence_attachment_id: None,
                },
            )
            .map_err(Error::Logic)?,
        );
    }
    Ok(lines)
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

#[cfg(test)]
mod tests {
    use super::build_acceptance_lines;
    use crate::fulfillment::AcceptanceLineInput;
    use entities::ids::{CustomerAcceptanceId, SalesOrderLineId};
    use entities::money::Quantity;
    use std::str::FromStr;

    #[test]
    fn acceptance_lines_are_built_and_validated() {
        let lines = build_acceptance_lines(
            &CustomerAcceptanceId::new("acc-1"),
            &[AcceptanceLineInput {
                sales_order_line_id: SalesOrderLineId::new("so-line-1"),
                accepted_quantity: Quantity::from_str("9").unwrap(),
                short_quantity: Quantity::from_str("1").unwrap(),
                rejected_quantity: Quantity::from_str("0").unwrap(),
                reason: None,
                allocations: vec![],
            }],
        )
        .unwrap();
        assert_eq!(lines[0].accepted_quantity, Quantity::from_str("9").unwrap());
    }
}
