//! 销售提交审计以快照为资源；通过不可变提交记录回到稳定销售单。
use erp_sales::entity::sales_order::SalesOrderId;
use erp_sales::repository::{SalesOrderExt, SalesOrderSubmissionRepositoryExt};
use erp_workflow::DocumentType;
use mongodb::Database;
use persistence_core::Executor;

use super::super::TimelineCategory;
use super::super::sources::Sources;
use crate::Result;

pub(super) async fn append(db: &Database, sources: &mut Sources, executor: &mut dyn Executor) -> Result<()> {
    let orders = sources.documents.values().filter(|document| {
        matches!(document.kind, DocumentType::SalesOrder | DocumentType::VoucherSalesOrder)
    });
    for document in orders {
        let rows = db
            .sales_order_submissions()
            .list_by_order_newest_first(&SalesOrderId::new(&document.id), executor)
            .await?;
        for row in rows {
            let Ok(time) = u64::try_from(row.submitted_at.unix_secs()) else {
                continue;
            };
            let mut event = document.event(
                format!("submission:{}", row.base.id),
                time,
                format!("提交销售单（第 {} 次）", row.submission_no),
                "未记录姓名".into(),
            );
            event.category = TimelineCategory::Approval;
            event.actor_id = Some(row.submitted_by);
            sources.facts.push(event);
        }
    }
    Ok(())
}
