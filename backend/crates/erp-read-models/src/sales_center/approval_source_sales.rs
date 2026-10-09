//! 采购审批允许范围内的完整销售版本预览；不查询当前单据或最新版本。

use erp_core::ids::{SalesOrderRevisionId, SalesOrderRevisionLineId};
use erp_sales::dto::sales_order::{RevisionView, SalesOrderWorkingCopyLineView};
use erp_sales::entity::sales_order::{SalesOrderRevision, SalesOrderRevisionLine};
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
use erp_sales::service::sales_order::mapper::revision_view;
use erp_workflow::entity::approval_integration::display_snapshot::ApprovalRelatedSalesSnapshot;
use erp_workflow::service::approval::execution::runtime_service::ApprovalMaterialsView;
use mongodb::Database;
use persistence_core::NoTransaction;
use serde::Serialize;

use crate::{Error, Result};

/// 来源销售单的业务编号及冻结版本完整正文。
#[derive(Debug, Serialize)]
pub struct ApprovalSourceSalesOrder {
    pub document_no: String,
    pub revision: RevisionView,
}

/// 在审批实例授权完成后，批量读取允许清单内的精确销售版本。
/// # 参数
/// 数据库及已经通过实例授权的冻结资料。
/// # 返回
/// 采购审批允许的完整来源版本；历史未冻结来源时返回空列表。
/// # 错误
/// 版本缺失、跨单据、版本号或完整行数不符、子类型缺失时拒绝。
pub(super) async fn source_sales_orders(
    db: &Database,
    materials: &ApprovalMaterialsView,
) -> Result<Vec<ApprovalSourceSalesOrder>> {
    let allowed = &materials.display.source_sales;
    if materials.document_type != "purchase_order" || allowed.is_empty() {
        return Ok(vec![]);
    }
    let ids = allowed.iter().map(|source| source.revision_id.clone()).collect::<Vec<_>>();
    let revisions = db.sales_order_revisions().find_revisions_by_ids(&ids, &mut NoTransaction).await?;
    let lines = db
        .sales_order_revision_lines()
        .list_lines_by_revisions(
            &ids.iter().map(SalesOrderRevisionId::new).collect::<Vec<_>>(),
            &mut NoTransaction,
        )
        .await?;
    let commercial = commercial_lines(db, &lines).await?;
    allowed
        .iter()
        .map(|source| {
            let revision = revisions
                .iter()
                .find(|row| row.base.id == source.revision_id)
                .ok_or_else(|| Error::ConflictError("来源销售版本不存在".into()))?;
            source_view(source, revision, &lines, &commercial)
        })
        .collect()
}

fn source_view(
    source: &ApprovalRelatedSalesSnapshot,
    revision: &SalesOrderRevision,
    lines: &[SalesOrderRevisionLine],
    commercial: &[SalesOrderWorkingCopyLineView],
) -> Result<ApprovalSourceSalesOrder> {
    let mut rows = lines
        .iter()
        .filter(|line| line.sales_order_revision_id.as_ref() == source.revision_id)
        .cloned()
        .collect::<Vec<_>>();
    validate_source(source, revision.sales_order_id.as_ref(), revision.revision.revision_no, rows.len())?;
    rows.sort_by_key(|line| line.line_no);
    let details = rows
        .iter()
        .map(|line| {
            commercial
                .iter()
                .find(|view| view.id == line.base.id)
                .cloned()
                .ok_or_else(|| Error::ConflictError("来源销售成交明细不完整".into()))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(ApprovalSourceSalesOrder {
        document_no: source.document_no.clone(),
        revision: revision_view(revision.clone(), rows, None, details),
    })
}

async fn commercial_lines(
    db: &Database,
    lines: &[SalesOrderRevisionLine],
) -> Result<Vec<SalesOrderWorkingCopyLineView>> {
    let ids = lines.iter().map(|line| SalesOrderRevisionLineId::new(&line.base.id)).collect::<Vec<_>>();
    let goods = db
        .sales_order_goods_service_line_revisions()
        .list_by_revision_line_ids(&ids, &mut NoTransaction)
        .await?;
    let vouchers =
        db.sales_order_voucher_line_revisions().list_by_revision_line_ids(&ids, &mut NoTransaction).await?;
    lines
        .iter()
        .map(|line| {
            Ok(SalesOrderWorkingCopyLineView::from_revision_line(
                line,
                goods.iter().find(|row| row.revision_line_id.as_ref() == line.base.id),
                vouchers.iter().find(|row| row.revision_line_id.as_ref() == line.base.id),
            )?)
        })
        .collect()
}

fn validate_source(
    source: &ApprovalRelatedSalesSnapshot,
    order_id: &str,
    revision_no: u32,
    count: usize,
) -> Result<()> {
    let expected =
        u32::try_from(source.source.lines.len()).ok().and_then(|n| n.checked_add(source.source.more_count));
    if source.document_id != order_id
        || source.revision_no != revision_no
        || expected != u32::try_from(count).ok()
    {
        return Err(Error::ConflictError("来源销售版本与本次审批资料不一致".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use erp_workflow::entity::approval_integration::display_snapshot::ApprovalBriefSource;

    use super::*;

    #[test]
    fn source_preview_requires_exact_order_version_and_complete_lines() {
        let source = ApprovalRelatedSalesSnapshot {
            document_id: "sales-1".into(),
            document_no: "XS-1".into(),
            revision_id: "revision-2".into(),
            revision_no: 2,
            source: ApprovalBriefSource { more_count: 19, ..Default::default() },
        };
        assert!(validate_source(&source, "sales-1", 2, 19).is_ok());
        assert!(validate_source(&source, "sales-2", 2, 19).is_err());
        assert!(validate_source(&source, "sales-1", 3, 19).is_err());
        assert!(validate_source(&source, "sales-1", 2, 3).is_err());
        assert!(validate_source(&source, "sales-1", 2, 0).is_err());
    }
}
