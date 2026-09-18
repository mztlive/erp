use application_core::non_blank;
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::super::SupplySourceType;
use crate::entity::purchase_order::{
    PurchaseType, RequestedLine, SourcingAssignment, SourcingAssignmentSet, digest_parts,
    normalize_requested_lines, payload_fingerprint,
};
use crate::{Error, Result};

/// 依据创建命令的服务端固定审计动作（同时参与请求指纹）。
pub const CREATE_ACTION: &str = "purchase_order.create_from_basis";
/// 选源创建命令的服务端固定审计动作（同时参与请求指纹）。
pub const CREATE_SOURCING_ACTION: &str = "purchase_order.create_from_sourcing";

/// 依据建单的单行本次数量。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CreatePurchaseOrderLineRequest {
    /// 销售稳定行身份。
    #[validate(custom(function = "non_blank", message = "销售行不能为空"))]
    pub sales_order_line_id: String,
    /// 本次创建数量；事务内必须大于零且不超过最新可创建数量。
    #[validate(custom(function = "non_blank", message = "本次数量不能为空"))]
    pub quantity: String,
    /// 采购确认的预计交付日，不得晚于销售承诺期限。
    #[validate(custom(function = "non_blank", message = "预计交付日不能为空"))]
    pub expected_delivery_date: String,
}

/// 依据创建采购单请求（精确拆单维度 + 逐行本次数量）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CreatePurchaseOrderFromBasisRequest {
    /// 冻结本次销售行责任范围的开放供给分配任务。
    #[validate(custom(function = "non_blank", message = "供给分配任务不能为空"))]
    pub work_item_id: String,
    /// 采购创建依据（当前任务范围内的精确供应商拆分）。
    #[validate(custom(function = "non_blank", message = "创建依据不能为空"))]
    pub basis_id: String,
    /// 采购类型。
    pub purchase_type: PurchaseType,
    /// 付款条件（受控码表代码）。
    #[validate(custom(function = "non_blank", message = "付款条件不能为空"))]
    pub payment_term_code: String,
    /// 仓库履约的目标收货仓；非仓库履约必须为空。
    #[serde(default)]
    #[validate(length(min = 1, max = 128, message = "目标仓库长度必须在1-128个字符之间"))]
    pub target_warehouse_id: Option<String>,
    /// 本次采购明细；允许只创建依据中的部分行或部分数量。
    #[validate(length(min = 1, max = 200, message = "本次采购明细必须在1-200行之间"), nested)]
    pub lines: Vec<CreatePurchaseOrderLineRequest>,
    /// 幂等键（同一命令重复创建返回同一采购单）。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

impl CreatePurchaseOrderFromBasisRequest {
    /// 规范化并校验逐行本次采购数量。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回去除首尾空白、数量已类型化且稳定行不重复的请求行。
    ///
    /// # 错误
    /// 稳定行重复、数量非法或数量不大于零时返回参数验证错误。
    ///
    /// # 关键业务约束
    /// 同一稳定销售行在一次命令中只能出现一次；字符串类型化与集合规则由
    /// `crate::entity::purchase_order::creation_basis` 承担，本方法只负责协议错误映射。
    pub fn normalized_lines(&self) -> Result<Vec<RequestedLine>> {
        let mut parsed = Vec::with_capacity(self.lines.len());
        for line in &self.lines {
            parsed.push(
                RequestedLine::parse(&line.sales_order_line_id, &line.quantity, &line.expected_delivery_date)
                    .map_err(|error| Error::ValidationError(error.to_string()))?,
            );
        }
        normalize_requested_lines(&parsed).map_err(|error| Error::ValidationError(error.to_string()))
    }

    /// 计算创建命令请求指纹（不含原始幂等键）。
    ///
    /// # 参数
    /// * `lines` - 已规范化并排序的请求行
    ///
    /// # 返回
    /// 返回不包含原始幂等键的 SHA-256 指纹。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 同一幂等键用于不同依据、范围或数量时必须冲突；摘要形态与存量收据一致，
    /// 修改会破坏幂等兼容。
    pub fn request_fingerprint(&self, lines: &[RequestedLine]) -> String {
        let mut parts = vec![
            self.work_item_id.trim().to_string(),
            self.basis_id.trim().to_string(),
            self.purchase_type.as_str().to_string(),
            self.payment_term_code.trim().to_string(),
            self.target_warehouse_id.as_deref().map(str::trim).unwrap_or_default().to_string(),
        ];
        parts.extend(lines.iter().map(|line| {
            format!("{}|{}|{}", line.sales_order_line_id, line.quantity, line.expected_delivery_date)
        }));
        digest_parts(parts)
    }
}

/// 创建采购单结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CreatePurchaseOrderResult {
    /// 采购单主键。
    pub purchase_order_id: String,
    /// 采购单号。
    pub purchase_no: String,
    /// 乐观锁版本。
    pub lock_version: u64,
    /// 是否复用已有创建结果（幂等重放）。
    pub replayed: bool,
    /// 业务引用。
    pub reference: String,
}

impl CreatePurchaseOrderResult {
    /// 由必填采购单身份与业务引用构造创建结果。
    ///
    /// # 参数
    /// * `purchase_order_id` - 采购单主键
    /// * `purchase_no` - 采购单号
    /// * `reference` - 业务引用
    ///
    /// # 返回
    /// 返回零版本、非重放的创建结果。
    ///
    /// # 错误
    /// 无。
    pub fn new(
        purchase_order_id: impl Into<String>,
        purchase_no: impl Into<String>,
        reference: impl Into<String>,
    ) -> Self {
        Self {
            purchase_order_id: purchase_order_id.into(),
            purchase_no: purchase_no.into(),
            lock_version: 0,
            replayed: false,
            reference: reference.into(),
        }
    }

    /// 设置乐观锁版本。
    ///
    /// # 参数
    /// * `lock_version` - 乐观锁版本
    ///
    /// # 返回
    /// 返回更新后的创建结果。
    ///
    /// # 错误
    /// 无。
    pub fn with_lock_version(mut self, lock_version: u64) -> Self {
        self.lock_version = lock_version;
        self
    }

    /// 设置是否为幂等重放。
    ///
    /// # 参数
    /// * `replayed` - 是否复用已有创建结果
    ///
    /// # 返回
    /// 返回更新后的创建结果。
    ///
    /// # 错误
    /// 无。
    pub fn with_replayed(mut self, replayed: bool) -> Self {
        self.replayed = replayed;
        self
    }
}

/// 供给分配的一条精确来源与数量。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct SourcingLineAssignment {
    /// 销售稳定行身份。
    #[validate(custom(function = "non_blank", message = "销售行不能为空"))]
    pub sales_order_line_id: String,
    /// 本行选用的精确依据，绑定库存余额或采购供给及其履约责任。
    #[validate(custom(function = "non_blank", message = "履约方案不能为空"))]
    pub basis_id: String,
    /// 供给来源；旧客户端缺省为供应商采购。
    #[serde(default)]
    pub source_type: SupplySourceType,
    /// 采购且由仓库履约时的目标收货仓；其他供给来源必须为空。
    #[serde(default)]
    #[validate(length(min = 1, max = 128, message = "目标仓库长度必须在1-128个字符之间"))]
    pub target_warehouse_id: Option<String>,
    /// 本次分配数量；事务内必须大于零且不超过最新可分配数量。
    #[validate(custom(function = "non_blank", message = "本次分配数量不能为空"))]
    pub quantity: String,
    /// 预计交付日；采购来源不得晚于销售承诺期限，库存来源保留同一请求形状。
    #[validate(custom(function = "non_blank", message = "预计交付日不能为空"))]
    pub expected_delivery_date: String,
}

impl SourcingLineAssignment {
    /// 由必填销售行、依据、数量与预计交付日构造选源分配。
    ///
    /// # 参数
    /// * `sales_order_line_id` - 销售稳定行身份
    /// * `basis_id` - 本行选用的精确依据
    /// * `quantity` - 本次分配数量
    /// * `expected_delivery_date` - 预计交付日
    ///
    /// # 返回
    /// 返回供应商采购来源、目标仓为空的选源分配。
    ///
    /// # 错误
    /// 无。
    pub fn new(
        sales_order_line_id: impl Into<String>,
        basis_id: impl Into<String>,
        quantity: impl Into<String>,
        expected_delivery_date: impl Into<String>,
    ) -> Self {
        Self {
            sales_order_line_id: sales_order_line_id.into(),
            basis_id: basis_id.into(),
            source_type: SupplySourceType::Purchase,
            target_warehouse_id: None,
            quantity: quantity.into(),
            expected_delivery_date: expected_delivery_date.into(),
        }
    }

    /// 设置供给来源。
    ///
    /// # 参数
    /// * `source_type` - 供给来源
    ///
    /// # 返回
    /// 返回更新后的选源分配。
    ///
    /// # 错误
    /// 无。
    pub fn with_source_type(mut self, source_type: SupplySourceType) -> Self {
        self.source_type = source_type;
        self
    }

    /// 设置采购且由仓库履约时的目标收货仓。
    ///
    /// # 参数
    /// * `target_warehouse_id` - 目标收货仓
    ///
    /// # 返回
    /// 返回更新后的选源分配。
    ///
    /// # 错误
    /// 无。
    pub fn with_target_warehouse_id(mut self, target_warehouse_id: impl Into<String>) -> Self {
        self.target_warehouse_id = Some(target_warehouse_id.into());
        self
    }
}

/// 一次确认库存与采购供给分配的请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CreatePurchaseOrdersFromSourcingRequest {
    /// 冻结本次销售行责任范围的开放供给分配任务。
    #[validate(custom(function = "non_blank", message = "供给分配任务不能为空"))]
    pub work_item_id: String,
    /// 来源销售单。
    #[validate(custom(function = "non_blank", message = "销售单不能为空"))]
    pub sales_order_id: String,
    /// 已选定的供给分配；同一销售行允许按库存与采购依据拆分。
    #[validate(length(min = 1, max = 200, message = "本次供给分配明细必须在1-200行之间"), nested)]
    pub lines: Vec<SourcingLineAssignment>,
    /// 幂等键（同一命令重复提交时返回原库存预占与采购单）。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

impl CreatePurchaseOrdersFromSourcingRequest {
    /// 规范化并校验逐行选源分配。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回字符串已类型化、同销售行同依据不重复且稳定排序的选源集合。
    ///
    /// # 错误
    /// 销售行或依据空白、数量或预计交付日非法、现有库存另行指定目标仓、
    /// 数量不大于零或同一销售行重复使用同一依据时返回参数验证错误。
    ///
    /// # 关键业务约束
    /// 字符串类型化与集合规则由 `crate::entity::purchase_order::sourcing_plan`
    /// 承担，本方法只负责协议错误映射；同一销售行允许按库存与采购依据拆分。
    pub fn sourcing_assignments(&self) -> Result<SourcingAssignmentSet> {
        let mut parsed = Vec::with_capacity(self.lines.len());
        for line in &self.lines {
            parsed.push(
                SourcingAssignment::parse(
                    &line.sales_order_line_id,
                    &line.basis_id,
                    line.source_type,
                    line.target_warehouse_id.as_deref(),
                    &line.quantity,
                    &line.expected_delivery_date,
                )
                .map_err(|error| Error::ValidationError(error.to_string()))?,
            );
        }
        SourcingAssignmentSet::normalize(&parsed).map_err(|error| Error::ValidationError(error.to_string()))
    }

    /// 计算整批选源创建命令请求指纹（不含原始幂等键）。
    ///
    /// # 参数
    /// * `assignments` - 已规范化并排序的选源行
    ///
    /// # 返回
    /// 返回不包含原始幂等键的 SHA-256 指纹。
    ///
    /// # 错误
    /// 指纹载荷序列化失败时返回内部错误。
    ///
    /// # 关键业务约束
    /// 同一幂等键用于不同任务、销售单、供应商或数量时必须冲突；摘要形态与
    /// 存量收据一致，修改会破坏幂等兼容。
    pub fn request_fingerprint(&self, assignments: &[SourcingAssignment]) -> Result<String> {
        let payload = assignments
            .iter()
            .map(|line| {
                (
                    line.sales_order_line_id.clone(),
                    line.basis_id.clone(),
                    line.source_type,
                    line.target_warehouse_id.clone(),
                    line.quantity.to_string(),
                    line.expected_delivery_date.to_string(),
                )
            })
            .collect::<Vec<_>>();
        payload_fingerprint(
            CREATE_SOURCING_ACTION,
            self.sales_order_id.trim(),
            &(self.work_item_id.trim(), payload),
        )
        .map_err(Into::into)
    }
}

/// 供给分配确认结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CreatePurchaseOrdersFromSourcingResult {
    /// 本次创建并提交或幂等回放的采购单。
    pub orders: Vec<CreatePurchaseOrderResult>,
    /// 本次建立或幂等回放的现有库存销售预占。
    pub stock_reservations: Vec<ExistingStockReservationResult>,
    /// 本次同步后的供给分配任务状态；部分分配固定保持 `OPEN`。
    pub work_item_status: String,
    /// 是否复用已有供给分配结果（幂等重放）。
    pub replayed: bool,
    /// 业务引用，指向来源销售单。
    pub reference: String,
}

impl CreatePurchaseOrdersFromSourcingResult {
    /// 由必填任务状态与业务引用构造选源确认结果。
    ///
    /// # 参数
    /// * `work_item_status` - 本次同步后的供给分配任务状态
    /// * `reference` - 业务引用（来源销售单）
    ///
    /// # 返回
    /// 返回订单与预占为空、非重放的确认结果。
    ///
    /// # 错误
    /// 无。
    pub fn new(work_item_status: impl Into<String>, reference: impl Into<String>) -> Self {
        Self {
            orders: Vec::new(),
            stock_reservations: Vec::new(),
            work_item_status: work_item_status.into(),
            replayed: false,
            reference: reference.into(),
        }
    }

    /// 设置本次创建的采购单。
    ///
    /// # 参数
    /// * `orders` - 本次创建并提交或幂等回放的采购单
    ///
    /// # 返回
    /// 返回更新后的确认结果。
    ///
    /// # 错误
    /// 无。
    pub fn with_orders(mut self, orders: Vec<CreatePurchaseOrderResult>) -> Self {
        self.orders = orders;
        self
    }

    /// 设置本次建立的库存预占。
    ///
    /// # 参数
    /// * `stock_reservations` - 本次建立或幂等回放的现有库存销售预占
    ///
    /// # 返回
    /// 返回更新后的确认结果。
    ///
    /// # 错误
    /// 无。
    pub fn with_stock_reservations(
        mut self,
        stock_reservations: Vec<ExistingStockReservationResult>,
    ) -> Self {
        self.stock_reservations = stock_reservations;
        self
    }

    /// 设置是否为幂等重放。
    ///
    /// # 参数
    /// * `replayed` - 是否复用已有供给分配结果
    ///
    /// # 返回
    /// 返回更新后的确认结果。
    ///
    /// # 错误
    /// 无。
    pub fn with_replayed(mut self, replayed: bool) -> Self {
        self.replayed = replayed;
        self
    }
}

/// 现有库存供给分配结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExistingStockReservationResult {
    /// 库存预占主键。
    pub stock_reservation_id: String,
    /// 销售稳定行主键。
    pub sales_order_line_id: String,
    /// 库存余额主键。
    pub stock_balance_id: String,
    /// 仓库主键。
    pub warehouse_id: String,
    /// 本次预占数量。
    pub quantity: String,
}
