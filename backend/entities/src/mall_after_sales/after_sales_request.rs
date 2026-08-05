//! `mall_after_sales_request` 与 `mall_after_sales_request_line`（数据模型 §6.18）。
//!
//! 售后请求是业务动作和审计载体，不表示取消或退款已经成功；ERP 按
//! 「商城 + 商城售后请求 ID」幂等接收，同一请求不得重复调用供应商（§6.18、
//! `erp-phase-2.md` §11.1）。请求头状态沿 [`AfterSalesRequestStatus`] 固定邻接
//! 推进，关闭条件从适用事实派生（P3 条目：§6.18 关闭条件派生）。
//!
//! 跨行约束（§6.18）：同一商品累计有效申请数量和金额不得超过已支付且尚未被成功
//! 退款覆盖的数量和金额；申请可包含多个商品和多个供应商，头表不得保存单一商品
//! 或单一供应商订单外键——累计上限与适用判定依赖聚合查询，由 P3 落实
//! （P3 条目：§6.18 申请累计上限）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::state::ensure_transition;
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{
    MallAfterSalesRequestId, MallAfterSalesRequestLineId, MallOrderId, MallOrderItemId,
    SupplierFulfillmentItemId,
};
use crate::mall_after_sales::types::{AfterSalesLineStatus, AfterSalesRequestStatus, AfterSalesRequestType};
use crate::money::{Amount, Quantity};
use crate::validation::normalize_required_text;

/// 目标商城代码最大长度。
const MALL_ID_MAX_LEN: usize = 64;
/// 商城售后请求稳定身份最大长度。
const EXTERNAL_REQUEST_ID_MAX_LEN: usize = 128;
/// 员工售后原因最大长度。
const REASON_MAX_LEN: usize = 512;

/// 售后请求创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MallAfterSalesRequestData {
    /// 商城售后请求稳定身份。
    pub mall_id: String,
    /// 商城售后请求稳定身份。
    pub external_request_id: String,
    /// 原商城订单。
    pub mall_order_id: MallOrderId,
    /// 取消或退款。
    pub request_type: AfterSalesRequestType,
    /// 员工售后原因。
    pub reason: String,
    /// 商城申请时间。
    pub created_at: Instant,
}

/// 售后请求头实体（数据模型 §6.18）。
///
/// 创建时状态为 `Received`；状态迁移沿固定邻接推进（见
/// [`AfterSalesRequestStatus`]），`Closed` 为不可逆终态。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct MallAfterSalesRequest {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 来源商城。
    pub mall_id: String,
    /// 商城售后请求稳定身份。
    pub external_request_id: String,
    /// 原商城订单。
    pub mall_order_id: MallOrderId,
    /// 取消或退款。
    pub request_type: AfterSalesRequestType,
    /// 员工售后原因。
    pub reason: String,
    /// 请求状态。
    pub status: AfterSalesRequestStatus,
    /// 商城申请时间。
    pub created_at: Instant,
}

impl MallAfterSalesRequest {
    /// 创建售后请求。
    ///
    /// 完成文本字段校验与规范化；状态固定为 `Received`，商城申请时间按字典
    /// 使用商城侧 `created_at`（与 ERP 记录时间 `BaseModel.created_at` 分开）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::MallAfterSalesRequestId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的售后请求头实体。
    ///
    /// # 错误
    /// 当必填文本为空或超长时返回错误。
    pub fn new(id: MallAfterSalesRequestId, data: MallAfterSalesRequestData) -> Result<Self> {
        let mall_id = normalize_required_text(
            data.mall_id,
            "来源商城不能为空",
            MALL_ID_MAX_LEN,
            "来源商城代码过长",
        )?;
        let external_request_id = normalize_required_text(
            data.external_request_id,
            "商城售后请求ID不能为空",
            EXTERNAL_REQUEST_ID_MAX_LEN,
            "商城售后请求ID过长",
        )?;
        let reason =
            normalize_required_text(data.reason, "售后原因不能为空", REASON_MAX_LEN, "售后原因过长")?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            mall_id,
            external_request_id,
            mall_order_id: data.mall_order_id,
            request_type: data.request_type,
            reason,
            status: AfterSalesRequestStatus::Received,
            created_at: data.created_at,
        })
    }

    /// 推进售后请求状态。
    ///
    /// 沿固定邻接推进（§6.18）；`Closed` 为不可逆终态，关闭只能由适用事实派生
    /// （P3 校验关闭条件，不得手工直接标记）。
    ///
    /// # 参数
    /// * `to` - 目标状态
    ///
    /// # 返回
    /// 迁移合法返回 `Ok(())`。
    ///
    /// # 错误
    /// 目标不在后继列表中且与当前状态不同时返回 `InvalidStateTransition`。
    pub fn transition_to(&mut self, to: AfterSalesRequestStatus) -> Result<()> {
        ensure_transition(self.status, to)?;
        self.status = to;
        Ok(())
    }
}

/// 售后明细创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MallAfterSalesRequestLineData {
    /// 售后申请。
    pub after_sales_request_id: MallAfterSalesRequestId,
    /// 行号（从 1 起）。
    pub line_no: u32,
    /// 原订单商品。
    pub mall_order_item_id: MallOrderItemId,
    /// 已形成自动供应商履约时的固定明细，可空。
    pub supplier_fulfillment_item_id: Option<SupplierFulfillmentItemId>,
    /// 本商品申请数量。
    pub requested_quantity: Quantity,
    /// 本商品申请金额。
    pub requested_amount: Amount,
    /// 行状态。
    pub line_status: AfterSalesLineStatus,
}

/// 售后明细行实体（数据模型 §6.18）。
///
/// 明细必须属于头表的原订单（跨实体一致性，由 P3 校验）。行状态是固定枚举，
/// 推进由 P3 按供应商动作结果与事实回流派生写入。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct MallAfterSalesRequestLine {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 售后申请。
    pub after_sales_request_id: MallAfterSalesRequestId,
    /// 行号。
    pub line_no: u32,
    /// 原订单商品。
    pub mall_order_item_id: MallOrderItemId,
    /// 已形成自动供应商履约时的固定明细。
    pub supplier_fulfillment_item_id: Option<SupplierFulfillmentItemId>,
    /// 本商品申请数量。
    pub requested_quantity: Quantity,
    /// 本商品申请金额。
    pub requested_amount: Amount,
    /// 行状态。
    pub line_status: AfterSalesLineStatus,
}

impl MallAfterSalesRequestLine {
    /// 创建售后明细行。
    ///
    /// `line_no` 从 1 起；申请数量和金额必须大于零（§6.18 申请数量/金额）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::MallAfterSalesRequestLineId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的售后明细行实体。
    ///
    /// # 错误
    /// 当行号为 0 或申请数量/金额非正时返回错误。
    pub fn new(id: MallAfterSalesRequestLineId, data: MallAfterSalesRequestLineData) -> Result<Self> {
        if data.line_no == 0 {
            return Err(Error::from("行号必须从 1 开始"));
        }
        if data.requested_quantity.to_decimal() <= rust_decimal::Decimal::ZERO {
            return Err(Error::from("申请数量必须大于零"));
        }
        if data.requested_amount.to_decimal() <= rust_decimal::Decimal::ZERO {
            return Err(Error::from("申请金额必须大于零"));
        }

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            after_sales_request_id: data.after_sales_request_id,
            line_no: data.line_no,
            mall_order_item_id: data.mall_order_item_id,
            supplier_fulfillment_item_id: data.supplier_fulfillment_item_id,
            requested_quantity: data.requested_quantity,
            requested_amount: data.requested_amount,
            line_status: data.line_status,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MallAfterSalesRequest, MallAfterSalesRequestData, MallAfterSalesRequestLine,
        MallAfterSalesRequestLineData,
    };
    use crate::common::state::{ensure_transition, DocumentState};
    use crate::common::time::Instant;
    use crate::ids::{
        MallAfterSalesRequestId, MallAfterSalesRequestLineId, MallOrderId, MallOrderItemId,
        SupplierFulfillmentItemId,
    };
    use crate::mall_after_sales::types::{
        AfterSalesLineStatus, AfterSalesRequestStatus, AfterSalesRequestType,
    };
    use crate::money::{Amount, Quantity};
    use std::str::FromStr;

    fn request_data() -> MallAfterSalesRequestData {
        MallAfterSalesRequestData {
            mall_id: " mall-a ".to_string(),
            external_request_id: " asr-100 ".to_string(),
            mall_order_id: MallOrderId::new("order-1"),
            request_type: AfterSalesRequestType::Refund,
            reason: " 商品破损 ".to_string(),
            created_at: Instant::from_unix_secs(1_700_000_050),
        }
    }

    fn line_data() -> MallAfterSalesRequestLineData {
        MallAfterSalesRequestLineData {
            after_sales_request_id: MallAfterSalesRequestId::new("asr-1"),
            line_no: 1,
            mall_order_item_id: MallOrderItemId::new("item-1"),
            supplier_fulfillment_item_id: Some(SupplierFulfillmentItemId::new("sfi-1")),
            requested_quantity: Quantity::from_str("1.000000").unwrap(),
            requested_amount: Amount::from_str("49.00").unwrap(),
            line_status: AfterSalesLineStatus::Pending,
        }
    }

    /// happy path：文本规范化，状态初始为已接收，商城申请时间落库。
    #[test]
    fn request_new_trims_fields_and_starts_received() {
        let request =
            MallAfterSalesRequest::new(MallAfterSalesRequestId::new("asr-1"), request_data()).unwrap();

        assert_eq!(request.mall_id, "mall-a");
        assert_eq!(request.external_request_id, "asr-100");
        assert_eq!(request.reason, "商品破损");
        assert_eq!(request.status, AfterSalesRequestStatus::Received);
        assert_eq!(request.created_at, Instant::from_unix_secs(1_700_000_050));
        assert_eq!(request.request_type, AfterSalesRequestType::Refund);
    }

    /// 失败路径：必填空、超长。
    #[test]
    fn request_new_rejects_blank_and_overlong_text() {
        let blank = MallAfterSalesRequestData {
            external_request_id: "  ".to_string(),
            ..request_data()
        };
        assert!(MallAfterSalesRequest::new(MallAfterSalesRequestId::new("asr-2"), blank).is_err());

        let overlong = MallAfterSalesRequestData {
            reason: "x".repeat(513),
            ..request_data()
        };
        assert!(MallAfterSalesRequest::new(MallAfterSalesRequestId::new("asr-3"), overlong).is_err());
    }

    /// 状态机：合法迁移、非法迁移与终态定向断言。
    #[test]
    fn request_status_machine_directed_edges() {
        assert!(ensure_transition(
            AfterSalesRequestStatus::Received,
            AfterSalesRequestStatus::SupplierProcessing
        )
        .is_ok());
        assert!(ensure_transition(
            AfterSalesRequestStatus::Received,
            AfterSalesRequestStatus::RefundProcessing
        )
        .is_ok());
        assert!(ensure_transition(
            AfterSalesRequestStatus::Received,
            AfterSalesRequestStatus::ManualNeeded
        )
        .is_ok());
        assert!(ensure_transition(
            AfterSalesRequestStatus::SupplierProcessing,
            AfterSalesRequestStatus::PartiallyCompleted
        )
        .is_ok());
        assert!(ensure_transition(
            AfterSalesRequestStatus::PartiallyCompleted,
            AfterSalesRequestStatus::Closed
        )
        .is_ok());
        assert!(ensure_transition(
            AfterSalesRequestStatus::RefundProcessing,
            AfterSalesRequestStatus::Closed
        )
        .is_ok());
        assert!(ensure_transition(
            AfterSalesRequestStatus::ManualNeeded,
            AfterSalesRequestStatus::SupplierProcessing
        )
        .is_ok());
        assert!(ensure_transition(
            AfterSalesRequestStatus::ManualNeeded,
            AfterSalesRequestStatus::Closed
        )
        .is_ok());

        assert!(
            ensure_transition(AfterSalesRequestStatus::Received, AfterSalesRequestStatus::Closed).is_err(),
            "未处理完成不得直接关闭"
        );
        assert!(
            ensure_transition(
                AfterSalesRequestStatus::PartiallyCompleted,
                AfterSalesRequestStatus::Received
            )
            .is_err(),
            "状态不得倒退"
        );
        assert!(ensure_transition(
            AfterSalesRequestStatus::Closed,
            AfterSalesRequestStatus::RefundProcessing
        )
        .is_err());
        assert!(ensure_transition(
            AfterSalesRequestStatus::RefundProcessing,
            AfterSalesRequestStatus::SupplierProcessing
        )
        .is_err());
        assert_eq!(
            AfterSalesRequestStatus::Closed.allowed_next(),
            &[] as &[AfterSalesRequestStatus]
        );
    }

    /// happy path + 状态机：请求沿固定邻接推进到关闭。
    #[test]
    fn request_transitions_along_fixed_adjacency() {
        let mut request =
            MallAfterSalesRequest::new(MallAfterSalesRequestId::new("asr-1"), request_data()).unwrap();

        request
            .transition_to(AfterSalesRequestStatus::SupplierProcessing)
            .unwrap();
        request
            .transition_to(AfterSalesRequestStatus::RefundProcessing)
            .unwrap();
        assert!(request
            .transition_to(AfterSalesRequestStatus::SupplierProcessing)
            .is_err());
        request.transition_to(AfterSalesRequestStatus::Closed).unwrap();
        assert!(request
            .transition_to(AfterSalesRequestStatus::ManualNeeded)
            .is_err());
        assert!(
            request.transition_to(AfterSalesRequestStatus::Closed).is_ok(),
            "幂等迁移恒合法"
        );
    }

    /// 明细行：happy path 与行号/数量/金额越界拒绝。
    #[test]
    fn line_new_keeps_fields_and_rejects_invalid_scope() {
        let line =
            MallAfterSalesRequestLine::new(MallAfterSalesRequestLineId::new("asrl-1"), line_data()).unwrap();
        assert_eq!(line.line_no, 1);
        assert_eq!(line.requested_quantity, Quantity::from_str("1.000000").unwrap());
        assert_eq!(line.requested_amount, Amount::from_str("49.00").unwrap());
        assert_eq!(line.line_status, AfterSalesLineStatus::Pending);
        assert_eq!(
            line.supplier_fulfillment_item_id,
            Some(SupplierFulfillmentItemId::new("sfi-1"))
        );

        let zero_no = MallAfterSalesRequestLineData {
            line_no: 0,
            ..line_data()
        };
        assert!(MallAfterSalesRequestLine::new(MallAfterSalesRequestLineId::new("asrl-2"), zero_no).is_err());

        let zero_quantity = MallAfterSalesRequestLineData {
            requested_quantity: Quantity::from_str("0.000000").unwrap(),
            ..line_data()
        };
        assert!(
            MallAfterSalesRequestLine::new(MallAfterSalesRequestLineId::new("asrl-3"), zero_quantity)
                .is_err()
        );

        let zero_amount = MallAfterSalesRequestLineData {
            requested_amount: Amount::from_str("0.00").unwrap(),
            ..line_data()
        };
        assert!(
            MallAfterSalesRequestLine::new(MallAfterSalesRequestLineId::new("asrl-4"), zero_amount).is_err()
        );
    }
}
