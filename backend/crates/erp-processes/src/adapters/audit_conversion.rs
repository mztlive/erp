//! 各消费领域预制审计与审计实体的组合层转换。
//!
//! 领域保持独立的 Port 和快照类型；只在本模块共享字段搬运及同构错误映射。

use std::num::NonZeroU32;

use entity_core::BaseModel;
use erp_audit::{AuditLog, AuditLogData, Result as AuditResult, prepare_business_log};
use erp_catalog::PreparedCatalogAudit;
use erp_contract::PreparedContractAudit;
use erp_customer::PreparedCustomerAudit;
use erp_identity::PreparedResourceAudit;
use erp_inventory::PreparedInventoryAudit;
use erp_party::PreparedPartyAudit;
use erp_support::PreparedSupportAudit;
use erp_warehouse::PreparedWarehouseAudit;

/// 保留原事件身份、元数据与安全关联的领域审计快照转换。
pub(super) trait PreparedAudit: Sized {
    /// 从已构造的审计实体复制消费领域需要的快照。
    ///
    /// # 参数
    /// * `log` - 已构造的审计实体，包含原始事件元数据。
    ///
    /// # 返回
    /// 返回领域快照；历史事件缺少序号时使用首个事件序号。
    ///
    /// # 错误
    /// 不返回错误；还原时重新验证快照中的安全字段。
    fn from_log(log: &AuditLog) -> Self;

    /// 使用原始元数据还原并校验安全业务审计。
    ///
    /// # 参数
    /// * `self` - 待持久化的消费领域快照。
    ///
    /// # 返回
    /// 返回沿用原始 ID、版本、时间戳及事件关联的审计实体。
    ///
    /// # 错误
    /// 审计字段、操作人名称或请求编号非法时返回审计领域错误。
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

impl_prepared_audit!(
    PreparedCatalogAudit,
    PreparedContractAudit,
    PreparedCustomerAudit,
    PreparedResourceAudit,
    PreparedInventoryAudit,
    PreparedPartyAudit,
    PreparedSupportAudit,
    PreparedWarehouseAudit,
);

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
        /// 将审计错误原样映射为消费领域对应变体。
        ///
        /// # 参数
        /// * `error` - 审计领域错误。
        ///
        /// # 返回
        /// 返回保留原始载荷和错误分类的消费领域错误。
        ///
        /// # 错误
        /// 不返回错误。
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

#[cfg(test)]
mod tests {
    use application_core::{AuditActor, ValidationIssues};
    use erp_audit::{AuditActorLogs, Error as AuditError};
    use erp_core::AccountKind;
    use persistence_core::Error as RepositoryError;

    use super::*;

    fn original_log() -> AuditLog {
        let mut log = AuditActor::new("actor-original".into(), "sales-original".into(), AccountKind::Admin)
            .with_actor_name_snapshot(Some("发生时名称".into()))
            .unwrap()
            .with_request_id(Some("request-original".into()))
            .unwrap()
            .resource_log_with_id(
                "audit-original".into(),
                "product.update",
                "product",
                "product-1".into(),
                None,
            )
            .unwrap()
            .with_event_sequence(7)
            .unwrap();
        log.base = BaseModel {
            id: "audit-original".into(),
            version: 7,
            created_at: 11,
            updated_at: 19,
            deleted_at: 23,
        };
        log.structured_event.as_mut().unwrap().occurred_at = log.base.created_at;
        prepare_business_log(&log).unwrap()
    }

    /// 每个实际消费类型均执行生产转换与共享上下文方法。
    macro_rules! check_context_round_trip {
        ($($prepared:ty),+ $(,)?) => {
            $(
                let original = original_log();
                let prepared = <$prepared>::from_log(&original);
                let context = prepared.audit_context();
                assert_eq!(context.actor_id, "actor-original");
                assert_eq!(context.actor_account, "sales-original");
                assert_eq!(context.actor_type, AccountKind::Admin);
                assert_eq!(context.actor_name_snapshot.as_deref(), Some("发生时名称"));
                assert_eq!(context.request_id.as_deref(), Some("request-original"));
                assert_eq!(context.event_sequence.get(), 7);
                assert_eq!(prepared.to_log().unwrap(), original);

                let unsafe_name = prepared.clone().with_actor_name_snapshot(Some("unsafe\nname".into()));
                assert!(unsafe_name.to_log().is_err());
                let unsafe_request = prepared.with_request_id(Some("unsafe\nrequest".into()));
                assert!(unsafe_request.to_log().is_err());
            )+
        };
    }

    #[test]
    fn shared_context_preserves_all_consumer_snapshots_and_adapter_validation() {
        check_context_round_trip!(
            PreparedCatalogAudit,
            PreparedContractAudit,
            PreparedCustomerAudit,
            PreparedResourceAudit,
            PreparedInventoryAudit,
            PreparedPartyAudit,
            PreparedSupportAudit,
            PreparedWarehouseAudit,
        );
    }

    /// 执行生产映射宏，确认每个消费领域均保留关联恢复所需的具体错误。
    macro_rules! check_error_mapping {
        ($domain:ident) => {{
            use $domain::Error as DomainError;

            map_audit_error!(fn convert -> $domain);

            assert!(matches!(
                convert(AuditError::Internal("payload".into())),
                DomainError::Internal(value) if value == "payload"
            ));
            assert!(matches!(
                convert(AuditError::NotFound("payload".into())),
                DomainError::NotFound(value) if value == "payload"
            ));
            assert!(matches!(
                convert(AuditError::ValidationError("payload".into())),
                DomainError::ValidationError(value) if value == "payload"
            ));
            assert!(matches!(
                convert(AuditError::BusinessLogicError("payload".into())),
                DomainError::BusinessLogicError(value) if value == "payload"
            ));
            assert!(matches!(
                convert(AuditError::ConflictError("payload".into())),
                DomainError::ConflictError(value) if value == "payload"
            ));
            assert!(matches!(
                convert(AuditError::Forbidden("payload".into())),
                DomainError::Forbidden(value) if value == "payload"
            ));
            assert!(matches!(
                convert(AuditError::Unauthenticated("payload".into())),
                DomainError::Unauthenticated(value) if value == "payload"
            ));
            let issues = ValidationIssues::field("contacts[2].name", "姓名不能为空");
            let DomainError::InvalidInput(mapped) = convert(AuditError::InvalidInput(issues.clone())) else {
                panic!("field issues must remain typed");
            };
            assert_eq!(mapped, issues);
            assert!(matches!(
                convert(AuditError::ReceiptDuplicate(RepositoryError::OptimisticLockingError)),
                DomainError::ReceiptDuplicate(RepositoryError::OptimisticLockingError)
            ));
            assert!(matches!(
                convert(AuditError::TransientTransaction(RepositoryError::OptimisticLockingError)),
                DomainError::TransientTransaction(RepositoryError::OptimisticLockingError)
            ));
            assert!(matches!(
                convert(AuditError::OutcomeUnknown(RepositoryError::OptimisticLockingError)),
                DomainError::OutcomeUnknown(RepositoryError::OptimisticLockingError)
            ));
            assert!(matches!(
                convert(AuditError::RepositoryError(RepositoryError::OptimisticLockingError)),
                DomainError::RepositoryError(RepositoryError::OptimisticLockingError)
            ));
            let DomainError::Logic(mapped) = convert(AuditError::Logic("logic".into())) else {
                panic!("core error must remain typed");
            };
            assert_eq!(mapped.to_string(), "logic");
        }};
    }

    #[test]
    fn all_consumers_preserve_error_payloads_and_recovery_variants() {
        check_error_mapping!(erp_catalog);
        check_error_mapping!(erp_contract);
        check_error_mapping!(erp_customer);
        check_error_mapping!(erp_identity);
        check_error_mapping!(erp_party);
        check_error_mapping!(erp_support);
        check_error_mapping!(erp_warehouse);
    }
}
