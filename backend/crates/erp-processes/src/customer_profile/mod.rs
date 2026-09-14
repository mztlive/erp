//! 客户资料根级命令与对象中心查询。
//!
//! 页面只通过本服务维护 Party 身份、客户角色、归属首行及当前从属事实；
//! 创建和修订把全部写入、审计与幂等结果放在同一 MongoDB 事务中。

mod create;
mod facts;
mod idempotency;
mod numbering;
mod query;
mod sensitive;
mod update;
mod views;
use std::sync::Arc;

use erp_identity::SharedRbacService;
use mongodb::Database;

use crate::{Error, Result};
use erp_party::SensitiveDataCodec;

/// 完整客户资料的根级服务。
pub struct CustomerProfileService {
    db: Database,
    sensitive_data: Arc<SensitiveDataCodec>,
    rbac: Option<SharedRbacService>,
}

impl CustomerProfileService {
    /// 创建客户资料根级服务。
    ///
    /// # 参数
    /// * `db` - MongoDB 数据库实例
    /// * `sensitive_data` - 启动期固定的敏感数据编解码器
    ///
    /// # 返回
    /// 返回可执行客户资料用例的服务实例。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 写入前必须注入 RBAC，不得在缺授权源时按登录人推断范围。
    pub fn new(db: Database, sensitive_data: Arc<SensitiveDataCodec>) -> Self {
        Self {
            db,
            sensitive_data,
            rbac: None,
        }
    }

    /// 注入当前 RBAC 快照，供资料写入在事务内重验 DataScope。
    ///
    /// # 参数
    /// * `rbac` - 共享 RBAC 服务
    ///
    /// # 返回
    /// 返回可在同一写入事务证明客户范围的资料服务。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// handler 事前检查不能代替事务内 `require_create` / `require_with`。
    pub fn with_rbac(mut self, rbac: SharedRbacService) -> Self {
        self.rbac = Some(rbac);
        self
    }

    /// 取得资料写入所需的授权源。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回已注入的 RBAC 服务。
    ///
    /// # 错误
    /// 未注入时返回内部错误。
    ///
    /// # 关键业务约束
    /// 缺少授权源不得写入客户资料。
    pub(super) fn require_rbac(&self) -> Result<&SharedRbacService> {
        self.rbac
            .as_ref()
            .ok_or_else(|| Error::Internal("客户资料写入需要授权源".into()))
    }
}

#[cfg(test)]
use erp_customer::CustomerAccountStatus;
#[cfg(test)]
use idempotency::checked_command_view;
#[cfg(test)]
use query::customer_status_blockers;

#[cfg(test)]
mod tests {
    use erp_core::common::time::BusinessDate;
    use erp_customer::{
        CustomerProfileCommand, CustomerProfileCommandData, CustomerProfileOperation,
        CustomerProfileReplayContext, CustomerProfileRequestFingerprint,
    };

    use crate::Error;

    use super::{checked_command_view, customer_status_blockers, CustomerAccountStatus};

    #[test]
    fn disabled_customer_blocks_new_business_actions() {
        assert!(customer_status_blockers(CustomerAccountStatus::Active).is_empty());
        let blockers = customer_status_blockers(CustomerAccountStatus::Disabled);
        assert_eq!(blockers.len(), 2);
        assert!(blockers.iter().all(|item| item.code == "CUSTOMER_DISABLED"));
    }

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
        assert!(matches!(
            checked_command_view(command, &mismatching),
            Err(Error::ConflictError(_))
        ));
    }
}
