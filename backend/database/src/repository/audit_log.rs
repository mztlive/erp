use super::{regex_filter::insert_literal_regex_filter, PageResult, Pagination, QueryFilter, Repository};
use crate::errors::Result;
use crate::{mongo_ops, Executor};
use entities::{AuditLog, CommandReceiptFact};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct CommandReceiptRow {
    id: String,
    actor_id: String,
    action: String,
    resource_type: String,
    resource_id: Option<String>,
    success: bool,
    message: Option<String>,
}

impl From<CommandReceiptRow> for CommandReceiptFact {
    fn from(row: CommandReceiptRow) -> Self {
        Self {
            id: row.id,
            actor_id: row.actor_id,
            action: row.action,
            resource_type: row.resource_type,
            resource_id: row.resource_id,
            success: row.success,
            message: row.message,
        }
    }
}

impl<'a> Repository<'a, AuditLog> {
    /// 按当前及历史候选 ID 批量读取命令收据最小事实。
    ///
    /// 空集合不访问数据库；返回顺序不表达收据优先级，调用方必须
    /// 按候选 ID 顺序选择当前格式或历史格式。
    pub async fn find_command_receipts_by_ids(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<CommandReceiptFact>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut ids = ids.to_vec();
        ids.sort();
        ids.dedup();
        let collection = self.collection().clone_with_type::<CommandReceiptRow>();
        let rows = mongo_ops::find_many(
            &collection,
            doc! {
                "id": { "$in": ids },
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            FindOptions::builder()
                .projection(doc! {
                    "id": 1,
                    "actor_id": 1,
                    "action": 1,
                    "resource_type": 1,
                    "resource_id": 1,
                    "success": 1,
                    "message": 1,
                })
                .build(),
            executor,
        )
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// 在调用方执行器内有序批量创建审计日志。
    ///
    /// 空集合直接返回且不访问数据库；完整原子性由调用方事务负责。
    ///
    /// # 参数
    /// * `logs` - 按对应业务事实顺序排列的审计日志
    /// * `executor` - 调用方事务或非事务执行器
    ///
    /// # 错误
    /// 插入失败时返回包含 MongoDB 批量写错误索引的仓储错误。
    pub async fn create_many_ordered(&self, logs: &[AuditLog], executor: &mut dyn Executor) -> Result<()> {
        if logs.is_empty() {
            return Ok(());
        }
        crate::mongo_ops::insert_many(&self.collection(), logs.to_vec(), executor).await?;
        Ok(())
    }

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
