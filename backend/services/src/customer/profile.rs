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
mod validation;

use std::sync::Arc;

use mongodb::Database;

use crate::party::SensitiveDataCodec;

/// 完整客户资料的根级服务。
pub struct CustomerProfileService {
    db: Database,
    sensitive_data: Arc<SensitiveDataCodec>,
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
    pub fn new(db: Database, sensitive_data: Arc<SensitiveDataCodec>) -> Self {
        Self { db, sensitive_data }
    }
}

#[cfg(test)]
use entities::customer::CustomerAccountStatus;
#[cfg(test)]
use idempotency::replay_command;
#[cfg(test)]
use query::customer_status_blockers;

#[cfg(test)]
mod tests {
    use entities::{
        common::time::BusinessDate,
        customer::{CustomerProfileCommand, CustomerProfileCommandData},
    };

    use super::{customer_status_blockers, replay_command, CustomerAccountStatus};

    #[test]
    fn disabled_customer_blocks_new_business_actions() {
        assert!(customer_status_blockers(CustomerAccountStatus::Active).is_empty());
        let blockers = customer_status_blockers(CustomerAccountStatus::Disabled);
        assert_eq!(blockers.len(), 2);
        assert!(blockers.iter().all(|item| item.code == "CUSTOMER_DISABLED"));
    }

    #[test]
    fn command_replay_requires_same_operation_customer_and_fingerprint() {
        let command = CustomerProfileCommand::new(
            "command-1",
            CustomerProfileCommandData {
                idempotency_key: "key-1".to_string(),
                operation: "update".to_string(),
                initiated_by: "admin-1".to_string(),
                request_fingerprint: "fingerprint-1".to_string(),
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
        assert!(replay_command(
            command.clone(),
            "update",
            Some("customer-1"),
            "admin-1",
            "fingerprint-1"
        )
        .is_ok());
        assert!(replay_command(command, "update", Some("customer-2"), "admin-1", "fingerprint-1").is_err());
    }
}
