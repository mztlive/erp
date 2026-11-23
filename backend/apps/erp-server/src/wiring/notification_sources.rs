//! 只装配各拥有仓储公开的来源，不读取或复制业务记录。
use erp_finance::repository::{PayableExt, PurchasePaymentRequestExt, ReceivableExt};
use erp_fulfillment::repository::FulfillmentExt;
use erp_identity::repository::{AccessControlExt, organization};
use erp_notification::RefreshTopic;
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::repository::purchase_source_update::purchase_source_updates;
use erp_sales::repository::{SalesInquiryExt, SalesOrderExt, SalesReviewExt, SalesSelectionExt};
use erp_support::repository::BulkJobExt;
use erp_workflow::repository::{BpmExt, WorkItemExt, approval_copy};
use persistence_core::change_feed::ChangeSource;

use super::Application;

// 返回所有导航徽标与单据分区的变更来源。
// 来源白名单；任何对象标识和正文均不进入刷新载荷。
pub(crate) fn sources(app: &Application) -> Vec<ChangeSource<RefreshTopic>> {
    use RefreshTopic::*;
    let db = app.db();
    vec![
        app.notifications().changes(Inbox),
        db.work_items().changes(Tasks),
        db.approval_process_instances().changes(Approvals),
        approval_copy::changes(Copies),
        db.sales_inquiries().changes(Inquiries),
        db.sales_selection_booklets().changes(Selection),
        db.background_jobs().changes(Jobs),
        db.sales_orders().changes(Sales),
        db.sales_change_orders().changes(Sales),
        db.purchase_orders().changes(Purchase),
        db.purchase_change_orders().changes(Purchase),
        db.purchase_line_sales_allocations().changes(Purchase),
        purchase_source_updates(&db).changes(Purchase),
        db.purchase_payment_requests().changes(Finance),
        db.payable_accounts().changes(Finance),
        db.payment_allocations().changes(Finance),
        db.receivable_accounts().changes(Finance),
        db.receipt_allocations().changes(Finance),
        db.deliveries().changes(Fulfillment),
        db.purchase_receipts().changes(Fulfillment),
        db.service_fulfillments().changes(Fulfillment),
        db.electronic_deliveries().changes(Fulfillment),
        db.customer_acceptances().changes(Fulfillment),
        db.accounts().changes(Authorization),
        db.roles().changes(Authorization),
        db.person_data_scopes().changes(Authorization),
        organization::changes(Authorization),
    ]
}
