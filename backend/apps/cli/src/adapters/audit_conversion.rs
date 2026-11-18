//! 各消费领域预制审计与审计实体的组合层转换。
//!
//! 领域保持独立的 Port 和快照类型；只在本模块共享字段搬运及同构错误映射。

use std::num::NonZeroU32;

use entity_core::BaseModel;
use erp_audit::{AuditLog, AuditLogData, Result as AuditResult, prepare_business_log};
use erp_identity::PreparedResourceAudit;

/// 保留原事件身份、元数据与安全关联的领域审计快照转换。
pub(super) trait PreparedAudit: Sized {
    // 身份适配器生成资源审计后，把事件身份、原始元数据和关联信息交给身份域暂存。
    // 历史事件没有序号时按第 1 个事件处理。
    fn from_log(log: &AuditLog) -> Self;

    // 写入审计前，从身份域快照还原事件，保留原始 ID、版本、时间戳和关联信息。
    // 重新校验审计字段、操作人名称与请求编号，不合规的事件不落库。
    fn to_log(&self) -> AuditResult<AuditLog>;
}

macro_rules! impl_prepared_audit {
    ($($prepared:ty),+ $(,)?) => {
        $(impl PreparedAudit for $prepared {
            fn from_log(log: &AuditLog) -> Self {
                Self::from_validated(
                    &log.base,
                    log.actor_id.clone(),
                    log.actor_account.clone(),
                    log.actor_type,
                    log.action.clone(),
                    log.resource_type.clone(),
                    log.resource_id.clone(),
                    log.success,
                    log.message.clone(),
                )
                .with_actor_name_snapshot(
                    log.structured_event.as_ref().and_then(|event| event.actor_name_snapshot.clone()),
                )
                .with_request_id(log.structured_event.as_ref().and_then(|event| event.request_id.clone()))
                .with_event_sequence(
                    log.structured_event.as_ref().map(|event| event.event_sequence).unwrap_or(NonZeroU32::MIN),
                )
            }

            fn to_log(&self) -> AuditResult<AuditLog> {
                let context = self.audit_context();
                let mut log = AuditLog::new(
                    self.id.clone(),
                    AuditLogData {
                        actor_id: context.actor_id,
                        actor_account: context.actor_account,
                        actor_type: context.actor_type,
                        action: self.action.clone(),
                        resource_type: self.resource_type.clone(),
                        resource_id: self.resource_id.clone(),
                        success: self.success,
                        message: self.message.clone(),
                    },
                )?;
                log.base = BaseModel {
                    id: self.id.clone(),
                    version: self.version,
                    created_at: self.created_at,
                    updated_at: self.updated_at,
                    deleted_at: self.deleted_at,
                };
                prepare_business_log(&log)?
                    .with_actor_name_snapshot(context.actor_name_snapshot)?
                    .with_request_id(context.request_id)?
                    .with_event_sequence(context.event_sequence.get())
            }
        })+
    };
}

impl_prepared_audit!(PreparedResourceAudit);

/// 生成保留审计错误变体和载荷的消费领域映射。
///
/// # 参数
/// * `$visibility` - 原映射入口的可见性。
/// * `$name` - 原映射函数名。
/// * `$domain` - 消费领域 crate。
///
/// # 返回
/// 展开为一个同构错误映射函数。
///
/// # 错误
/// 不返回错误；特殊领域映射不得使用本宏。
macro_rules! map_audit_error {
    ($visibility:vis fn $name:ident -> $domain:ident) => {
        // 审计构造或写入失败时，将错误转换为消费领域的对应变体，保留分类和原始载荷。
        $visibility fn $name(error: erp_audit::Error) -> $domain::Error {
            match error {
                erp_audit::Error::Internal(payload) => $domain::Error::Internal(payload),
                erp_audit::Error::NotFound(payload) => $domain::Error::NotFound(payload),
                erp_audit::Error::ValidationError(payload) => $domain::Error::ValidationError(payload),
                erp_audit::Error::InvalidInput(payload) => $domain::Error::InvalidInput(payload),
                erp_audit::Error::BusinessLogicError(payload) => $domain::Error::BusinessLogicError(payload),
                erp_audit::Error::ConflictError(payload) => $domain::Error::ConflictError(payload),
                erp_audit::Error::ReceiptDuplicate(payload) => $domain::Error::ReceiptDuplicate(payload),
                erp_audit::Error::TransientTransaction(payload) => $domain::Error::TransientTransaction(payload),
                erp_audit::Error::Forbidden(payload) => $domain::Error::Forbidden(payload),
                erp_audit::Error::Unauthenticated(payload) => $domain::Error::Unauthenticated(payload),
                erp_audit::Error::Logic(payload) => $domain::Error::Logic(payload),
                erp_audit::Error::OutcomeUnknown(payload) => $domain::Error::OutcomeUnknown(payload),
                erp_audit::Error::RepositoryError(payload) => $domain::Error::RepositoryError(payload),
            }
        }
    };
}
pub(super) use map_audit_error;
