//! 从实际窄接口生成无凭据的只读能力描述；不表达连接授权或健康状态。

use serde::Serialize;

use super::SupplierConnector;
use super::address::AddressRequirement;
use super::funds::OrderFundingRequirement;
use crate::entity::supplier_api::SupplierApiCapabilityCode;

/// 外部通知的实际接收方式，推送和拉取不能互相冒充。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationMode {
    Unavailable,
    Callback,
    Pull,
    CallbackAndPull,
}

/// 连接器公开的完整能力描述，不含账号、地址票据、资金账号及凭据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConnectorDescription {
    pub operations: Vec<SupplierApiCapabilityCode>,
    pub address_requirement: AddressRequirement,
    pub address_candidates: bool,
    pub notification_mode: NotificationMode,
    pub funding_requirement: OrderFundingRequirement,
    pub funds_source: bool,
}

impl ConnectorDescription {
    /// 读取已装配接口生成描述，不使用历史能力配置或发起外部调用。
    /// # 参数
    /// `connector` 为按当前部署构造的连接器。
    /// # 返回
    /// 操作、地址、消息与资金能力的实际声明。
    /// # 错误
    /// 不返回错误，缺失的接口保持缺失。
    pub fn from_connector<C: SupplierConnector + ?Sized>(connector: &C) -> Self {
        let notification_mode = match (connector.callbacks().is_some(), connector.messages().is_some()) {
            (false, false) => NotificationMode::Unavailable,
            (true, false) => NotificationMode::Callback,
            (false, true) => NotificationMode::Pull,
            (true, true) => NotificationMode::CallbackAndPull,
        };
        Self {
            operations: connector.capabilities(),
            address_requirement: connector.address_requirement(),
            address_candidates: connector.address_source().is_some(),
            notification_mode,
            funding_requirement: connector.funding_requirement(),
            funds_source: connector.funds_source().is_some(),
        }
    }
}

#[cfg(test)]
mod tests {
    use erp_core::ids::SupplierApiConnectionId;

    use super::*;

    struct Unsupported(SupplierApiConnectionId);
    impl SupplierConnector for Unsupported {
        fn connection_id(&self) -> &SupplierApiConnectionId {
            &self.0
        }
        fn address_requirement(&self) -> AddressRequirement {
            AddressRequirement::FourLevel
        }
        fn funding_requirement(&self) -> OrderFundingRequirement {
            OrderFundingRequirement::VerifiedSupplierDeposit
        }
    }

    #[test]
    fn requirements_do_not_invent_available_operations_or_evidence() {
        let description = ConnectorDescription::from_connector(&Unsupported(SupplierApiConnectionId::new(
            "private-connection",
        )));
        assert!(description.operations.is_empty());
        assert_eq!(description.address_requirement, AddressRequirement::FourLevel);
        assert!(!description.address_candidates);
        assert_eq!(description.notification_mode, NotificationMode::Unavailable);
        assert_eq!(description.funding_requirement, OrderFundingRequirement::VerifiedSupplierDeposit);
        assert!(!description.funds_source);

        let value = serde_json::to_value(&description).unwrap();
        assert_eq!(value["funding_requirement"], "verified_supplier_deposit");
        assert_eq!(value["address_requirement"]["type"], "four_level");
        assert!(!value.to_string().contains("private-connection"));
    }
}
