//! `supplier_fulfillment_order`（数据模型 §6.19 供应商履约订单）。
//!
//! 履约主线、取消与退款是三条正交状态机（§7.6），定义与邻接矩阵见 [`super::status`]；
//! 明细见 [`super::fulfillment_item`]。`COMPLETED`/`REJECTED` 是终态，乱序或重复回调
//! 经 [`crate::common::state::ensure_transition`] 拒绝（从高状态回低状态即非法迁移）。

use std::fmt;

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::state::ensure_transition;
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{SupplierAccountId, SupplierApiConnectionId, SupplierFulfillmentOrderId};
use crate::validation::{normalize_optional_text, normalize_required_text};

// 兼容既有深层导入路径：`supplier_fulfillment::fulfillment_order::{...}`。
pub use super::fulfillment_item::{SupplierFulfillmentItem, SupplierFulfillmentItemData};
use super::order_action::{SupplierOrderAction, SupplierOrderActionStatus, SupplierOrderActionType};
pub use super::status::{CancelStatus, FulfillmentStatus, RefundStatus, VerifiedSupplierOrderResolution};

/// 供应商子订单号（ERP 下单幂等键）最大长度。
const ORDER_NO_MAX_LEN: usize = 64;
/// 外部（供应商）订单号最大长度。
const EXTERNAL_ORDER_NO_MAX_LEN: usize = 64;
/// 履约地址快照加密值最大长度。
const ADDRESS_ENCRYPTED_MAX_LEN: usize = 8192;
/// 履约地址快照 HMAC 查询指纹最大长度。
const ADDRESS_FINGERPRINT_MAX_LEN: usize = 128;

/// 供应商子订单创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierFulfillmentOrderData {
    /// ERP 供应商子订单号（唯一，也是下单幂等键）。
    pub fulfillment_order_no: String,
    /// 固定供应商。
    pub supplier_id: SupplierAccountId,
    /// 供应商 API 连接。
    pub connection_id: SupplierApiConnectionId,
    /// 同一商城订单、同一供应商下的确定性拆单序号。
    pub split_no: u32,
    /// 履约主线状态。
    pub fulfillment_status: FulfillmentStatus,
    /// 取消进度状态。
    pub cancel_status: CancelStatus,
    /// 退款进度状态。
    pub refund_status: RefundStatus,
    /// 供应商订单号（下单成功回传后填写）。
    pub external_order_no: Option<String>,
    /// 提交给供应商的时间。
    pub submitted_at: Option<Instant>,
    /// 供应商接单时间。
    pub accepted_at: Option<Instant>,
    /// 履约完成时间。
    pub completed_at: Option<Instant>,
    /// 履约地址快照加密值（§4.5.5，完整值加密存储）。
    pub address_snapshot_encrypted: String,
    /// 履约地址快照带密钥 HMAC 查询指纹（§4.5.5，禁止裸摘要）。
    pub address_snapshot_fingerprint: String,
}

impl SupplierFulfillmentOrderData {
    /// 构造已进入提交中的供应商子订单数据。
    ///
    /// 初始三条状态与提交时间由领域规则统一设置，调用方只提供稳定身份和地址
    /// 快照，不得自行组合不一致的初始状态。
    ///
    /// # 参数
    /// * `fulfillment_order_no` - ERP 供应商子订单号
    /// * `supplier_id` - 固定供应商
    /// * `connection_id` - 供应商 API 连接
    /// * `split_no` - 确定性拆单序号
    /// * `submitted_at` - 提交供应商的时间
    /// * `address_snapshot_encrypted` - 地址快照密文
    /// * `address_snapshot_fingerprint` - 地址查询指纹
    ///
    /// # 返回
    /// 返回主线为提交中、取消与退款均为无的创建数据。
    #[allow(clippy::too_many_arguments)]
    pub fn submitting(
        fulfillment_order_no: impl Into<String>,
        supplier_id: SupplierAccountId,
        connection_id: SupplierApiConnectionId,
        split_no: u32,
        submitted_at: Instant,
        address_snapshot_encrypted: impl Into<String>,
        address_snapshot_fingerprint: impl Into<String>,
    ) -> Self {
        Self {
            fulfillment_order_no: fulfillment_order_no.into(),
            supplier_id,
            connection_id,
            split_no,
            fulfillment_status: FulfillmentStatus::Submitting,
            cancel_status: CancelStatus::None,
            refund_status: RefundStatus::None,
            external_order_no: None,
            submitted_at: Some(submitted_at),
            accepted_at: None,
            completed_at: None,
            address_snapshot_encrypted: address_snapshot_encrypted.into(),
            address_snapshot_fingerprint: address_snapshot_fingerprint.into(),
        }
    }
}

/// 供应商子订单更新数据（不含系统字段与关键字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SupplierFulfillmentOrderUpdate {
    /// 供应商订单号；`None` 表示不修改。状态推进走 `advance_*` 系列方法。
    pub external_order_no: Option<String>,
}

/// 供应商子订单实体（数据模型 §6.19，正式单据）。
///
/// `fulfillment_order_no`、`supplier_id`、`connection_id`、`split_no`
/// 与三条状态是创建后不可修改的关键字段；地址快照为敏感值，`Debug` 一律脱敏。
#[derive(Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SupplierFulfillmentOrder {
    #[serde(flatten)]
    pub base: BaseModel,
    /// ERP 供应商子订单号。
    pub fulfillment_order_no: String,
    /// 固定供应商。
    pub supplier_id: SupplierAccountId,
    /// 供应商 API 连接。
    pub connection_id: SupplierApiConnectionId,
    /// 确定性拆单序号。
    pub split_no: u32,
    /// 履约主线状态。
    pub fulfillment_status: FulfillmentStatus,
    /// 取消进度状态。
    pub cancel_status: CancelStatus,
    /// 退款进度状态。
    pub refund_status: RefundStatus,
    /// 供应商订单号。
    pub external_order_no: Option<String>,
    /// 提交给供应商的时间。
    pub submitted_at: Option<Instant>,
    /// 供应商接单时间。
    pub accepted_at: Option<Instant>,
    /// 履约完成时间。
    pub completed_at: Option<Instant>,
    /// 履约地址快照加密值。
    pub address_snapshot_encrypted: String,
    /// 履约地址快照 HMAC 查询指纹。
    pub address_snapshot_fingerprint: String,
}

impl SupplierFulfillmentOrder {
    /// 创建供应商子订单。
    ///
    /// 完成单号/地址快照的校验与规范化，并按状态机校验里程碑时间（§6.19：
    /// 提交后 `submitted_at` 必填、接单后 `accepted_at` 必填、完成后 `completed_at` 必填）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SupplierFulfillmentOrderId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的子订单实体。
    ///
    /// # 错误
    /// 单号/地址快照为空或超长、状态与里程碑时间不一致时返回错误。
    pub fn new(id: SupplierFulfillmentOrderId, data: SupplierFulfillmentOrderData) -> Result<Self> {
        let fulfillment_order_no = normalize_required_text(
            data.fulfillment_order_no,
            "供应商子订单号不能为空",
            ORDER_NO_MAX_LEN,
            "供应商子订单号过长",
        )?;
        let external_order_no =
            normalize_optional_text(data.external_order_no, "外部订单号", EXTERNAL_ORDER_NO_MAX_LEN)?;
        let address_snapshot_encrypted = normalize_required_text(
            data.address_snapshot_encrypted,
            "履约地址快照不能为空",
            ADDRESS_ENCRYPTED_MAX_LEN,
            "履约地址快照过长",
        )?;
        let address_snapshot_fingerprint = normalize_required_text(
            data.address_snapshot_fingerprint,
            "履约地址查询指纹不能为空",
            ADDRESS_FINGERPRINT_MAX_LEN,
            "履约地址查询指纹过长",
        )?;
        ensure_timestamp_consistency(
            data.fulfillment_status,
            data.submitted_at,
            data.accepted_at,
            data.completed_at,
        )?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            fulfillment_order_no,
            supplier_id: data.supplier_id,
            connection_id: data.connection_id,
            split_no: data.split_no,
            fulfillment_status: data.fulfillment_status,
            cancel_status: data.cancel_status,
            refund_status: data.refund_status,
            external_order_no,
            submitted_at: data.submitted_at,
            accepted_at: data.accepted_at,
            completed_at: data.completed_at,
            address_snapshot_encrypted,
            address_snapshot_fingerprint,
        })
    }

    /// 更新供应商子订单。
    ///
    /// 复用 `new` 的校验规则；单号、来源、供应商、连接、拆单序号与三条状态是
    /// 关键字段，状态只能通过 `advance_*` 系列方法推进。
    ///
    /// # 参数
    /// * `update` - 更新数据
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 外部订单号为空或超长时返回错误。
    pub fn update(&mut self, update: SupplierFulfillmentOrderUpdate) -> Result<()> {
        if let Some(external_order_no) = update.external_order_no {
            self.external_order_no = Some(normalize_required_text(
                external_order_no,
                "外部订单号不能为空",
                EXTERNAL_ORDER_NO_MAX_LEN,
                "外部订单号过长",
            )?);
        }
        Ok(())
    }

    /// 推进履约主线状态。
    ///
    /// 按 §7.6 固化邻接矩阵校验迁移；进入 `SUBMITTING`/`ACCEPTED`/`COMPLETED` 时
    /// 补填缺失的里程碑时间，重复回调（幂等迁移）不覆盖已记录时间。
    ///
    /// # 参数
    /// * `to` - 目标状态
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 目标不在合法后继中（含从高状态回低状态）时返回
    /// [`crate::errors::Error::InvalidStateTransition`]。
    pub fn advance_fulfillment(&mut self, to: FulfillmentStatus) -> Result<()> {
        ensure_transition(self.fulfillment_status, to)?;
        self.fulfillment_status = to;
        match to {
            FulfillmentStatus::Submitting => {
                self.submitted_at.get_or_insert_with(Instant::now);
            }
            FulfillmentStatus::Accepted => {
                self.accepted_at.get_or_insert_with(Instant::now);
            }
            FulfillmentStatus::Completed => {
                self.completed_at.get_or_insert_with(Instant::now);
            }
            FulfillmentStatus::Received
            | FulfillmentStatus::Rejected
            | FulfillmentStatus::ResultUnknown
            | FulfillmentStatus::Fulfilling
            | FulfillmentStatus::Shipped
            | FulfillmentStatus::Exception => {}
        }
        Ok(())
    }

    /// 推进取消进度状态。
    ///
    /// # 参数
    /// * `to` - 目标状态
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 目标不在合法后继中时返回 [`crate::errors::Error::InvalidStateTransition`]。
    pub fn advance_cancel(&mut self, to: CancelStatus) -> Result<()> {
        ensure_transition(self.cancel_status, to)?;
        self.cancel_status = to;
        Ok(())
    }

    /// 推进退款进度状态。
    ///
    /// # 参数
    /// * `to` - 目标状态
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 目标不在合法后继中时返回 [`crate::errors::Error::InvalidStateTransition`]。
    pub fn advance_refund(&mut self, to: RefundStatus) -> Result<()> {
        ensure_transition(self.refund_status, to)?;
        self.refund_status = to;
        Ok(())
    }

    /// 校验调用方持有的订单版本仍是当前版本。
    ///
    /// # 参数
    /// * `expected` - 调用方读取到的订单版本
    ///
    /// # 返回
    /// 版本一致时返回 `Ok(())`。
    ///
    /// # 错误
    /// 版本不一致时返回领域错误。
    pub fn ensure_version(&self, expected: u64) -> Result<()> {
        if self.base.version != expected {
            return Err(Error::from("供应商履约订单版本不一致"));
        }
        Ok(())
    }

    /// 判断订单是否属于指定供应商。
    ///
    /// # 参数
    /// * `supplier_id` - 待校验供应商
    ///
    /// # 返回
    /// 供应商一致时返回 `true`。
    pub fn belongs_to_supplier(&self, supplier_id: &SupplierAccountId) -> bool {
        self.supplier_id == *supplier_id
    }

    /// 返回与履约完成状态一致的正式完成时间。
    ///
    /// # 返回
    /// 已完成订单返回完成时间；尚未完成且没有完成时间时返回 `None`。
    ///
    /// # 错误
    /// 完成状态缺少完成时间，或未完成状态携带完成时间时返回领域错误。
    pub fn confirmed_completed_at(&self) -> Result<Option<Instant>> {
        match (self.fulfillment_status, self.completed_at) {
            (FulfillmentStatus::Completed, Some(completed_at)) => Ok(Some(completed_at)),
            (FulfillmentStatus::Completed, None) => Err(Error::from("已完成订单缺少正式完成时间")),
            (_, Some(_)) => Err(Error::from("未完成订单不得携带正式完成时间")),
            (_, None) => Ok(None),
        }
    }

    /// 校验订单已经到达取消完成终态。
    ///
    /// # 返回
    /// 取消状态为 `CANCELED` 时返回 `Ok(())`。
    ///
    /// # 错误
    /// 订单尚未取消完成时返回领域错误。
    pub fn ensure_canceled(&self) -> Result<()> {
        if self.cancel_status != CancelStatus::Canceled {
            return Err(Error::from("取消补证与供应商订单取消终态不一致"));
        }
        Ok(())
    }

    /// 根据订单状态与原供应商动作状态判定已验证业务终态。
    ///
    /// # 参数
    /// * `action` - 与订单关联的原下单、取消或退款动作
    ///
    /// # 返回
    /// 订单与动作共同证明终态时返回对应结果；关系或状态不足时返回 `None`。
    pub fn verified_resolution(
        &self,
        action: &SupplierOrderAction,
    ) -> Option<VerifiedSupplierOrderResolution> {
        if action.supplier_fulfillment_order_id.as_ref() != self.base.id {
            return None;
        }
        match action.action_type {
            SupplierOrderActionType::Place => match (self.fulfillment_status, action.status) {
                (FulfillmentStatus::Completed, SupplierOrderActionStatus::Succeeded) => {
                    Some(VerifiedSupplierOrderResolution::OrderCompleted)
                }
                (FulfillmentStatus::Rejected, SupplierOrderActionStatus::Failed) => {
                    Some(VerifiedSupplierOrderResolution::OrderRejected)
                }
                (
                    FulfillmentStatus::Accepted | FulfillmentStatus::Fulfilling | FulfillmentStatus::Shipped,
                    SupplierOrderActionStatus::Succeeded,
                ) => Some(VerifiedSupplierOrderResolution::OrderAccepted),
                _ => None,
            },
            SupplierOrderActionType::Cancel
                if self.cancel_status == CancelStatus::Canceled
                    && action.status == SupplierOrderActionStatus::Succeeded =>
            {
                Some(VerifiedSupplierOrderResolution::Canceled)
            }
            SupplierOrderActionType::Refund
                if self.refund_status == RefundStatus::Refunded
                    && action.status == SupplierOrderActionStatus::Succeeded =>
            {
                Some(VerifiedSupplierOrderResolution::Refunded)
            }
            SupplierOrderActionType::Cancel
            | SupplierOrderActionType::Refund
            | SupplierOrderActionType::Query => None,
        }
    }

    /// 判断原下单动作是否处于可由明确无结果证据重放的状态。
    ///
    /// # 参数
    /// * `action` - 待重放的原动作
    ///
    /// # 返回
    /// 订单与动作均为结果未知的原下单请求时返回 `true`。
    pub fn can_replay_place_action(&self, action: &SupplierOrderAction) -> bool {
        action.supplier_fulfillment_order_id.as_ref() == self.base.id
            && action.action_type == SupplierOrderActionType::Place
            && action.status == SupplierOrderActionStatus::ResultUnknown
            && self.fulfillment_status == FulfillmentStatus::ResultUnknown
    }
}

impl fmt::Debug for SupplierFulfillmentOrder {
    /// 脱敏调试输出：地址快照加密值与查询指纹只输出 `<redacted>`，
    /// 禁止在日志中泄漏敏感值（数据模型 §4.5.5）。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SupplierFulfillmentOrder")
            .field("base", &self.base)
            .field("fulfillment_order_no", &self.fulfillment_order_no)
            .field("supplier_id", &self.supplier_id)
            .field("connection_id", &self.connection_id)
            .field("split_no", &self.split_no)
            .field("fulfillment_status", &self.fulfillment_status)
            .field("cancel_status", &self.cancel_status)
            .field("refund_status", &self.refund_status)
            .field("external_order_no", &self.external_order_no)
            .field("submitted_at", &self.submitted_at)
            .field("accepted_at", &self.accepted_at)
            .field("completed_at", &self.completed_at)
            .field("address_snapshot_encrypted", &"<redacted>")
            .field("address_snapshot_fingerprint", &"<redacted>")
            .finish()
    }
}

/// 校验状态与里程碑时间的一致性（§6.19：到达该状态后对应时间必填）。
///
/// # 参数
/// * `status` - 履约主线状态
/// * `submitted_at` - 提交时间
/// * `accepted_at` - 接单时间
/// * `completed_at` - 完成时间
///
/// # 错误
/// 状态已越过提交/接单/完成节点但对应时间为空时返回错误。
fn ensure_timestamp_consistency(
    status: FulfillmentStatus,
    submitted_at: Option<Instant>,
    accepted_at: Option<Instant>,
    completed_at: Option<Instant>,
) -> Result<()> {
    if status != FulfillmentStatus::Received && submitted_at.is_none() {
        return Err(Error::from("进入提交中后 submitted_at 必填"));
    }
    if !matches!(
        status,
        FulfillmentStatus::Received | FulfillmentStatus::Submitting
    ) && accepted_at.is_none()
    {
        return Err(Error::from("进入已接单后 accepted_at 必填"));
    }
    if status == FulfillmentStatus::Completed && completed_at.is_none() {
        return Err(Error::from("已完成状态 completed_at 必填"));
    }
    Ok(())
}

/// 计算敏感值的带密钥 HMAC 查询指纹（数据模型 §4.5.5，禁止裸摘要）。
///
/// # 参数
/// * `plain` - 敏感明文（如履约地址快照原文，仅取首尾去空白后的内容）
/// * `key` - 密钥字节
///
/// # 返回
/// 返回 64 字符十六进制 HMAC-SHA256 摘要；同一明文与密钥恒等，不同密钥产生不同指纹。
///
/// # 说明
/// 仅在测试环境编译：`hmac`/`sha2` 目前是 dev-dependencies（P0 冻结）；生产路径如需
/// 生成指纹，需经 `chore/erp-p0-amend-*` 地基修订把依赖提升为正式依赖（列为地基修订候选）。
#[cfg(test)]
pub fn fingerprint(plain: &str, key: &[u8]) -> String {
    use hmac::{Hmac, KeyInit, Mac};
    use sha2::Sha256;

    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC 接受任意长度密钥");
    mac.update(plain.trim().as_bytes());
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_data() -> SupplierFulfillmentOrderData {
        SupplierFulfillmentOrderData {
            fulfillment_order_no: " FO-2026-001 ".to_string(),
            supplier_id: SupplierAccountId::new("supplier-1"),
            connection_id: SupplierApiConnectionId::new("connection-1"),
            split_no: 1,
            fulfillment_status: FulfillmentStatus::Received,
            cancel_status: CancelStatus::None,
            refund_status: RefundStatus::None,
            external_order_no: None,
            submitted_at: None,
            accepted_at: None,
            completed_at: None,
            address_snapshot_encrypted: "encrypted-address".to_string(),
            address_snapshot_fingerprint: "fingerprint-address".to_string(),
        }
    }

    fn sample_order() -> SupplierFulfillmentOrder {
        SupplierFulfillmentOrder::new(SupplierFulfillmentOrderId::new("order-1"), sample_data()).unwrap()
    }

    fn place_action(status: SupplierOrderActionStatus) -> SupplierOrderAction {
        SupplierOrderAction::new(
            crate::ids::SupplierOrderActionId::new("action-1"),
            crate::supplier_fulfillment::SupplierOrderActionData {
                supplier_fulfillment_order_id: SupplierFulfillmentOrderId::new("order-1"),
                action_type: SupplierOrderActionType::Place,
                idempotency_key: "order-1".to_string(),
                status,
                external_request_id: None,
                request_summary: None,
                response_summary: None,
                attempt_count: 0,
                next_attempt_at: None,
            },
        )
        .unwrap()
    }

    #[test]
    fn new_trims_and_normalizes_order_fields() {
        let order = sample_order();

        assert_eq!(order.fulfillment_order_no, "FO-2026-001");
        assert_eq!(order.fulfillment_status, FulfillmentStatus::Received);
        assert_eq!(order.cancel_status, CancelStatus::None);
        assert_eq!(order.refund_status, RefundStatus::None);
        assert_eq!(order.split_no, 1);
        assert!(order.external_order_no.is_none());
    }

    #[test]
    fn new_rejects_empty_or_overlong_fulfillment_order_no() {
        let empty = SupplierFulfillmentOrderData {
            fulfillment_order_no: "   ".to_string(),
            ..sample_data()
        };
        assert!(SupplierFulfillmentOrder::new(SupplierFulfillmentOrderId::new("order-2"), empty).is_err());

        let overlong = SupplierFulfillmentOrderData {
            fulfillment_order_no: "x".repeat(65),
            ..sample_data()
        };
        assert!(SupplierFulfillmentOrder::new(SupplierFulfillmentOrderId::new("order-3"), overlong).is_err());
    }

    #[test]
    fn new_rejects_overlong_address_snapshot() {
        let overlong_address = SupplierFulfillmentOrderData {
            address_snapshot_encrypted: "a".repeat(8193),
            ..sample_data()
        };
        assert!(
            SupplierFulfillmentOrder::new(SupplierFulfillmentOrderId::new("order-4"), overlong_address)
                .is_err()
        );

        let blank_address = SupplierFulfillmentOrderData {
            address_snapshot_encrypted: " ".to_string(),
            ..sample_data()
        };
        assert!(
            SupplierFulfillmentOrder::new(SupplierFulfillmentOrderId::new("order-5"), blank_address).is_err()
        );
    }

    #[test]
    fn new_rejects_missing_timestamps_for_progressed_status() {
        let submitting_without_time = SupplierFulfillmentOrderData {
            fulfillment_status: FulfillmentStatus::Submitting,
            ..sample_data()
        };
        assert!(SupplierFulfillmentOrder::new(
            SupplierFulfillmentOrderId::new("order-6"),
            submitting_without_time
        )
        .is_err());

        let completed_without_time = SupplierFulfillmentOrderData {
            fulfillment_status: FulfillmentStatus::Completed,
            submitted_at: Some(Instant::from_unix_secs(1_700_000_000)),
            accepted_at: Some(Instant::from_unix_secs(1_700_000_100)),
            ..sample_data()
        };
        assert!(SupplierFulfillmentOrder::new(
            SupplierFulfillmentOrderId::new("order-7"),
            completed_without_time
        )
        .is_err());
    }

    #[test]
    fn advance_fulfillment_follows_main_line() {
        let mut order = sample_order();
        order.advance_fulfillment(FulfillmentStatus::Submitting).unwrap();
        assert_eq!(order.fulfillment_status, FulfillmentStatus::Submitting);
        assert!(order.submitted_at.is_some());

        order.advance_fulfillment(FulfillmentStatus::Accepted).unwrap();
        assert!(order.accepted_at.is_some());

        order.advance_fulfillment(FulfillmentStatus::Fulfilling).unwrap();
        order.advance_fulfillment(FulfillmentStatus::Shipped).unwrap();
        order.advance_fulfillment(FulfillmentStatus::Completed).unwrap();
        assert!(order.completed_at.is_some());
        assert_eq!(order.fulfillment_status, FulfillmentStatus::Completed);

        let completed_at = order.completed_at;
        order.advance_fulfillment(FulfillmentStatus::Completed).unwrap();
        assert_eq!(
            order.completed_at, completed_at,
            "重复回调的幂等迁移不覆盖里程碑时间"
        );
    }

    #[test]
    fn advance_fulfillment_rejects_illegal_and_regressive_transitions() {
        let mut order = sample_order();
        assert!(
            order.advance_fulfillment(FulfillmentStatus::Accepted).is_err(),
            "跳过 SUBMITTING 非法"
        );

        order.advance_fulfillment(FulfillmentStatus::Submitting).unwrap();
        order.advance_fulfillment(FulfillmentStatus::Rejected).unwrap();
        assert!(
            order.advance_fulfillment(FulfillmentStatus::Submitting).is_err(),
            "REJECTED 是终态"
        );
        assert!(order.advance_fulfillment(FulfillmentStatus::Exception).is_err());
    }

    #[test]
    fn cancel_status_progresses_and_is_terminal() {
        let mut order = sample_order();
        order.advance_cancel(CancelStatus::CancelPending).unwrap();
        order.advance_cancel(CancelStatus::Canceled).unwrap();
        assert_eq!(order.cancel_status, CancelStatus::Canceled);

        let mut failed = sample_order();
        failed.advance_cancel(CancelStatus::CancelPending).unwrap();
        failed.advance_cancel(CancelStatus::Failed).unwrap();
        assert_eq!(failed.cancel_status, CancelStatus::Failed);
    }

    #[test]
    fn refund_status_progresses_and_is_terminal() {
        let mut order = sample_order();
        order.advance_refund(RefundStatus::RefundPending).unwrap();
        order.advance_refund(RefundStatus::Partial).unwrap();
        order.advance_refund(RefundStatus::Refunded).unwrap();
        assert_eq!(order.refund_status, RefundStatus::Refunded);

        let mut failed = sample_order();
        failed.advance_refund(RefundStatus::RefundPending).unwrap();
        failed.advance_refund(RefundStatus::RefundFailed).unwrap();
        assert_eq!(failed.refund_status, RefundStatus::RefundFailed);

        let mut after_partial = sample_order();
        after_partial.advance_refund(RefundStatus::RefundPending).unwrap();
        after_partial.advance_refund(RefundStatus::Partial).unwrap();
        after_partial.advance_refund(RefundStatus::Manual).unwrap();
        assert_eq!(after_partial.refund_status, RefundStatus::Manual);
    }

    #[test]
    fn submitting_factory_freezes_initial_states_and_version_guard() {
        let data = SupplierFulfillmentOrderData::submitting(
            "order-2",
            SupplierAccountId::new("supplier-1"),
            SupplierApiConnectionId::new("connection-1"),
            1,
            Instant::from_unix_secs(1_700_000_000),
            "encrypted",
            "fingerprint",
        );
        assert_eq!(data.fulfillment_status, FulfillmentStatus::Submitting);
        assert_eq!(data.cancel_status, CancelStatus::None);
        assert_eq!(data.refund_status, RefundStatus::None);
        let order = SupplierFulfillmentOrder::new(SupplierFulfillmentOrderId::new("order-2"), data).unwrap();
        assert!(order.ensure_version(order.base.version).is_ok());
        assert!(order.ensure_version(order.base.version + 1).is_err());
    }

    #[test]
    fn verified_resolution_requires_consistent_order_and_action_status() {
        let mut order = sample_order();
        order.advance_fulfillment(FulfillmentStatus::Submitting).unwrap();
        order.advance_fulfillment(FulfillmentStatus::Accepted).unwrap();
        let succeeded = place_action(SupplierOrderActionStatus::Succeeded);
        assert_eq!(
            order.verified_resolution(&succeeded),
            Some(VerifiedSupplierOrderResolution::OrderAccepted)
        );

        let unknown = place_action(SupplierOrderActionStatus::ResultUnknown);
        order.fulfillment_status = FulfillmentStatus::ResultUnknown;
        assert!(order.can_replay_place_action(&unknown));
        assert!(order.verified_resolution(&unknown).is_none());
    }

    #[test]
    fn settlement_fact_access_requires_consistent_terminal_states() {
        let mut order = sample_order();
        assert_eq!(order.confirmed_completed_at().unwrap(), None);
        assert!(order.ensure_canceled().is_err());

        order.advance_cancel(CancelStatus::CancelPending).unwrap();
        order.advance_cancel(CancelStatus::Canceled).unwrap();
        order.ensure_canceled().unwrap();

        order.completed_at = Some(Instant::from_unix_secs(1_700_000_200));
        assert!(order.confirmed_completed_at().is_err());
        order.fulfillment_status = FulfillmentStatus::Completed;
        assert_eq!(
            order.confirmed_completed_at().unwrap(),
            Some(Instant::from_unix_secs(1_700_000_200))
        );
    }

    #[test]
    fn debug_redacts_address_snapshot() {
        let debug = format!("{:?}", sample_order());
        assert!(
            !debug.contains("encrypted-address"),
            "Debug 不得输出地址快照加密值"
        );
        assert!(!debug.contains("fingerprint-address"), "Debug 不得输出查询指纹");
        assert!(debug.contains("<redacted>"));
    }

    #[test]
    fn fingerprint_is_stable_and_keyed() {
        let first = fingerprint("  上海市浦东新区世纪大道 100 号  ", b"key-1");
        let second = fingerprint("上海市浦东新区世纪大道 100 号", b"key-1");
        assert_eq!(first, second, "指纹只依赖明文与密钥，首尾空白不参与");
        assert_eq!(first.len(), 64, "HMAC-SHA256 十六进制为 64 字符");

        let other_key = fingerprint("上海市浦东新区世纪大道 100 号", b"key-2");
        assert_ne!(first, other_key, "不同密钥必须产生不同指纹");
    }

    #[test]
    fn update_sets_external_order_no() {
        let mut order = sample_order();
        order
            .update(SupplierFulfillmentOrderUpdate {
                external_order_no: Some(" SUP-1001 ".to_string()),
            })
            .unwrap();
        assert_eq!(order.external_order_no.as_deref(), Some("SUP-1001"));

        assert!(order
            .update(SupplierFulfillmentOrderUpdate {
                external_order_no: Some("  ".to_string()),
            })
            .is_err());
        assert!(order
            .update(SupplierFulfillmentOrderUpdate {
                external_order_no: Some("x".repeat(65)),
            })
            .is_err());
    }
}
