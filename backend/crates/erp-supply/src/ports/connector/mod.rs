//! 已确认的供应商协议适配合同；尚未装配运行。
//!
//! 每个实例绑定一个已授权连接；只转换协议和返回外部事实，不访问 ERP 仓储。
//! Process 持久化动作后在事务外调用，每次写调用至多发送一次；超时或取消后
//! 结果不明必须调查原动作，不在适配器中隐藏重试、拆单或支付确认。
//! 能力由窄 trait 的可选引用声明；连接和商品的业务启用资格仍由现有领域校验。
//! 具体执行、来源时效和与既有网关的衔接见本目录 README.md。

pub mod callback;
pub mod common;
pub mod offer;
pub mod order;
pub mod settlement;

use callback::Callbacks;
use erp_core::ids::SupplierApiConnectionId;
use offer::{AvailabilitySource, DeliverySource, OfferSource, ServiceAreas};
use order::{Cancellations, Orders, PaymentConfirmation, Refunds};
use settlement::Settlements;

/// 已绑定连接的供应商适配器；所有可选能力默认缺失，禁止伪造成功。
pub trait SupplierConnector: Send + Sync {
    /// 返回装配时绑定的连接身份，所有返回的外部标识均在此连接内解释。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 已验证且不可由请求载荷覆盖的连接 ID。
    /// # 错误
    /// 不返回错误。
    fn connection_id(&self) -> &SupplierApiConnectionId;

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
        assert!(port.offers().is_none());
        assert!(port.availability().is_none());
        assert!(port.service_areas().is_none());
        assert!(port.delivery().is_none());
        assert!(port.orders().is_none());
        assert!(port.payment_confirmation().is_none());
        assert!(port.cancellations().is_none());
        assert!(port.refunds().is_none());
        assert!(port.callbacks().is_none());
        assert!(port.settlements().is_none());
    }
}
