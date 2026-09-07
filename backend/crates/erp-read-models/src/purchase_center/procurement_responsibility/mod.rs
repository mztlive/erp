//! 采购责任规则管理列表及采购责任解析需要的目录事实。
mod catalog;
pub mod dto;
mod facts;
mod ids;
mod mapping;
mod query;
mod repository;

pub use catalog::load_procurement_catalog_bundle;
pub use facts::{collect_rule_list_ids, ProcurementRuleListDisplayFacts, ProcurementRuleListPage};
pub use mapping::{apply_rule_list_facts, to_rule_list_views};
pub use repository::{load_procurement_rule_list_facts, load_procurement_rule_list_page};
/// 只读取分页规则与该页关联显示事实的服务。
pub struct ProcurementResponsibilityReadService {
    db: mongodb::Database,
}
impl ProcurementResponsibilityReadService {
    /// 构造本身无查询和权限副作用，入口保留原 RBAC 与参数校验。
    pub fn new(db: mongodb::Database) -> Self {
        Self { db }
    }
}
