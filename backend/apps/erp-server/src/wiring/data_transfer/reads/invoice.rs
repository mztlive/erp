use application_core::AuditActor;
use erp_finance::dto::receivable::InvoiceRequestQuery;
use erp_read_models::Error as FundsError;
use erp_read_models::finance::funds_scope::ScopedInvoiceRequestRow;
use erp_workflow::entity::work_item::WorkItemType;
use erp_workflow::service::work_item::read::WorkItemSearchParams;
use erp_workflow::{ErrorCode, WorkItemListParams, WorkItemReadService, WorkItemView};
use serde_json::{Value, json};

use super::{Application, Error, Result, funds_access, input, output};

pub(super) async fn request_page(state: &Application, query: &Value, actor: &AuditActor) -> Result<Value> {
    let query: InvoiceRequestQuery = input(query)?;
    let page = funds_access(state.db(), state.authorization()).request_list_scoped(&query, actor).await?;
    Ok(json!({"total": page.total, "scope_version": page.scope_version,
        "empty_reason": page.empty_reason,
        "items": page.items.iter().map(request_row).collect::<Result<Vec<_>>>()?}))
}

pub(super) async fn task_page(state: &Application, query: &Value, actor: &AuditActor) -> Result<Value> {
    let page = WorkItemReadService::new(state.db(), state.authorization())
        .work_item_search(task_params(query)?, actor.clone())
        .await?;
    Ok(json!({"total": page.total, "scope_version": page.scope_version,
        "queue_context_id": page.queue_context_id, "items": page.items.iter().map(task_row).collect::<Vec<_>>()}))
}

fn task_row(row: &WorkItemView) -> Value {
    json!({"id": row.id, "work_item_type": row.work_item_type, "task_version": row.task_version,
        "receivable_account_id": row.business_object_id, "type_label": row.work_item_type.label(), "status_label": row.status.label(),
        "priority_label": row.priority.label(),
        "owner_user": row.owner_user, "owner_organization": row.owner_organization,
        "created_at": row.created_at, "due_at": row.due_at, "completed_at": row.completed_at,
        "closed_at": row.closed_at})
}

fn task_params(query: &Value) -> Result<WorkItemSearchParams> {
    let mut base: WorkItemListParams = input(query)?;
    base.work_item_type = Some(WorkItemType::SalesInvoiceExecution);
    base.current_work_item_id = None;
    let filters = match &query["filters"] {
        Value::Null => None,
        Value::String(value) => Some(value.clone()),
        Value::Object(_) => Some(query["filters"].to_string()),
        _ => return Err(Error::ValidationError("工作台筛选条件格式不正确".into())),
    };
    Ok(WorkItemSearchParams { base, filters })
}

pub(super) async fn linked_request(
    state: &Application,
    work_item_id: &str,
    receivable_account_id: &str,
    actor: &AuditActor,
) -> Result<Option<Value>> {
    let result = funds_access(state.db(), state.authorization())
        .request_detail_for_work_item_scoped(work_item_id, receivable_account_id, actor)
        .await;
    let request = match result {
        Ok(request) => request,
        Err(
            FundsError::NotFound(_)
            | FundsError::Forbidden(_)
            | FundsError::Coded(ErrorCode::ResourceAuthorizationNotReady),
        ) => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    request
        .map(|request| {
            if request.data.work_item_id.as_deref() != Some(work_item_id) {
                return Err(Error::ConflictError("开票申请关联已变化，请重新导出".into()));
            }
            Ok(json!({"data": request_row(&request.data)?, "scope_version": request.scope_version}))
        })
        .transpose()
}

fn request_row(row: &ScopedInvoiceRequestRow) -> Result<Value> {
    let mut result = output(row)?;
    result["status_label"] = json!(row.status.label());
    result["remaining_amount"] =
        json!(row.amount.checked_sub(row.invoiced_amount).max(erp_core::money::Amount::zero()));
    for (index, line) in row.data.lines.iter().enumerate() {
        result["data"]["lines"][index]["line_amount"] = json!(line.gross_amount()?);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use erp_finance::entity::receivable::InvoiceRequestStatus;
    use erp_workflow::dto::work_item::WorkItemScope;
    use erp_workflow::service::work_item::read::WorkspaceFilters;

    use super::*;

    #[test]
    fn invoice_task_query_keeps_scope_and_applied_filters_but_forces_invoice_kind() {
        for (scope, expected) in [
            ("mine", WorkItemScope::Mine),
            ("history", WorkItemScope::History),
            ("managed", WorkItemScope::Managed),
        ] {
            let query = task_params(&json!({"scope": scope, "work_item_type": "BUSINESS_EXCEPTION",
                "current_work_item_id": "not-a-filter", "q": " SO-1 ", "handler_user_ids": "u1,u2",
                "filters": {"sales_orders": ["so"], "blocked": true}, "page": 2,
                "queue_context_id": "queue", "scope_version": "version"}))
            .unwrap();
            assert_eq!(query.base.scope, expected);
            assert_eq!(query.base.work_item_type, Some(WorkItemType::SalesInvoiceExecution));
            assert_eq!(query.base.current_work_item_id, None);
            assert_eq!(query.base.q.as_deref(), Some(" SO-1 "));
            assert_eq!(query.base.handler_user_ids.as_deref(), Some("u1,u2"));
            assert_eq!(query.base.page, Some(2));
            assert_eq!(query.base.queue_context_id.as_deref(), Some("queue"));
            assert_eq!(query.base.scope_version.as_deref(), Some("version"));
            let filters = WorkspaceFilters::parse(query.filters.as_deref()).unwrap();
            assert_eq!(filters.sales_orders, ["so"]);
            assert!(filters.blocked);
        }
    }

    #[test]
    fn invoice_export_projection_uses_the_domain_rounded_line_amount() {
        let row = ScopedInvoiceRequestRow {
            id: "request".into(),
            version: 1,
            request_no: "KP-1".into(),
            receivable_account_id: "receivable".into(),
            sales_order_id: "sales".into(),
            sales_order_no: "SO-1".into(),
            status: InvoiceRequestStatus::Approved,
            created_at: 1,
            applicant_user_id: "applicant".into(),
            handler_user_id: None,
            work_item_id: None,
            amount: "12.34".parse().unwrap(),
            data: serde_json::from_value(json!({"amount":"12.34", "invoice_title":"购方",
                "tax_number":"001", "invoice_content":"服务", "reason":"开票",
                "lines":[{"content":"服务", "quantity":"1", "unit_price":"12.345"}]}))
            .unwrap(),
            invoiced_amount: "0".parse().unwrap(),
            permission_limited: false,
            sales_owner_user_id: None,
            business_org_unit_id: None,
        };
        let projection = request_row(&row).unwrap();
        assert_eq!(projection["data"]["lines"][0]["line_amount"], "12.34");
        assert_eq!(projection["data"]["lines"][0]["unit_price"], "12.345");
        assert_eq!(projection["data"]["tax_number"], "001");
    }

    #[test]
    fn invoice_task_query_rejects_unsupported_scope_and_non_query_filters() {
        assert!(task_params(&json!({"scope": "started"})).is_err());
        assert!(task_params(&json!({"scope": "mine", "filters": []})).is_err());
    }
}
