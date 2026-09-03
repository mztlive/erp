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

/// 职责分离校验的最小审计事实投影（FIN-R13）。
///
/// 只携带策略解释所需的 actor/action/资源三元组；调用方 Service 继续解释
/// SoD 政策、当前 actor 与拒绝文案。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SeparationAuditFact {
    /// 资源类型稳定代码。
    pub resource_type: String,
    /// 资源业务 ID。
    pub resource_id: Option<String>,
    /// 经办人账号 ID。
    pub actor_id: String,
    /// 审计动作（含版本化后缀，如 `customer_receipt.post:`）。
    pub action: String,
}

impl<'a> Repository<'a, AuditLog> {
    /// 按资源 pair 集合批量返回最小职责分离事实（FIN-R13）。
    ///
    /// 单次 `$or` 查询全部 `(resource_type, resource_id)` 的成功审计，
    /// 只投影 actor/action/资源三元组；空输入不访问数据库。仅成功事件计入
    /// 证据，非正式动作由 Service 按前缀判定，本方法不解释 SoD 政策。
    ///
    /// # 参数
    /// * `pairs` - 资源 pair 集合，每项为 `(resource_type, resource_id)`
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部命中资源的最小事实；无命中返回空集合（调用方 fail closed）。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_separation_facts_by_resources(
        &self,
        pairs: &[(String, String)],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SeparationAuditFact>> {
        if pairs.is_empty() {
            return Ok(Vec::new());
        }
        let mut sorted = pairs.to_vec();
        sorted.sort();
        sorted.dedup();
        let alternatives = sorted
            .into_iter()
            .map(|(resource_type, resource_id)| {
                doc! {
                    "resource_type": resource_type,
                    "resource_id": resource_id,
                }
            })
            .collect::<Vec<_>>();
        let collection = self.collection().clone_with_type::<SeparationAuditFact>();
        mongo_ops::find_many(
            &collection,
            doc! {
                "$or": alternatives,
                "success": true,
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            FindOptions::builder()
                .projection(doc! {
                    "resource_type": 1,
                    "resource_id": 1,
                    "actor_id": 1,
                    "action": 1,
                })
                .build(),
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, AuditLog> {
    /// 查询映射任务的不可变审计时间线。
    ///
    /// # 参数
    /// * `mapping_task_id` - 映射任务 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回资源匹配的审计记录，按创建时间与 ID 升序排列。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    ///
    /// # 约束
    /// 仅查询本仓储拥有的审计日志集合，按资源引用过滤映射任务，不访问映射任务集合。
    pub async fn list_master_mapping_task_history(
        &self,
        mapping_task_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<AuditLog>> {
        self.find_many_sorted(
            doc! {
                "resource_type": "MASTER_MAPPING_TASK",
                "resource_id": mapping_task_id,
            },
            doc! { "created_at": 1, "id": 1 },
            executor,
        )
        .await
    }

    /// 批量读取指定资源的成功创建审计。
    ///
    /// 工作项简报 hydration 入口：只返回动作固定为 `<resource_type>.create` 的
    /// 成功审计，供调用方解析创建人事实。
    ///
    /// # 参数
    /// * `resource_type` - 资源类型
    /// * `resource_ids` - 资源 ID 集合；为空时直接返回空集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回动作固定为 `<resource_type>.create` 的成功审计。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    ///
    /// # 约束
    /// 仅查询本仓储拥有的审计日志集合，不访问业务资源集合。
    pub async fn list_work_item_creation_audits(
        &self,
        resource_type: &str,
        resource_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<AuditLog>> {
        if resource_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(
            doc! {
                "resource_type": resource_type,
                "resource_id": { "$in": resource_ids },
                "action": format!("{resource_type}.create"),
                "success": true,
            },
            executor,
        )
        .await
    }

    /// 批量读取指定资源的全部成功工作项事实审计。
    ///
    /// 工作项简报 hydration 入口：返回资源类型和 ID 命中的全部成功审计，
    /// 调用方按业务动作判定所需事实。
    ///
    /// # 参数
    /// * `resource_type` - 资源类型
    /// * `resource_ids` - 资源 ID 集合；为空时直接返回空集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回资源类型和 ID 命中的全部成功审计。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    ///
    /// # 约束
    /// 仅查询本仓储拥有的审计日志集合，不访问业务资源集合。
    pub async fn list_successful_work_item_fact_audits(
        &self,
        resource_type: &str,
        resource_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<AuditLog>> {
        if resource_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(
            doc! {
                "resource_type": resource_type,
                "resource_id": { "$in": resource_ids },
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

    /// 空资源集合直接返回空且不访问数据库。
    #[tokio::test]
    async fn separation_facts_empty_input_returns_empty_without_db() {
        use entities::AuditLog;
        let client = mongodb::Client::with_uri_str("mongodb://127.0.0.1:1")
            .await
            .expect("客户端句柄创建失败");
        let database = client.database("unused");
        let repository = super::Repository::new(&database, "audit_logs");
        let repository: super::Repository<'_, AuditLog> = repository;
        let facts = repository
            .list_separation_facts_by_resources(&[], &mut crate::NoTransaction)
            .await
            .expect("空输入批量查询必须成功");
        assert!(facts.is_empty());
    }
}
