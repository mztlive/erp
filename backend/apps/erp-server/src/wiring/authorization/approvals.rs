//! 审批实例的拥有领域实现绑定。
mod sources;
use std::collections::BTreeMap;
use std::sync::Arc;

use erp_identity::{AccessMapping, Result};
use erp_permissions::{
    approval_instance, internal, inventory, payable, purchase_order, purchase_payment_request, receivable,
    sales_order,
};
use erp_read_models::approval_subject::ApprovalManagementMapping;
use erp_workflow::authorization::approval_instances;
use mongodb::Database;
use sources::SubjectFacts;

use super::{FactsBindings, MappingBindings};

// 连接审批实例唯一事实读取器及列表查询映射。
// 审批实例的资源绑定与固定查询入口。
// 拥有领域模型构造错误原样返回，不使用占位实现。
pub(super) fn bindings(
    db: Database,
    roots: &FactsBindings,
    mappings: &MappingBindings,
) -> Result<(FactsBindings, MappingBindings)> {
    let subjects = Arc::new(SubjectFacts::new(db.clone(), roots));
    let (facts, mapping) = approval_instances::bindings(db, subjects)?;
    let sources = source_mappings(mappings)?;
    let management = Arc::new(ApprovalManagementMapping::new(mapping.clone(), &sources)?);
    Ok((
        vec![("approval_instance".into(), facts)],
        vec![
            (approval_instance::INSTANCE_LIST, mapping.clone()),
            (internal::APPROVAL_STARTED_LIST, mapping.clone()),
            (internal::APPROVAL_MINE_LIST, mapping),
            (internal::APPROVAL_MANAGEMENT_READ, management),
        ],
    ))
}

// 为审批来源绑定实际业务对象的固定查询实现，不让同资源下的任务投影覆盖订单授权模型。
fn source_mappings(mappings: &MappingBindings) -> Result<BTreeMap<String, Arc<dyn AccessMapping>>> {
    [
        sales_order::DETAIL,
        purchase_order::DETAIL,
        inventory::STOCK_ADJUSTMENT_DETAIL,
        receivable::INVOICE_REQUEST_INVOICE_REQUEST_DETAIL,
        purchase_payment_request::DETAIL,
        receivable::CUSTOMER_RECEIPT_DETAIL,
        payable::SUPPLIER_PAYMENT_DETAIL,
        receivable::ACCOUNT_DETAIL,
        payable::ACCOUNT_DETAIL,
    ]
    .into_iter()
    .map(|permission| {
        let mapping = super::mapping(mappings, &permission)?;
        Ok((permission.resource().into(), mapping))
    })
    .collect()
}
