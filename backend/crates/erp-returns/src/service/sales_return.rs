//! SalesReturnCase 的本域构造与头、首行持久化。
use erp_core::ids::{SalesReturnCaseId, SalesReturnLineId};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;
use crate::dto::CreateSalesReturnCaseRequest;
use crate::entity::returns::{SalesReturnCase, SalesReturnCaseData, SalesReturnLine, SalesReturnLineData};
use crate::repository::ReturnsExt;

/// 由创建请求构造处理单与首条明细。
///
/// 明细验收字段在创建时为空；累计有效退回数量由后续验收事务校验。
///
/// # 参数
/// * `req` - 已通过 `Validate` 的创建请求
/// * `created_by` - 创建人
///
/// # 返回
/// 返回草稿处理单与对应明细。
///
/// # 错误
/// 处理号/原因为空超长，或申请数量非正时返回校验错误。
pub fn build_sales_return_case_and_line(
    req: CreateSalesReturnCaseRequest,
    created_by: &str,
) -> Result<(SalesReturnCase, SalesReturnLine)> {
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
        created_by,
    )?;
    let line = SalesReturnLine::new(
        SalesReturnLineId::new(next_id()),
        SalesReturnLineData {
            sales_return_case_id: case_id,
            sales_order_line_id: req.lines[0].sales_order_line_id.clone(),
            requested_quantity: req.lines[0].requested_quantity,
            received_quantity: None,
            quality_result: None,
            restockable_quantity: None,
        },
    )?;
    Ok((case, line))
}

/// 在调用方 Executor 上依次写入退货头与首条明细。
///
/// # Errors
/// 头或明细写入失败时返回原仓储错误；事务由外层根持有。
pub async fn persist_sales_return_case_with_line(
    db: &Database,
    case: &SalesReturnCase,
    line: &SalesReturnLine,
    executor: &mut dyn Executor,
) -> Result<()> {
    db.returns().create_sales_return_with_line(case, line, executor).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::common::time::Instant;
    use erp_core::ids::{SalesOrderId, SalesOrderLineId};
    use erp_core::money::Quantity;
    use validator::Validate;

    use super::build_sales_return_case_and_line;
    use crate::dto::{CreateSalesReturnCaseRequest, CreateSalesReturnLineRequest};
    use crate::entity::returns::{CaseType, ReturnRoute};

    #[test]
    fn creation_keeps_only_the_first_requested_sales_return_line() {
        let req = CreateSalesReturnCaseRequest {
            return_no: "SR-1".into(),
            sales_order_id: SalesOrderId::new("sales-order"),
            acceptance_id: None,
            case_type: CaseType::Return,
            reason: "破损".into(),
            discovered_at: Instant::from_unix_secs(100),
            return_route: ReturnRoute::CompanyWarehouse,
            lines: vec![
                CreateSalesReturnLineRequest {
                    sales_order_line_id: SalesOrderLineId::new("first-line"),
                    requested_quantity: Quantity::from_str("2").unwrap(),
                },
                CreateSalesReturnLineRequest {
                    sales_order_line_id: SalesOrderLineId::new("ignored-second-line"),
                    requested_quantity: Quantity::from_str("0").unwrap(),
                },
            ],
        };
        req.validate().unwrap();
        let (case, line) = build_sales_return_case_and_line(req, "actor").unwrap();
        assert_eq!(line.sales_return_case_id.as_ref(), case.base.id);
        assert_eq!(line.sales_order_line_id.as_ref(), "first-line");
        assert_eq!(line.requested_quantity, Quantity::from_str("2").unwrap());
        assert!(line.received_quantity.is_none());
        assert!(line.quality_result.is_none());
        assert!(line.restockable_quantity.is_none());
    }
}
