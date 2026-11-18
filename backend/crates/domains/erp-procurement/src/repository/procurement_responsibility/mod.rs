//! 采购责任规则仓储查询。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{Document, doc};
use persistence_core::{Pagination, QueryFilter};

use crate::entity::procurement_responsibility::{EnableStatus, ProcurementResponsibilityRuleType};

/// 采购责任规则列表筛选条件。
#[derive(Debug, Clone)]
pub struct ProcurementResponsibilityRuleFilter {
    /// 规则类型；`None` 表示不筛选。
    pub rule_type: Option<ProcurementResponsibilityRuleType>,
    /// 负责人账号 ID；`None` 表示不筛选。
    pub owner_user_id: Option<String>,
    /// 启停状态；`None` 表示不筛选。
    pub status: Option<EnableStatus>,
    /// 页码，从 1 开始。
    pub page: u64,
    /// 每页条数。
    pub page_size: u32,
}

impl Default for ProcurementResponsibilityRuleFilter {
    // 列表未传筛选时默认第 1 页、每页 20 条，不另加筛选条件。
    //
    // 返回规则类型、负责人和状态均不筛选的首页条件（`page` 为 1，`page_size` 为 20）。
    fn default() -> Self {
        Self { rule_type: None, owner_user_id: None, status: None, page: 1, page_size: 20 }
    }
}

impl QueryFilter for ProcurementResponsibilityRuleFilter {
    // 构造包含软删除约束的 MongoDB 查询文档。
    //
    // `rule_type`、`owner_user_id`、`status` 为 `None` 时不写入；有值时规则类型与状态写入稳定代码。
    //
    // 返回规则类型、负责人及状态筛选文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(rule_type) = self.rule_type {
            filter.insert("rule_type", rule_type.as_str());
        }
        if let Some(owner_user_id) = self.owner_user_id.as_deref() {
            filter.insert("owner_user_ids", owner_user_id);
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for ProcurementResponsibilityRuleFilter {
    // 读取页码与每页条数。
    //
    // `page` 原样返回，`page_size` 由 `u32` 拓宽为 `u64`。
    //
    // 返回 `(page, page_size)`。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

mod query;
pub(crate) mod rotation;
pub use query::ProcurementResponsibilityRuleRepositoryExt;

pub(crate) mod migration;
