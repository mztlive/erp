//! 客户资料根级跨域命令。
//!
//! 页面只通过本服务维护 Party 身份、客户角色、归属首行及当前从属事实；
//! 创建和修订把全部写入、审计与幂等结果放在同一 MongoDB 事务中。

mod create;
mod facts;
mod field_changes;
mod idempotency;
mod identity;
mod query;
mod sensitive;
mod update;
use std::sync::Arc;

use erp_customer::CustomerAccess;
use erp_identity::AuthorizationService;
use erp_party::SensitiveDataCodec;
use mongodb::Database;

/// 完整客户资料的根级服务。
#[derive(Clone)]
pub struct CustomerProfileProcess {
    db: Database,
    sensitive_data: Arc<SensitiveDataCodec>,
    access: CustomerAccess,
    authorization: Arc<AuthorizationService>,
}

impl CustomerProfileProcess {
    /// 创建具备完整授权能力的客户资料服务。
    ///
    /// db 为业务数据库，sensitive_data 为敏感数据编解码器；access 和 authorization 来自同次共享授权装配。
    ///
    /// 可在原事务中重验客户访问资格的服务。
    ///
    /// 构造不执行 I/O。
    pub fn new(
        db: Database,
        sensitive_data: Arc<SensitiveDataCodec>,
        access: CustomerAccess,
        authorization: Arc<AuthorizationService>,
    ) -> Self {
        Self { db, sensitive_data, access, authorization }
    }
}

#[cfg(test)]
use idempotency::checked_command_view;

#[cfg(test)]
mod tests {
    use erp_core::common::time::BusinessDate;
    use erp_customer::{
        CustomerProfileCommand, CustomerProfileCommandData, CustomerProfileOperation,
        CustomerProfileReplayContext, CustomerProfileRequestFingerprint,
    };

    use super::checked_command_view;
    use crate::Error;

    #[test]
    fn command_replay_mismatch_maps_to_stable_service_conflict() {
        let command = CustomerProfileCommand::new(
            "command-1",
            CustomerProfileCommandData {
                idempotency_key: "key-1".to_string(),
                operation: "update".to_string(),
                initiated_by: "admin-1".to_string(),
                request_fingerprint: "0".repeat(64),
                customer_id: "customer-1".to_string(),
                customer_no: "KH-1".to_string(),
                party_id: "party-1".to_string(),
                revision_id: "revision-2".to_string(),
                revision_no: 2,
                customer_version: 2,
                party_version: 2,
                effective_from: BusinessDate::from_ymd(2026, 8, 8).unwrap(),
                change_reason: "资料修订".to_string(),
            },
        )
        .unwrap();
        let fingerprint = CustomerProfileRequestFingerprint::parse_compatible("0".repeat(64)).unwrap();
        let matching = CustomerProfileReplayContext::new(
            "key-1",
            CustomerProfileOperation::Update,
            Some("customer-1".to_string()),
            "admin-1",
            fingerprint.clone(),
        )
        .unwrap();
        assert!(checked_command_view(command.clone(), &matching).is_ok());

        let mismatching = CustomerProfileReplayContext::new(
            "key-1",
            CustomerProfileOperation::Update,
            Some("customer-2".to_string()),
            "admin-1",
            fingerprint,
        )
        .unwrap();
        assert!(matches!(checked_command_view(command, &mismatching), Err(Error::ConflictError(_))));
    }
}
