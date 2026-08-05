//! `mall_balance_restoration` 与 `mall_balance_restoration_allocation`：卡券余额恢复
//! 事实头与按原 CARD 退款资金分配的恢复分配（数据模型 §6.18）。
//!
//! 只记录余额回补，不冲减消费、供应商成本或应付（§6.18）；商城退款、供应商退款、
//! 卡券余额恢复分别对账。事实不可变，只提供 `new()`。
//!
//! 跨行不变式（§6.18，依赖聚合查询，由 P3 落实，对应事务不变量 §8.4 第 4 条）：
//! - 分配合计等于恢复头金额；
//! - 只能引用净有效的 CARD 退款分配，卡实例必须等于该原支付来源的卡实例；
//! - 每张卡累计恢复金额不得超过对应 CARD 退款净额。
//!
//! 本实体做单行不变式：恢复分配金额非负、序号从 1 起。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{
    MallAfterSalesRequestId, MallBalanceRestorationAllocationId, MallBalanceRestorationId,
    MallCardInstanceId, MallOrderFactId, MallRefundAllocationId, MallRefundId,
};
use crate::money::Amount;
use crate::validation::normalize_required_text;

/// 目标商城代码最大长度。
const MALL_ID_MAX_LEN: usize = 64;
/// 恢复单号最大长度。
const RESTORATION_NO_MAX_LEN: usize = 128;
/// 恢复版本最大长度。
const RESTORATION_VERSION_MAX_LEN: usize = 64;

/// 余额恢复头创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MallBalanceRestorationData {
    /// `CARD_BALANCE_RESTORED` 事实。
    pub mall_order_fact_id: MallOrderFactId,
    /// 同一售后案件。
    pub after_sales_request_id: MallAfterSalesRequestId,
    /// 关联退款。
    pub mall_refund_id: MallRefundId,
    /// 来源商城。
    pub mall_id: String,
    /// 恢复身份。
    pub external_restoration_no: String,
    /// 恢复身份版本。
    pub version: String,
    /// 实际恢复金额。
    pub restored_amount: Amount,
    /// 实际恢复时间。
    pub restored_at: Instant,
}

/// 余额恢复事实头实体（数据模型 §6.18）。
///
/// 事实类型必须为 `CARD_BALANCE_RESTORED`（与所引 `mall_order_fact.fact_type`
/// 一致，跨实体校验由 P3 落实）；`mall_order_fact_id` 非空且唯一由 P2 唯一索引
/// 落实。不可变，只提供 `new()`。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct MallBalanceRestoration {
    #[serde(flatten)]
    pub base: BaseModel,
    /// `CARD_BALANCE_RESTORED` 事实。
    pub mall_order_fact_id: MallOrderFactId,
    /// 同一售后案件。
    pub after_sales_request_id: MallAfterSalesRequestId,
    /// 关联退款。
    pub mall_refund_id: MallRefundId,
    /// 来源商城。
    pub mall_id: String,
    /// 恢复身份。
    pub external_restoration_no: String,
    /// 恢复身份版本。
    pub version: String,
    /// 实际恢复金额。
    pub restored_amount: Amount,
    /// 实际恢复时间。
    pub restored_at: Instant,
}

impl MallBalanceRestoration {
    /// 创建余额恢复事实头。
    ///
    /// 完成文本校验与规范化；`restored_amount` 必须大于零（实际恢复金额）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::MallBalanceRestorationId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的余额恢复头实体。
    ///
    /// # 错误
    /// 当文本为空/超长或恢复金额非正时返回错误。
    pub fn new(id: MallBalanceRestorationId, data: MallBalanceRestorationData) -> Result<Self> {
        let mall_id = normalize_required_text(
            data.mall_id,
            "来源商城不能为空",
            MALL_ID_MAX_LEN,
            "来源商城代码过长",
        )?;
        let external_restoration_no = normalize_required_text(
            data.external_restoration_no,
            "恢复单号不能为空",
            RESTORATION_NO_MAX_LEN,
            "恢复单号过长",
        )?;
        let version = normalize_required_text(
            data.version,
            "恢复版本不能为空",
            RESTORATION_VERSION_MAX_LEN,
            "恢复版本过长",
        )?;
        if data.restored_amount.to_decimal() <= rust_decimal::Decimal::ZERO {
            return Err(Error::from("恢复金额必须大于零"));
        }

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            mall_order_fact_id: data.mall_order_fact_id,
            after_sales_request_id: data.after_sales_request_id,
            mall_refund_id: data.mall_refund_id,
            mall_id,
            external_restoration_no,
            version,
            restored_amount: data.restored_amount,
            restored_at: data.restored_at,
        })
    }
}

/// 余额恢复分配创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MallBalanceRestorationAllocationData {
    /// 余额恢复头。
    pub mall_balance_restoration_id: MallBalanceRestorationId,
    /// 稳定分配序号（从 1 起）。
    pub allocation_no: u32,
    /// 原 CARD 退款资金分配。
    pub mall_refund_allocation_id: MallRefundAllocationId,
    /// 实际恢复到的原支付卡实例。
    pub mall_card_instance_id: MallCardInstanceId,
    /// 本卡恢复金额。
    pub restored_amount: Amount,
}

/// 余额恢复分配实体（数据模型 §6.18）。
///
/// `(mall_balance_restoration_id, allocation_no)` 唯一由 P2 唯一索引落实；
/// 卡实例必须等于原支付来源的卡实例、累计不超过 CARD 退款净额由 P3 落实
/// （P3 条目：§6.18 恢复上限，对应 §8.4 第 4 条）。不可变，只提供 `new()`。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct MallBalanceRestorationAllocation {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 余额恢复头。
    pub mall_balance_restoration_id: MallBalanceRestorationId,
    /// 稳定分配序号。
    pub allocation_no: u32,
    /// 原 CARD 退款资金分配。
    pub mall_refund_allocation_id: MallRefundAllocationId,
    /// 实际恢复到的原支付卡实例。
    pub mall_card_instance_id: MallCardInstanceId,
    /// 本卡恢复金额。
    pub restored_amount: Amount,
}

impl MallBalanceRestorationAllocation {
    /// 创建余额恢复分配。
    ///
    /// `allocation_no` 从 1 起；恢复金额非负（本卡恢复金额，§6.18）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::MallBalanceRestorationAllocationId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的余额恢复分配实体。
    ///
    /// # 错误
    /// 当序号为 0 或恢复金额为负时返回错误。
    pub fn new(
        id: MallBalanceRestorationAllocationId,
        data: MallBalanceRestorationAllocationData,
    ) -> Result<Self> {
        if data.allocation_no == 0 {
            return Err(Error::from("分配序号必须从 1 开始"));
        }
        if data.restored_amount.to_decimal().is_sign_negative() {
            return Err(Error::from("恢复金额不能为负"));
        }

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            mall_balance_restoration_id: data.mall_balance_restoration_id,
            allocation_no: data.allocation_no,
            mall_refund_allocation_id: data.mall_refund_allocation_id,
            mall_card_instance_id: data.mall_card_instance_id,
            restored_amount: data.restored_amount,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MallBalanceRestoration, MallBalanceRestorationAllocation, MallBalanceRestorationAllocationData,
        MallBalanceRestorationData,
    };
    use crate::common::time::Instant;
    use crate::ids::{
        MallAfterSalesRequestId, MallBalanceRestorationAllocationId, MallBalanceRestorationId,
        MallCardInstanceId, MallOrderFactId, MallRefundAllocationId, MallRefundId,
    };
    use crate::money::Amount;
    use std::str::FromStr;

    fn restoration_data() -> MallBalanceRestorationData {
        MallBalanceRestorationData {
            mall_order_fact_id: MallOrderFactId::new("fact-3"),
            after_sales_request_id: MallAfterSalesRequestId::new("asr-1"),
            mall_refund_id: MallRefundId::new("refund-1"),
            mall_id: " mall-a ".to_string(),
            external_restoration_no: " br-200 ".to_string(),
            version: " v1 ".to_string(),
            restored_amount: Amount::from_str("49.00").unwrap(),
            restored_at: Instant::from_unix_secs(1_700_000_300),
        }
    }

    fn allocation_data() -> MallBalanceRestorationAllocationData {
        MallBalanceRestorationAllocationData {
            mall_balance_restoration_id: MallBalanceRestorationId::new("br-1"),
            allocation_no: 1,
            mall_refund_allocation_id: MallRefundAllocationId::new("ra-1"),
            mall_card_instance_id: MallCardInstanceId::new("card-1"),
            restored_amount: Amount::from_str("49.00").unwrap(),
        }
    }

    /// happy path：恢复头文本规范化、事实/退款/售后关联落库。
    #[test]
    fn restoration_new_trims_fields_and_keeps_links() {
        let restoration =
            MallBalanceRestoration::new(MallBalanceRestorationId::new("br-1"), restoration_data()).unwrap();

        assert_eq!(restoration.mall_id, "mall-a");
        assert_eq!(restoration.external_restoration_no, "br-200");
        assert_eq!(restoration.version, "v1");
        assert_eq!(restoration.restored_amount, Amount::from_str("49.00").unwrap());
        assert_eq!(restoration.mall_order_fact_id, MallOrderFactId::new("fact-3"));
        assert_eq!(restoration.mall_refund_id, MallRefundId::new("refund-1"));
        assert_eq!(
            restoration.after_sales_request_id,
            MallAfterSalesRequestId::new("asr-1")
        );
    }

    /// 失败路径：必填空、超长、恢复金额非正。
    #[test]
    fn restoration_new_rejects_blank_overlong_and_non_positive_amount() {
        let blank = MallBalanceRestorationData {
            external_restoration_no: "  ".to_string(),
            ..restoration_data()
        };
        assert!(MallBalanceRestoration::new(MallBalanceRestorationId::new("br-2"), blank).is_err());

        let overlong = MallBalanceRestorationData {
            version: "v".repeat(65),
            ..restoration_data()
        };
        assert!(MallBalanceRestoration::new(MallBalanceRestorationId::new("br-3"), overlong).is_err());

        let zero = MallBalanceRestorationData {
            restored_amount: Amount::from_str("0.00").unwrap(),
            ..restoration_data()
        };
        assert!(MallBalanceRestoration::new(MallBalanceRestorationId::new("br-4"), zero).is_err());
    }

    /// 分配：happy path 与序号/金额越界拒绝。
    #[test]
    fn allocation_keeps_fields_and_rejects_invalid_scope() {
        let allocation = MallBalanceRestorationAllocation::new(
            MallBalanceRestorationAllocationId::new("bra-1"),
            allocation_data(),
        )
        .unwrap();
        assert_eq!(allocation.allocation_no, 1);
        assert_eq!(
            allocation.mall_card_instance_id,
            MallCardInstanceId::new("card-1")
        );
        assert_eq!(
            allocation.mall_refund_allocation_id,
            MallRefundAllocationId::new("ra-1")
        );
        assert_eq!(allocation.restored_amount, Amount::from_str("49.00").unwrap());

        let zero_no = MallBalanceRestorationAllocationData {
            allocation_no: 0,
            ..allocation_data()
        };
        assert!(MallBalanceRestorationAllocation::new(
            MallBalanceRestorationAllocationId::new("bra-2"),
            zero_no,
        )
        .is_err());

        let negative = MallBalanceRestorationAllocationData {
            restored_amount: Amount::from_str("-1.00").unwrap(),
            ..allocation_data()
        };
        assert!(MallBalanceRestorationAllocation::new(
            MallBalanceRestorationAllocationId::new("bra-3"),
            negative,
        )
        .is_err());

        let zero_amount = MallBalanceRestorationAllocationData {
            restored_amount: Amount::from_str("0.00").unwrap(),
            ..allocation_data()
        };
        assert!(
            MallBalanceRestorationAllocation::new(
                MallBalanceRestorationAllocationId::new("bra-4"),
                zero_amount,
            )
            .is_ok(),
            "分配金额非负（可为零）"
        );
    }
}
