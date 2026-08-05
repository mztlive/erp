use std::{future::Future, pin::Pin};

use database::{repository::AuditLogFilter, DatabaseExt, Transactional};
use entities::{AuditLog, AuditLogData};
use id_generator::next_id;
use mongodb::{ClientSession, Database};
use validator::Validate;

use crate::{
    errors::{Error, Result},
    owned_task::await_owned,
    Page,
};

use self::dto::NormalizedAuditLogListParams;
pub use self::dto::{AuditLogItem, AuditLogListParams};

mod dto;

/// 已通过 HTTP 鉴权的审计操作人。
///
/// 该类型只携带操作人身份；审计动作、资源类型和目标由具体 Service 决定，
/// 避免协议层伪造或遗漏业务审计语义。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditActor {
    actor_id: String,
    actor_account: String,
    actor_type: entities::AccountKind,
}

impl AuditActor {
    /// 创建审计操作人。
    ///
    /// # 参数
    /// * `actor_id` - 操作人账号 ID
    /// * `actor_account` - 操作人登录账号
    /// * `actor_type` - 操作人账号类型
    ///
    /// # 返回值
    /// 返回只包含鉴权身份的审计操作人。
    pub fn new(actor_id: String, actor_account: String, actor_type: entities::AccountKind) -> Self {
        Self {
            actor_id,
            actor_account,
            actor_type,
        }
    }

    /// 返回操作人账号 ID。
    ///
    /// # 返回值
    /// 返回已认证身份中的账号 ID。
    pub(crate) fn id(&self) -> &str {
        &self.actor_id
    }

    /// 返回操作人账号类型。
    ///
    /// # 返回值
    /// 返回已认证身份中的后台账号类型。
    pub(crate) fn kind(&self) -> entities::AccountKind {
        self.actor_type
    }

    /// 在业务写入前构造并验证成功资源审计日志。
    ///
    /// # 参数
    /// * `action` - Service 确定的动作名
    /// * `resource_type` - Service 确定的资源类型
    /// * `resource_id` - 本次操作目标
    ///
    /// # 返回值
    /// 返回已通过领域校验、可直接持久化的审计日志。
    ///
    /// # 错误
    /// 当操作人或资源审计字段不符合领域约束时返回错误。
    pub(crate) fn resource_log(
        self,
        action: &str,
        resource_type: &str,
        resource_id: String,
    ) -> Result<AuditLog> {
        if resource_id.trim().is_empty() {
            return Err(Error::ValidationError("资源ID不能为空".to_string()));
        }
        AuditLog::new(
            next_id(),
            AuditLogData {
                actor_id: self.actor_id,
                actor_account: self.actor_account,
                actor_type: self.actor_type,
                action: action.to_string(),
                resource_type: resource_type.to_string(),
                resource_id: Some(resource_id),
                success: true,
                message: None,
            },
        )
        .map_err(Into::into)
    }
}

/// 在独立所有权任务中执行业务写入，并在同一事务追加审计日志。
///
/// 审计日志必须在调用本函数前完成领域校验。HTTP 请求被取消时，所有权任务仍会
/// 完成事务收尾；最终只能同时提交业务数据与审计日志，或全部回滚。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `audit` - 已验证的审计日志
/// * `operation` - 使用同一 MongoDB session 的业务写入
///
/// # 返回值
/// 返回业务写入的结果。
///
/// # 错误
/// 当业务写入、审计写入、事务提交或所有权任务失败时返回错误。
pub(crate) async fn run_audited_transaction<T, F>(db: Database, audit: AuditLog, operation: F) -> Result<T>
where
    T: Send + 'static,
    F: for<'a> FnOnce(&'a mut ClientSession) -> Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>
        + Send
        + 'static,
{
    let client = db.client().clone();
    await_owned("审计事务", async move {
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let value = operation(session).await?;
                    db.audit_logs().create_with_session(&audit, session).await?;
                    Ok(value)
                })
            })
            .await
    })
    .await
}

/// 审计日志服务
///
/// 提供审计日志的写入与查询能力。
pub struct AuditLogService {
    db: Database,
}

impl AuditLogService {
    /// 创建审计日志服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回值
    /// 返回审计日志服务实例
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 写入审计日志。
    ///
    /// # 参数
    /// * `data` - 审计日志数据
    ///
    /// # 返回值
    /// 返回写入后的审计日志实体
    pub async fn create(&self, data: AuditLogData) -> Result<AuditLog> {
        let id = next_id();
        let log = AuditLog::new(id, data)?;
        self.db.audit_logs().create(&log).await?;
        Ok(log)
    }

    /// 获取审计日志列表。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回值
    /// 返回分页后的审计日志集合
    pub async fn audit_log_list(&self, params: &AuditLogListParams) -> Result<Page<AuditLogItem>> {
        params.validate()?;
        let NormalizedAuditLogListParams {
            actor_account,
            action,
            resource_type,
            success,
            page,
            page_size,
        } = params.normalized();
        let filter = AuditLogFilter {
            actor_account,
            action,
            resource_type,
            success,
            page,
            page_size,
        };
        let page = self.db.audit_logs().search_logs(&filter).await?;
        let items = page.items.into_iter().map(Into::into).collect();
        Ok(Page::new(items, page.total))
    }
}

#[cfg(test)]
mod tests {
    use entities::AccountKind;

    use super::AuditActor;

    #[test]
    fn audit_actor_builds_valid_success_resource_log() {
        let log = AuditActor::new("admin-1".to_string(), "root".to_string(), AccountKind::Admin)
            .resource_log("consumer.create", "consumer", "consumer-1".to_string())
            .unwrap();

        assert_eq!(log.actor_id, "admin-1");
        assert_eq!(log.actor_account, "root");
        assert_eq!(log.actor_type, AccountKind::Admin);
        assert_eq!(log.action, "consumer.create");
        assert_eq!(log.resource_type, "consumer");
        assert_eq!(log.resource_id.as_deref(), Some("consumer-1"));
        assert!(log.success);
        assert!(log.message.is_none());
    }

    #[test]
    fn audit_actor_validates_before_transaction() {
        let result = AuditActor::new("admin-1".to_string(), "root".to_string(), AccountKind::Admin)
            .resource_log("", "consumer", "consumer-1".to_string());

        assert!(result.is_err());
    }

    #[test]
    fn audit_actor_rejects_empty_resource_id() {
        let result = AuditActor::new("admin-1".to_string(), "root".to_string(), AccountKind::Admin)
            .resource_log("consumer.create", "consumer", "  ".to_string());

        assert!(result.is_err());
    }
}
