use super::{regex_filter::insert_literal_regex_filter, PageResult, Pagination, QueryFilter, Repository};
use crate::errors::Result;
use entities::AuditLog;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};

impl<'a> Repository<'a, AuditLog> {
    /// 按条件检索审计日志列表。
    ///
    /// # 参数
    /// * `filter` - 审计日志筛选条件
    ///
    /// # 返回值
    /// 返回分页后的审计日志集合
    pub async fn search_logs(&self, filter: &AuditLogFilter) -> Result<PageResult<AuditLog>> {
        self.search(filter).await
    }
}

/// 审计日志列表过滤条件。
#[derive(Debug, Clone)]
pub struct AuditLogFilter {
    pub actor_account: Option<String>,
    pub action: Option<String>,
    pub resource_type: Option<String>,
    pub success: Option<bool>,
    pub page: u64,
    pub page_size: u32,
}

impl QueryFilter for AuditLogFilter {
    /// 转换为 MongoDB 查询条件。
    ///
    /// # 返回值
    /// 返回查询条件文档
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };

        insert_literal_regex_filter(&mut filter, "actor_account", self.actor_account.as_deref());
        insert_literal_regex_filter(&mut filter, "action", self.action.as_deref());

        if let Some(resource_type) = &self.resource_type {
            filter.insert("resource_type", resource_type);
        }

        if let Some(success) = self.success {
            filter.insert("success", success);
        }

        filter
    }
}

impl Pagination for AuditLogFilter {
    /// 返回页码和分页大小。
    ///
    /// # 返回值
    /// 返回原始页码与单页条目数。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

#[cfg(test)]
mod tests {
    use super::{AuditLogFilter, QueryFilter};

    #[test]
    fn action_filter_treats_regex_metacharacters_as_literal_text() {
        let filter = AuditLogFilter {
            actor_account: None,
            action: Some("audit.create+".to_string()),
            resource_type: None,
            success: None,
            page: 1,
            page_size: 20,
        }
        .to_doc();

        assert_eq!(
            filter.get_document("action").unwrap().get_str("$regex").unwrap(),
            r"audit\.create\+"
        );
    }
}
