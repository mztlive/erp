//! 询价和导入预览的事实及静态查询实现绑定。
use erp_data_import::sales_inquiry_import::preview_facts_reader;
use erp_identity::Result;
use erp_order_to_cash::authorization_bindings::{
    inquiry_facts, inquiry_mapping, procurement_facts, procurement_mapping,
};
use erp_permissions::{internal, resources, sales_inquiry};
use mongodb::Database;

use super::{FactsBindings, MappingBindings};

/// 绑定销售询价、精确采购任务和本人导入预览的完整实现。
/// # 参数
/// db 为进程共享业务库。
/// # 返回
/// 按资源唯一的事实和按权限唯一的查询映射。
/// # 错误
/// 不执行 I/O；完整绑定能力由统一 Builder 校验。
pub(super) fn bindings(db: Database) -> Result<(FactsBindings, MappingBindings)> {
    let facts: FactsBindings = vec![
        (resources::SALES_INQUIRY.into(), inquiry_facts(db.clone())),
        (resources::PROCUREMENT_INQUIRY.into(), procurement_facts(db.clone())),
        (internal::SALES_INQUIRY_IMPORT_PREVIEW.resource().into(), preview_facts_reader(db)),
    ];
    let sales = inquiry_mapping();
    let procurement = procurement_mapping();
    let mappings: MappingBindings = vec![
        (sales_inquiry::QUERY_SALES_LIST, sales.clone()),
        (sales_inquiry::QUERY_SALES_DETAIL, sales),
        (sales_inquiry::QUERY_PROCUREMENT_LIST, procurement.clone()),
        (sales_inquiry::QUERY_PROCUREMENT_DETAIL, procurement.clone()),
        (sales_inquiry::INTERNAL_FEE_BASES, procurement),
    ];
    Ok((facts, mappings))
}
