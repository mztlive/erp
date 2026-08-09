//! `supplier_fulfillment_order` 与 `supplier_fulfillment_item`（数据模型 §6.19 供应商履约订单与明细）。
//!
//! 履约主线、取消与退款是三条正交状态机（§7.6），`CANCEL_PENDING`/`CANCELED`/
//! `REFUND_PENDING`/`REFUNDED` 不得折叠为单一状态枚举（§6.19）；COMPLETED/REJECTED 是
//! 终态，乱序或重复回调经 [`ensure_transition`] 拒绝（从高状态回低状态即非法迁移）。

use std::fmt;

use entity_core::BaseModel;
use entity_macros::Entity;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::common::state::{ensure_transition, DocumentState};
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{
    MallOrderId, MallOrderItemId, SupplierAccountId, SupplierApiConnectionId, SupplierFulfillmentItemId,
    SupplierFulfillmentOrderId, SupplierOfferingRevisionId,
};
use crate::money::{round_to_cent, Amount, Quantity, Rate, UnitPrice};
use crate::validation::{normalize_optional_text, normalize_required_text};

/// 供应商子订单号（ERP 下单幂等键）最大长度。
const ORDER_NO_MAX_LEN: usize = 64;
/// 外部（供应商）订单号最大长度。
const EXTERNAL_ORDER_NO_MAX_LEN: usize = 64;
/// 履约地址快照加密值最大长度。
const ADDRESS_ENCRYPTED_MAX_LEN: usize = 8192;
/// 履约地址快照 HMAC 查询指纹最大长度。
const ADDRESS_FINGERPRINT_MAX_LEN: usize = 128;
/// 供应商侧商品与 SKU 编码快照最大长度。
const SUPPLIER_ITEM_CODE_MAX_LEN: usize = 128;

/// 供应商履约主线状态（数据模型 §6.19、§7.6）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FulfillmentStatus {
    /// 接收：已形成子订单，尚未提交供应商。
    Received,
    /// 提交中：下单请求已发出，等待接单结果。
    Submitting,
    /// 已接单：供应商明确接单。
    Accepted,
    /// 明确拒绝：供应商明确拒单，终态；退款/余额恢复闭环由 refund_status 与退款事实表达。
    Rejected,
    /// 结果未知：网络超时或查询能力不足，先查询原请求，不盲目重复下单。
    ResultUnknown,
    /// 履约中：供应商履约中（含已发货）。
    Fulfilling,
    /// 已发货：适用时（存在发货阶段的履约）。
    Shipped,
    /// 已完成：终态。
    Completed,
    /// 异常：需人工查询或补偿；恢复动作以追加事实表达，不直接改状态。
    Exception,
}

impl FulfillmentStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Received => "接收",
            Self::Submitting => "提交中",
            Self::Accepted => "已接单",
            Self::Rejected => "明确拒绝",
            Self::ResultUnknown => "结果未知",
            Self::Fulfilling => "履约中",
            Self::Shipped => "已发货",
            Self::Completed => "已完成",
            Self::Exception => "异常",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Received => "RECEIVED",
            Self::Submitting => "SUBMITTING",
            Self::Accepted => "ACCEPTED",
            Self::Rejected => "REJECTED",
            Self::ResultUnknown => "RESULT_UNKNOWN",
            Self::Fulfilling => "FULFILLING",
            Self::Shipped => "SHIPPED",
            Self::Completed => "COMPLETED",
            Self::Exception => "EXCEPTION",
        }
    }

    /// 判断是否处于终态。
    ///
    /// # 返回
    /// `COMPLETED`、`REJECTED` 或 `EXCEPTION` 时返回 `true`。
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Rejected | Self::Exception)
    }
}

impl DocumentState for FulfillmentStatus {
    /// 返回全部合法后继状态（数据模型 §7.6，禁止运行时扩展）。
    ///
    /// # 返回
    /// 后继状态切片（不含自身；终态返回空）。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Received => &[Self::Submitting, Self::Exception],
            Self::Submitting => &[
                Self::Accepted,
                Self::Rejected,
                Self::ResultUnknown,
                Self::Exception,
            ],
            Self::Accepted => &[Self::Fulfilling, Self::Exception],
            Self::Fulfilling => &[Self::Shipped, Self::Completed, Self::Exception],
            Self::Shipped => &[Self::Completed, Self::Exception],
            Self::ResultUnknown => &[Self::Accepted, Self::Rejected, Self::Exception],
            Self::Completed | Self::Rejected | Self::Exception => &[],
        }
    }
}

/// 供应商履约取消进度状态（数据模型 §6.19、§7.6，独立于履约主线的正交状态）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CancelStatus {
    /// 无。
    None,
    /// 取消中。
    CancelPending,
    /// 已取消：终态。
    Canceled,
    /// 取消失败：终态。
    Failed,
    /// 待人工：终态。
    Manual,
}

impl CancelStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::None => "无",
            Self::CancelPending => "取消中",
            Self::Canceled => "已取消",
            Self::Failed => "取消失败",
            Self::Manual => "待人工",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "NONE",
            Self::CancelPending => "CANCEL_PENDING",
            Self::Canceled => "CANCELED",
            Self::Failed => "FAILED",
            Self::Manual => "MANUAL",
        }
    }
}

impl DocumentState for CancelStatus {
    /// 返回全部合法后继状态（数据模型 §7.6：NONE→CANCEL_PENDING→CANCELED|FAILED|MANUAL）。
    ///
    /// # 返回
    /// 后继状态切片（不含自身；终态返回空）。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::None => &[Self::CancelPending],
            Self::CancelPending => &[Self::Canceled, Self::Failed, Self::Manual],
            Self::Canceled | Self::Failed | Self::Manual => &[],
        }
    }
}

/// 供应商履约退款进度状态（数据模型 §6.19、§7.6，独立于履约主线的正交状态）。
///
/// 主线 `NONE → REFUND_PENDING → PARTIAL → REFUNDED`；`REFUND_PENDING` 可分支到
/// `REFUND_FAILED`/`MANUAL`。多次部分退款表达为 `PARTIAL` 的幂等停留（新退款请求的
/// 在途状态由 `supplier_order_action` 的 REFUND 动作承载），`PARTIAL` 之后的退款失败
/// 进入 `REFUND_FAILED`、人工接手进入 `MANUAL`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RefundStatus {
    /// 无。
    None,
    /// 退款中。
    RefundPending,
    /// 部分退款。
    Partial,
    /// 全部退款：终态。
    Refunded,
    /// 退款失败：终态。
    RefundFailed,
    /// 待人工：终态。
    Manual,
}

impl RefundStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::None => "无",
            Self::RefundPending => "退款中",
            Self::Partial => "部分退款",
            Self::Refunded => "全部退款",
            Self::RefundFailed => "退款失败",
            Self::Manual => "待人工",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "NONE",
            Self::RefundPending => "REFUND_PENDING",
            Self::Partial => "PARTIAL",
            Self::Refunded => "REFUNDED",
            Self::RefundFailed => "REFUND_FAILED",
            Self::Manual => "MANUAL",
        }
    }
}

impl DocumentState for RefundStatus {
    /// 返回全部合法后继状态（数据模型 §7.6，禁止运行时扩展）。
    ///
    /// # 返回
    /// 后继状态切片（不含自身；终态返回空）。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::None => &[Self::RefundPending],
            Self::RefundPending => &[Self::Partial, Self::Refunded, Self::RefundFailed, Self::Manual],
            Self::Partial => &[Self::Refunded, Self::RefundFailed, Self::Manual],
            Self::Refunded | Self::RefundFailed | Self::Manual => &[],
        }
    }
}

/// 供应商子订单创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierFulfillmentOrderData {
    /// ERP 供应商子订单号（唯一，也是下单幂等键）。
    pub fulfillment_order_no: String,
    /// 来源商城订单。
    pub mall_order_id: MallOrderId,
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

/// 供应商子订单更新数据（不含系统字段与关键字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SupplierFulfillmentOrderUpdate {
    /// 供应商订单号；`None` 表示不修改。状态推进走 `advance_*` 系列方法。
    pub external_order_no: Option<String>,
}

/// 供应商子订单实体（数据模型 §6.19，正式单据）。
///
/// `fulfillment_order_no`、`mall_order_id`、`supplier_id`、`connection_id`、`split_no`
/// 与三条状态是创建后不可修改的关键字段；地址快照为敏感值，`Debug` 一律脱敏。
#[derive(Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SupplierFulfillmentOrder {
    #[serde(flatten)]
    pub base: BaseModel,
    /// ERP 供应商子订单号。
    pub fulfillment_order_no: String,
    /// 来源商城订单。
    pub mall_order_id: MallOrderId,
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
            mall_order_id: data.mall_order_id,
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
}

impl fmt::Debug for SupplierFulfillmentOrder {
    /// 脱敏调试输出：地址快照加密值与查询指纹只输出 `<redacted>`，
    /// 禁止在日志中泄漏敏感值（数据模型 §4.5.5）。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SupplierFulfillmentOrder")
            .field("base", &self.base)
            .field("fulfillment_order_no", &self.fulfillment_order_no)
            .field("mall_order_id", &self.mall_order_id)
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

/// 供应商履约明细创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierFulfillmentItemData {
    /// 所属供应商子订单。
    pub supplier_fulfillment_order_id: SupplierFulfillmentOrderId,
    /// 来源商城商品明细。
    pub mall_order_item_id: MallOrderItemId,
    /// 下单时固定的供给修订。
    pub supplier_offering_revision_id: SupplierOfferingRevisionId,
    /// 下单时固定的供应商侧订货 SKU 编码。
    pub supplier_sku_code_snapshot: String,
    /// 下单时固定的供应商侧商品编码。
    pub supplier_product_code_snapshot: Option<String>,
    /// 整条明细数量（SKU 基础单位，最多 6 位小数）。
    pub quantity: Quantity,
    /// 下单含税单位成本快照（最多 4 位小数）。
    pub unit_cost_snapshot_gross: UnitPrice,
    /// 明细含税成本快照（= 单位成本 × 数量，按分舍入）。
    pub cost_snapshot_total_gross: Amount,
    /// 下单成本进项税率（最多 6 位小数）。
    pub input_tax_rate: Rate,
}

/// 供应商履约明细实体（数据模型 §6.19）。
///
/// 随子订单同事务创建，创建后不可修改（后续供给关系变化不影响已支付订单）；
/// 一条商城商品明细只属于一个供应商子订单（跨记录唯一性由唯一索引保证，P3）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SupplierFulfillmentItem {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属供应商子订单。
    pub supplier_fulfillment_order_id: SupplierFulfillmentOrderId,
    /// 来源商城商品明细。
    pub mall_order_item_id: MallOrderItemId,
    /// 下单时固定的供给修订。
    pub supplier_offering_revision_id: SupplierOfferingRevisionId,
    /// 下单时固定的供应商侧订货 SKU 编码。
    pub supplier_sku_code_snapshot: String,
    /// 下单时固定的供应商侧商品编码。
    pub supplier_product_code_snapshot: Option<String>,
    /// 整条明细数量。
    pub quantity: Quantity,
    /// 下单含税单位成本快照。
    pub unit_cost_snapshot_gross: UnitPrice,
    /// 明细含税成本快照。
    pub cost_snapshot_total_gross: Amount,
    /// 下单成本进项税率。
    pub input_tax_rate: Rate,
}

impl SupplierFulfillmentItem {
    /// 创建供应商履约明细。
    ///
    /// 校验数量大于零，并强制成本快照恒等（§4.2：明细含税成本必须等于
    /// 单位成本 × 数量后按分舍入）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SupplierFulfillmentItemId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的明细实体。
    ///
    /// # 错误
    /// 数量非正、成本快照与单价数量不一致或金额精度非法时返回错误。
    pub fn new(id: SupplierFulfillmentItemId, data: SupplierFulfillmentItemData) -> Result<Self> {
        ensure_positive_quantity(data.quantity, "明细数量必须大于零")?;
        ensure_snapshot_consistent(
            data.quantity,
            data.unit_cost_snapshot_gross,
            data.cost_snapshot_total_gross,
        )?;
        let supplier_sku_code_snapshot = normalize_required_text(
            data.supplier_sku_code_snapshot,
            "供应商 SKU 编码快照不能为空",
            SUPPLIER_ITEM_CODE_MAX_LEN,
            "供应商 SKU 编码快照过长",
        )?;
        let supplier_product_code_snapshot = normalize_optional_text(
            data.supplier_product_code_snapshot,
            "供应商商品编码快照",
            SUPPLIER_ITEM_CODE_MAX_LEN,
        )?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            supplier_fulfillment_order_id: data.supplier_fulfillment_order_id,
            mall_order_item_id: data.mall_order_item_id,
            supplier_offering_revision_id: data.supplier_offering_revision_id,
            supplier_sku_code_snapshot,
            supplier_product_code_snapshot,
            quantity: data.quantity,
            unit_cost_snapshot_gross: data.unit_cost_snapshot_gross,
            cost_snapshot_total_gross: data.cost_snapshot_total_gross,
            input_tax_rate: data.input_tax_rate,
        })
    }
}

/// 校验数量大于零。
///
/// # 参数
/// * `value` - 数量
/// * `message` - 失败时的错误信息
///
/// # 错误
/// 数量小于等于零时返回错误。
fn ensure_positive_quantity(value: Quantity, message: &str) -> Result<()> {
    if value.to_decimal() <= Decimal::ZERO {
        return Err(Error::from(message));
    }
    Ok(())
}

/// 校验明细成本快照恒等：`round_to_cent(单位成本 × 数量) == 明细成本`（§4.2 铁律 1）。
///
/// # 参数
/// * `quantity` - 数量
/// * `unit_cost` - 含税单位成本
/// * `total_gross` - 明细含税成本
///
/// # 错误
/// 恒等式不成立或舍入结果无法表示为 `Amount` 时返回错误。
fn ensure_snapshot_consistent(quantity: Quantity, unit_cost: UnitPrice, total_gross: Amount) -> Result<()> {
    let expected = Amount::try_from(round_to_cent(unit_cost.to_decimal() * quantity.to_decimal()))
        .map_err(|_| Error::from("明细成本快照金额无效"))?;
    if expected != total_gross {
        return Err(Error::from("明细成本快照必须等于下单单位成本 × 数量并舍入到分"));
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
    use hmac::{Hmac, Mac};
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
    use crate::common::state::ensure_transition;
    use std::str::FromStr;

    fn sample_data() -> SupplierFulfillmentOrderData {
        SupplierFulfillmentOrderData {
            fulfillment_order_no: " FO-2026-001 ".to_string(),
            mall_order_id: MallOrderId::new("mall-order-1"),
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

    fn sample_item_data() -> SupplierFulfillmentItemData {
        SupplierFulfillmentItemData {
            supplier_fulfillment_order_id: SupplierFulfillmentOrderId::new("order-1"),
            mall_order_item_id: MallOrderItemId::new("mall-item-1"),
            supplier_offering_revision_id: SupplierOfferingRevisionId::new("offering-rev-1"),
            supplier_sku_code_snapshot: "SUP-SKU-1".to_string(),
            supplier_product_code_snapshot: Some("SUP-SPU-1".to_string()),
            quantity: Quantity::from_str("3.000000").unwrap(),
            unit_cost_snapshot_gross: UnitPrice::from_str("9.9900").unwrap(),
            cost_snapshot_total_gross: Amount::from_str("29.97").unwrap(),
            input_tax_rate: Rate::from_str("0.130000").unwrap(),
        }
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
    fn terminal_states_are_absorbing() {
        let mut order = sample_order();
        order.advance_fulfillment(FulfillmentStatus::Submitting).unwrap();
        order.advance_fulfillment(FulfillmentStatus::Rejected).unwrap();
        assert!(FulfillmentStatus::Rejected.allowed_next().is_empty());
        assert!(FulfillmentStatus::Completed.allowed_next().is_empty());
        assert!(FulfillmentStatus::Exception.allowed_next().is_empty());
        assert!(FulfillmentStatus::Rejected.is_terminal());
        assert!(FulfillmentStatus::Completed.is_terminal());

        for (from, to) in [
            (FulfillmentStatus::Completed, FulfillmentStatus::Fulfilling),
            (FulfillmentStatus::Completed, FulfillmentStatus::Exception),
            (FulfillmentStatus::Rejected, FulfillmentStatus::Received),
            (FulfillmentStatus::Exception, FulfillmentStatus::Accepted),
        ] {
            let error = ensure_transition(from, to).unwrap_err();
            assert!(
                matches!(error, Error::InvalidStateTransition { .. }),
                "终态定向断言：{from:?} → {to:?} 必须被拒绝"
            );
        }
    }

    #[test]
    fn exception_branches_and_result_unknown_resolution() {
        for start in [
            FulfillmentStatus::Received,
            FulfillmentStatus::Submitting,
            FulfillmentStatus::Accepted,
            FulfillmentStatus::Fulfilling,
            FulfillmentStatus::Shipped,
            FulfillmentStatus::ResultUnknown,
        ] {
            assert!(
                ensure_transition(start, FulfillmentStatus::Exception).is_ok(),
                "任一可恢复节点可进入 EXCEPTION：{start:?}"
            );
        }

        assert!(ensure_transition(FulfillmentStatus::Submitting, FulfillmentStatus::ResultUnknown).is_ok());
        for to in [
            FulfillmentStatus::Accepted,
            FulfillmentStatus::Rejected,
            FulfillmentStatus::Exception,
        ] {
            assert!(
                ensure_transition(FulfillmentStatus::ResultUnknown, to).is_ok(),
                "RESULT_UNKNOWN 可解析为 {to:?}"
            );
        }
        assert!(ensure_transition(FulfillmentStatus::Submitting, FulfillmentStatus::Rejected).is_ok());
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

        assert!(
            ensure_transition(CancelStatus::None, CancelStatus::Canceled).is_err(),
            "跳过 CANCEL_PENDING 非法"
        );
        assert!(
            ensure_transition(CancelStatus::Canceled, CancelStatus::Manual).is_err(),
            "CANCELED 是终态"
        );
        assert!(
            ensure_transition(CancelStatus::Canceled, CancelStatus::None).is_err(),
            "取消进度不得倒退"
        );
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

        assert!(
            ensure_transition(RefundStatus::None, RefundStatus::Refunded).is_err(),
            "跳过 REFUND_PENDING 非法"
        );
        assert!(
            ensure_transition(RefundStatus::Refunded, RefundStatus::Partial).is_err(),
            "REFUNDED 是终态"
        );
        assert!(
            ensure_transition(RefundStatus::RefundFailed, RefundStatus::None).is_err(),
            "退款进度不得倒退"
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

    #[test]
    fn item_new_accepts_valid_item_and_keeps_snapshot() {
        let item = SupplierFulfillmentItem::new(SupplierFulfillmentItemId::new("item-1"), sample_item_data())
            .unwrap();

        assert_eq!(item.quantity, Quantity::from_str("3.000000").unwrap());
        assert_eq!(item.cost_snapshot_total_gross, Amount::from_str("29.97").unwrap());
        assert_eq!(item.input_tax_rate, Rate::from_str("0.130000").unwrap());
        assert_eq!(item.supplier_sku_code_snapshot, "SUP-SKU-1");
        assert_eq!(item.supplier_product_code_snapshot.as_deref(), Some("SUP-SPU-1"));
    }

    #[test]
    fn item_new_rejects_blank_supplier_sku_snapshot() {
        let data = SupplierFulfillmentItemData {
            supplier_sku_code_snapshot: "   ".to_string(),
            ..sample_item_data()
        };
        assert!(SupplierFulfillmentItem::new(SupplierFulfillmentItemId::new("item-blank"), data).is_err());
    }

    #[test]
    fn item_new_rejects_non_positive_quantity() {
        for quantity in [
            Quantity::from_str("0.000000").unwrap(),
            Quantity::from_str("-1.000000").unwrap(),
        ] {
            let data = SupplierFulfillmentItemData {
                quantity,
                ..sample_item_data()
            };
            assert!(SupplierFulfillmentItem::new(SupplierFulfillmentItemId::new("item-2"), data).is_err());
        }
    }

    #[test]
    fn item_new_rejects_inconsistent_cost_snapshot() {
        let data = SupplierFulfillmentItemData {
            cost_snapshot_total_gross: Amount::from_str("30.00").unwrap(),
            ..sample_item_data()
        };
        assert!(SupplierFulfillmentItem::new(SupplierFulfillmentItemId::new("item-3"), data).is_err());
    }

    #[test]
    fn item_new_rejects_over_scale_money() {
        assert!(
            UnitPrice::from_str("9.99999").is_err(),
            "单价最多 4 位小数，超位必须拒绝"
        );
        assert!(
            Quantity::from_str("1.0000001").is_err(),
            "数量最多 6 位小数，超位必须拒绝"
        );
    }
}
