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
    /// 取得实际收到商品后可调用的独立通知能力，缺失时为 `None`；不能用预占确认、支付通知或物流妥投代替。
    fn receipt_confirmation(&self) -> Option<&dyn OrderReceiptConfirmation> {
        None
    }

    /// 从当前适配器实际暴露的操作接口枚举能力；查询和创建分别判断，只有订单端口明确支持创建才登记
    /// `Order`。不读取历史配置，也不代替业务启用校验。
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
    /// 已验证且不可由请求载荷覆盖的连接 ID。
    fn connection_id(&self) -> &SupplierApiConnectionId;

    /// 声明实际协议需要的收货地址资料。
    /// 默认只要求文本；坐标和四级地址须由适配器明确声明。
    fn address_requirement(&self) -> AddressRequirement {
        AddressRequirement::Text
    }

    /// 返回供应商原生地址候选及已确认地址有效性读取能力。
    /// 已实现能力；缺失时禁止用自由填写外部编码替代候选核验。
    fn address_source(&self) -> Option<&dyn SupplierAddressSource> {
        None
    }

    /// 声明本协议发单所需的资金保障。
    /// 默认沿用普通付款门槛；预存款要求不得被远端余额单独满足。
    fn funding_requirement(&self) -> OrderFundingRequirement {
        OrderFundingRequirement::PostedPaymentGate
    }

    /// 返回供应商资金余额及稳定交易证据的只读查询能力。
    /// 未实现时为 None；证据查询不直接产生充值、付款或返款事实。
    fn funds_source(&self) -> Option<&dyn SupplierFundsSource> {
        None
    }

    /// 返回 G0 已核实的供应商外部资金账号，技术命名空间不能代替资金账号。
    /// 已验收执行资料中的资金账号；未知或无预存款协议时为 None。
    /// 缺失时资金编排须失败关闭。
    fn funds_account_reference(&self) -> Option<&str> {
        None
    }

    /// 取得读取供应商目录商品和报价的接口；未装配时为 `None`，不能据此启动商品或报价同步。
    fn offers(&self) -> Option<&dyn OfferSource> {
        None
    }

    /// 取得真实可供状态的读取接口，供查库存或观察可供使用；未装配时为 `None`。
    fn availability(&self) -> Option<&dyn AvailabilitySource> {
        None
    }

    /// 取得完整可售地区的读取接口；缺失时为 `None`，不得用单地址配送核验冒充完整范围。
    fn service_areas(&self) -> Option<&dyn ServiceAreas> {
        None
    }

    /// 取得地址、运费、交付时段和拆单分组查询接口，供执行组冻结前核验配送方案；未装配时为 `None`。
    fn delivery(&self) -> Option<&dyn DeliverySource> {
        None
    }

    /// 取得供应商订单读取及创建接口，供下单及原订单调查分别使用。`None` 表示未装配；有该接口仍须另查
    /// `supports_creation` 才能判断是否可创建。
    fn orders(&self) -> Option<&dyn Orders> {
        None
    }

    /// 取得用冻结数量、地址和批准预算只读核验下单条件的接口；不支持时返回 `None`，不可用简单库存查询替代。
    fn preflight(&self) -> Option<&dyn OrderPreflight> {
        None
    }

    /// 取得预占订单创建后可用的独立确认接口；未装配时为 `None`，不改用支付结果通知。
    fn order_confirmation(&self) -> Option<&dyn OrderConfirmation> {
        None
    }

    /// 取得向供应商通知支付结果的独立接口；未支持时为 `None`，不能用来发起商城扣款。
    fn payment_confirmation(&self) -> Option<&dyn PaymentConfirmation> {
        None
    }

    /// 取得主动发起取消并查询原取消动作的接口；未装配为 `None`，能接收取消通知并不证明能主动取消。
    fn cancellations(&self) -> Option<&dyn Cancellations> {
        None
    }

    /// 取得发起单笔退款和查询该动作结果的独立接口；未装配为 `None`，编排不能假定退款已经发生。
    fn refunds(&self) -> Option<&dyn Refunds> {
        None
    }

    /// 取得绑定连接的回调验签与解析接口，供接收原始报文时使用；未装配时为 `None`。
    fn callbacks(&self) -> Option<&dyn Callbacks> {
        None
    }

    /// 返回账号级拉取消息能力，不将其解释为已验签主动回调。
    /// 未实现时为 None。
    fn messages(&self) -> Option<&dyn SupplierMessageSource> {
        None
    }

    /// 返回结算来源读取能力，不直接形成应付。
    /// 已实现的能力；`None` 表示不支持。
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
