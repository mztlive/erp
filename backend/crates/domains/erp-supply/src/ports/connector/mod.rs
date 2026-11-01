//! 供应商协议适配合同；具体适配器由组合层装配。
//!
//! 每个实例绑定一个已授权连接；只转换协议和返回外部事实，不访问 ERP 仓储。
//! Process 持久化动作后在事务外调用，每次写调用至多发送一次；超时或取消后
//! 结果不明必须调查原动作，不在适配器中隐藏重试、拆单或支付确认。
//! 能力由窄 trait 的可选引用声明；连接和商品的业务启用资格仍由现有领域校验。
//! 具体执行、来源时效和与既有网关的衔接见本目录 README.md。

pub mod address;
pub mod callback;
pub mod common;
pub mod description;
pub mod funds;
pub mod messages;
pub mod offer;
pub mod order;
pub mod order_tree;
mod order_tree_tracking;
pub mod preflight;
pub mod receipt;
pub mod runtime;
pub mod settlement;
pub mod transform;

use address::{AddressRequirement, SupplierAddressSource};
use callback::Callbacks;
use erp_core::ids::SupplierApiConnectionId;
use funds::{OrderFundingRequirement, SupplierFundsSource};
use messages::SupplierMessageSource;
use offer::{AvailabilitySource, DeliverySource, OfferSource, ServiceAreas};
use order::{Cancellations, OrderConfirmation, Orders, PaymentConfirmation, Refunds};
use preflight::OrderPreflight;
use receipt::OrderReceiptConfirmation;
use settlement::Settlements;

use crate::entity::supplier_api::SupplierApiCapabilityCode;

/// 已绑定连接的供应商适配器；所有可选能力默认缺失，禁止伪造成功。
pub trait SupplierConnector: Send + Sync {
    /// 返回实际收到商品之后的独立供应商通知能力。
    /// # 参数
    /// 无；能力由已绑定连接的受控运行时提供。
    /// # 返回
    /// 已实现能力或 None；不能以预占确认、付款通知或物流妥投替代。
    /// # 错误
    /// 不返回错误，缺失能力不发送请求。
    fn receipt_confirmation(&self) -> Option<&dyn OrderReceiptConfirmation> {
        None
    }

    /// 从实际提供的窄接口生成只读能力，不读取连接上的历史能力配置。
    /// # 参数
    /// 无。
    /// # 返回
    /// 当前适配器实际支持的操作。
    /// # 错误
    /// 不返回错误。
    fn capabilities(&self) -> Vec<SupplierApiCapabilityCode> {
        use SupplierApiCapabilityCode as Code;
        [
            (self.offers().is_some(), Code::Product),
            (self.offers().is_some(), Code::Price),
            (self.availability().is_some(), Code::Stock),
            (self.service_areas().is_some(), Code::ServiceAreas),
            (self.delivery().is_some(), Code::Delivery),
            (self.payment_confirmation().is_some(), Code::PaymentConfirmation),
            (self.order_confirmation().is_some(), Code::OrderConfirmation),
            (self.receipt_confirmation().is_some(), Code::ReceiptConfirmation),
            (self.preflight().is_some(), Code::OrderPreflight),
            (self.orders().is_some_and(|orders| orders.supports_creation()), Code::Order),
            (self.orders().is_some(), Code::Query),
            (self.cancellations().is_some(), Code::Cancel),
            (self.refunds().is_some(), Code::Refund),
            (self.callbacks().is_some(), Code::Callback),
            (self.settlements().is_some(), Code::Settlement),
        ]
        .into_iter()
        .filter_map(|(supported, code)| supported.then_some(code))
        .collect()
    }

    /// 返回装配时绑定的连接身份，所有返回的外部标识均在此连接内解释。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 已验证且不可由请求载荷覆盖的连接 ID。
    /// # 错误
    /// 不返回错误。
    fn connection_id(&self) -> &SupplierApiConnectionId;

    /// 声明实际协议需要的收货地址资料。
    /// # 参数
    /// 无。
    /// # 返回
    /// 默认只要求文本；坐标和四级地址须由适配器明确声明。
    /// # 错误
    /// 不返回错误。
    fn address_requirement(&self) -> AddressRequirement {
        AddressRequirement::Text
    }

    /// 返回供应商原生地址候选及已确认地址有效性读取能力。
    /// # 参数
    /// 无。
    /// # 返回
    /// 已实现能力；缺失时禁止用自由填写外部编码替代候选核验。
    /// # 错误
    /// 不返回错误。
    fn address_source(&self) -> Option<&dyn SupplierAddressSource> {
        None
    }

    /// 声明本协议发单所需的资金保障。
    /// # 参数
    /// 无。
    /// # 返回
    /// 默认沿用普通付款门槛；预存款要求不得被远端余额单独满足。
    /// # 错误
    /// 不返回错误。
    fn funding_requirement(&self) -> OrderFundingRequirement {
        OrderFundingRequirement::PostedPaymentGate
    }

    /// 返回供应商资金余额及稳定交易证据的只读查询能力。
    /// # 参数
    /// 无。
    /// # 返回
    /// 未实现时为 None；证据查询不直接产生充值、付款或返款事实。
    /// # 错误
    /// 不返回错误。
    fn funds_source(&self) -> Option<&dyn SupplierFundsSource> {
        None
    }

    /// 返回 G0 已核实的供应商外部资金账号，技术命名空间不能代替资金账号。
    /// # 参数
    /// 无。
    /// # 返回
    /// 已验收执行资料中的资金账号；未知或无预存款协议时为 None。
    /// # 错误
    /// 不返回错误，缺失时资金编排须失败关闭。
    fn funds_account_reference(&self) -> Option<&str> {
        None
    }

    /// 返回商品及报价读取能力。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 已实现的能力；`None` 表示不支持。
    /// # 错误
    /// 不返回错误。
    fn offers(&self) -> Option<&dyn OfferSource> {
        None
    }

    /// 返回可供事实查询能力。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 已实现的能力；`None` 表示不支持。
    /// # 错误
    /// 不返回错误。
    fn availability(&self) -> Option<&dyn AvailabilitySource> {
        None
    }

    /// 返回完整可售地区读取能力，独立于单地址配送校验。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 已实现的能力；`None` 表示不支持。
    /// # 错误
    /// 不返回错误。
    fn service_areas(&self) -> Option<&dyn ServiceAreas> {
        None
    }

    /// 返回地址、运费、时段及拆单分组预检能力。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 已实现的能力；`None` 表示不支持。
    /// # 错误
    /// 不返回错误。
    fn delivery(&self) -> Option<&dyn DeliverySource> {
        None
    }

    /// 返回订单创建和恢复查询能力。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 已实现的能力；`None` 表示不支持。
    /// # 错误
    /// 不返回错误。
    fn orders(&self) -> Option<&dyn Orders> {
        None
    }

    /// 返回按实际数量、地址和批准金额核验的下单预检能力。
    /// # 参数
    /// 无。
    /// # 返回
    /// 未实现时为 None，不由简单库存查询冒充。
    /// # 错误
    /// 不返回错误。
    fn preflight(&self) -> Option<&dyn OrderPreflight> {
        None
    }

    /// 返回独立的预占订单确认能力。
    /// # 参数
    /// 无。
    /// # 返回
    /// 未实现时为 None；与支付结果通知分别声明。
    /// # 错误
    /// 不返回错误。
    fn order_confirmation(&self) -> Option<&dyn OrderConfirmation> {
        None
    }

    /// 返回独立的供应商支付结果确认能力，不执行商城扣款。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 已实现的能力；`None` 表示不支持。
    /// # 错误
    /// 不返回错误。
    fn payment_confirmation(&self) -> Option<&dyn PaymentConfirmation> {
        None
    }

    /// 返回主动取消能力；能收到取消通知不代表能主动取消。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 已实现的能力；`None` 表示不支持。
    /// # 错误
    /// 不返回错误。
    fn cancellations(&self) -> Option<&dyn Cancellations> {
        None
    }

    /// 返回主动退款及按原请求查询退款结果的能力。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 已实现的能力；`None` 表示不支持。
    /// # 错误
    /// 不返回错误。
    fn refunds(&self) -> Option<&dyn Refunds> {
        None
    }

    /// 返回回调校验与标准通知解析能力。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 已实现的能力；`None` 表示不支持。
    /// # 错误
    /// 不返回错误。
    fn callbacks(&self) -> Option<&dyn Callbacks> {
        None
    }

    /// 返回账号级拉取消息能力，不将其解释为已验签主动回调。
    /// # 参数
    /// 无。
    /// # 返回
    /// 未实现时为 None。
    /// # 错误
    /// 不返回错误。
    fn messages(&self) -> Option<&dyn SupplierMessageSource> {
        None
    }

    /// 返回结算来源读取能力，不直接形成应付。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 已实现的能力；`None` 表示不支持。
    /// # 错误
    /// 不返回错误。
    fn settlements(&self) -> Option<&dyn Settlements> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Unconfigured(SupplierApiConnectionId);

    impl SupplierConnector for Unconfigured {
        fn connection_id(&self) -> &SupplierApiConnectionId {
            &self.0
        }
    }

    #[test]
    fn unconfigured_connector_exposes_no_capability() {
        let connector = Unconfigured(SupplierApiConnectionId::new("review-only"));
        let port: &dyn SupplierConnector = &connector;
        assert_eq!(port.connection_id(), &connector.0);
        assert!(port.capabilities().is_empty());
        assert!(port.offers().is_none());
        assert!(port.availability().is_none());
        assert!(port.service_areas().is_none());
        assert!(port.delivery().is_none());
        assert!(port.orders().is_none());
        assert!(port.order_confirmation().is_none());
        assert!(port.preflight().is_none());
        assert!(port.funds_source().is_none());
        assert!(port.messages().is_none());
        assert_eq!(port.address_requirement(), AddressRequirement::Text);
        assert_eq!(port.funding_requirement(), OrderFundingRequirement::PostedPaymentGate);
        assert!(port.payment_confirmation().is_none());
        assert!(port.cancellations().is_none());
        assert!(port.refunds().is_none());
        assert!(port.callbacks().is_none());
        assert!(port.settlements().is_none());
    }
}
