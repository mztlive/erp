use super::{regex_filter::insert_literal_regex_filter, PageResult, Pagination, QueryFilter, Repository};
use crate::errors::Result;
use crate::Executor;
use entities::AuditLog;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};

impl<'a> Repository<'a, AuditLog> {
    /// 按条件检索审计日志列表。
    ///
    /// # 参数
    /// * `filter` - 审计日志筛选条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回值
    /// 返回分页后的审计日志集合
    ///
    /// # 错误
    /// 当 MongoDB 查询或计数失败时返回错误。
    pub async fn search_logs(
        &self,
        filter: &AuditLogFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<AuditLog>> {
        self.search(filter, executor).await
    }

    /// 按资源读取全部成功审计事实。
    ///
    /// # 参数
    /// * `resource_type` - 资源类型稳定代码
    /// * `resource_id` - 资源业务 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回该资源全部成功且未删除的审计日志；调用方按业务动作判定所需事实。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_successful_by_resource(
        &self,
        resource_type: &str,
        resource_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<AuditLog>> {
        self.find_many(
            doc! {
                "resource_type": resource_type,
                "resource_id": resource_id,
                "success": true,
            },
            executor,
        )
        .await
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
