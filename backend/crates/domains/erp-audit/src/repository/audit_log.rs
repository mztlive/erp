//! 审计存储仅提供有授权的展示查询与正常日志写入。
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{Document, doc};
use persistence_core::{
    Executor, PageResult, Pagination, QueryFilter, Repository, Result, insert_literal_regex_filter, mongo_ops,
};

use crate::entity::{AuditLog, BusinessEventResult};

/// 展示查询与同事务写入，不暴露命令或业务身份事实。
#[allow(async_fn_in_trait)]
pub trait AuditLogRepositoryExt {
    /// 用调用方的执行器按业务原顺序批量追加日志，空批次直接成功；非空批次写入失败时透传仓储错误。
    async fn create_many_ordered(&self, logs: &[AuditLog], executor: &mut dyn Executor) -> Result<()>;
    /// 在同一执行器上按已授权的条件读取日志分页与总数，列表按创建时间倒序；查询或计数失败时返回仓储错误。
    async fn search_logs(
        &self,
        filter: &AuditLogFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<AuditLog>>;
}
impl AuditLogRepositoryExt for Repository<'_, AuditLog> {
    async fn create_many_ordered(&self, logs: &[AuditLog], executor: &mut dyn Executor) -> Result<()> {
        if !logs.is_empty() {
            mongo_ops::insert_many(&self.collection(), logs, executor).await?;
        }
        Ok(())
    }
    async fn search_logs(
        &self,
        filter: &AuditLogFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<AuditLog>> {
        self.search(filter, executor).await
    }
}

/// 审计日志列表过滤条件。
#[derive(Debug, Clone)]
pub struct AuditLogFilter {
    pub actor_account: Option<String>,
    pub action: Option<String>,
    pub resource_type: Option<String>,
    pub success: Option<bool>,
    pub event_result: Option<BusinessEventResult>,
    pub resource_number: Option<String>,
    pub page: u64,
    pub page_size: u32,
}

impl Default for AuditLogFilter {
    // 为没有指定条件的日志列表提供第 `1` 页、每页 `20` 条的空筛选。
    fn default() -> Self {
        Self {
            actor_account: None,
            action: None,
            resource_type: None,
            success: None,
            event_result: None,
            resource_number: None,
            page: 1,
            page_size: 20,
        }
    }
}

impl QueryFilter for AuditLogFilter {
    // 生成未删除日志的 MongoDB 条件：操作人账号、动作和业务编号使用字面量正则，资源类型和成功标记精确匹配。事件结果映射为 `succeeded`、`rejected` 或 `unknown`，分页不进入谓词。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };

        insert_literal_regex_filter(&mut filter, "actor_account", self.actor_account.as_deref());
        insert_literal_regex_filter(&mut filter, "action", self.action.as_deref());
        insert_literal_regex_filter(
            &mut filter,
            "structured_event.resource_number_snapshot",
            self.resource_number.as_deref(),
        );

        if let Some(result) = self.event_result {
            let code = match result {
                BusinessEventResult::Succeeded => "succeeded",
                BusinessEventResult::Rejected => "rejected",
                BusinessEventResult::Unknown => "unknown",
            };
            filter.insert("structured_event.result", code);
        }

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
    // 把调用方保存的页码和条数交给仓储，条数转为 `u64`；此处不把 `0` 改为第一页。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use application_core::AuditActor;
    use erp_core::AccountKind;
    use erp_core::money::{Amount, Quantity};
    use mongodb::bson::{deserialize_from_slice, serialize_to_vec};
    use persistence_core::{Pagination, QueryFilter};

    use super::AuditLogFilter;
    use crate::entity::{
        AuditAction, AuditFact, AuditField, AuditFieldChange, AuditFieldKind, AuditLog, AuditValue,
        BusinessEventContent, BusinessEventContext, BusinessEventResult,
    };

    #[test]
    fn audit_log_filter_default_starts_at_page_one_size_twenty() {
        let filter = AuditLogFilter::default();
        assert_eq!(filter.page, 1);
        assert_eq!(filter.page_size, 20);
        assert_eq!(filter.actor_account, None);
        assert_eq!(filter.success, None);
        assert_eq!(filter.page_and_size(), (1, 20));
    }

    #[test]
    fn action_filter_treats_regex_metacharacters_as_literal_text() {
        let filter =
            AuditLogFilter { action: Some("audit.create+".to_string()), ..Default::default() }.to_doc();

        assert_eq!(filter.get_document("action").unwrap().get_str("$regex").unwrap(), r"audit\.create\+");
    }

    #[test]
    fn structured_filter_preserves_unknown_result_and_literal_business_number() {
        let filter = AuditLogFilter {
            event_result: Some(BusinessEventResult::Unknown),
            resource_number: Some("SF+1".to_string()),
            ..Default::default()
        }
        .to_doc();
        assert_eq!(filter.get_str("structured_event.result").unwrap(), "unknown");
        assert_eq!(
            filter
                .get_document("structured_event.resource_number_snapshot")
                .unwrap()
                .get_str("$regex")
                .unwrap(),
            r"SF\+1",
        );
    }

    #[test]
    fn structured_decimal_values_round_trip_in_bson_without_database() {
        const ACTION: AuditAction = AuditAction {
            code: "service_fulfillment.confirm",
            resource_type: "service_fulfillment",
            label: "确认服务履约",
            version: 1,
            allowed_fields: &[
                AuditField { code: "quantity", label: "服务数量", kind: AuditFieldKind::Quantity },
                AuditField { code: "amount", label: "确认金额", kind: AuditFieldKind::Amount },
            ],
        };
        let actor = AuditActor::new("actor-1".to_string(), "sales".to_string(), AccountKind::Admin);
        let context = BusinessEventContext::new(actor, ACTION)
            .unwrap()
            .with_actor_name_snapshot(Some("周晓彤".to_string()))
            .unwrap()
            .with_command_id(Some("command-1".to_string()))
            .unwrap()
            .with_request_id(Some("request-1".to_string()))
            .unwrap();
        let log = context
            .log(BusinessEventContent {
                target_id: "service-1".to_string(),
                target_number: Some("FW202610040001".to_string()),
                result: BusinessEventResult::Succeeded,
                field_changes: vec![AuditFieldChange {
                    field: "quantity".to_string(),
                    before: AuditValue::Quantity { value: Quantity::from_str("0.000001").unwrap() },
                    after: AuditValue::Quantity { value: Quantity::from_str("1.234567").unwrap() },
                }],
                facts: vec![AuditFact {
                    field: "amount".to_string(),
                    value: AuditValue::Amount { value: Amount::from_str("12.30").unwrap() },
                }],
            })
            .unwrap();

        let bytes = serialize_to_vec(&log).unwrap();
        let restored: AuditLog = deserialize_from_slice(&bytes).unwrap();
        assert_eq!(restored, log);
    }
}
