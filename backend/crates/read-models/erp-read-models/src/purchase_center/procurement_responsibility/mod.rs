//! 采购责任规则管理列表及采购责任解析需要的目录事实。
use std::sync::Arc;

use erp_identity::AuthorizationService;
mod catalog;
pub mod dto;
mod facts;
mod ids;
mod mapping;
mod query;
mod repository;

pub use catalog::load_procurement_catalog_bundle;
pub use facts::{ProcurementRuleListDisplayFacts, ProcurementRuleListPage, collect_rule_list_ids};
pub use mapping::{apply_rule_list_facts, to_rule_list_views};
pub use repository::{load_procurement_rule_list_facts, load_procurement_rule_list_page};
/// 只读取分页规则与该页关联显示事实的服务。
pub struct ProcurementResponsibilityReadService {
    db: mongodb::Database,
    authorization: Arc<AuthorizationService>,
}
impl ProcurementResponsibilityReadService {
    /// 绑定共享授权服务和业务数据库，入口执行完整功能资格检查。
    ///
    /// # 参数
    /// * `authorization` - 组合根构造并共享的统一授权服务。
    /// * `db` - 目标数据库。
    ///
    /// # 返回
    /// 返回读服务；构造不查询、不校验权限。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: mongodb::Database, authorization: Arc<AuthorizationService>) -> Self {
        Self { db, authorization }
    }
}
