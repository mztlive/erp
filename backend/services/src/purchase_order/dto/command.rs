use std::collections::HashMap;

use entities::purchase_order::{
    digest_parts, normalize_requested_lines, payload_fingerprint, DraftLineEdit, PurchaseLineType,
    PurchaseOrderSubmissionLine, PurchaseType, RequestedLine, SourcingAssignment, SourcingAssignmentSet,
};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::{Error, Result};
use crate::query::non_blank;

use super::query::TotalsView;
use super::SupplySourceType;

/// 作废草稿命令的服务端固定审计动作（同时参与请求指纹）。
pub const VOID_ACTION: &str = "purchase_order.void";
/// 保存草稿命令的服务端固定审计动作（同时参与请求指纹）。
pub const SAVE_ACTION: &str = "purchase_order.update";
/// 依据创建命令的服务端固定审计动作（同时参与请求指纹）。
pub const CREATE_ACTION: &str = "purchase_order.create_from_basis";
/// 选源创建命令的服务端固定审计动作（同时参与请求指纹）。
pub const CREATE_SOURCING_ACTION: &str = "purchase_order.create_from_sourcing";
/// 提交命令的服务端固定审计动作。
pub const PURCHASE_SUBMIT_ACTION: &str = "purchase_order.submit";

/// 撤回采购单审批请求。原因必填。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CancelPurchaseOrderApprovalRequest {
    /// 期望的单据乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_lock_version: u64,
    /// 非空撤回原因。
    #[validate(length(min = 1, max = 512, message = "撤回原因不能为空"))]
    pub reason: String,
    /// 业务请求幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

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
    /// `entities::purchase_order::creation_basis` 承担，本方法只负责协议错误映射。
    pub(crate) fn normalized_lines(&self) -> Result<Vec<RequestedLine>> {
        let mut parsed = Vec::with_capacity(self.lines.len());
        for line in &self.lines {
            parsed.push(
                RequestedLine::parse(
                    &line.sales_order_line_id,
                    &line.quantity,
                    &line.expected_delivery_date,
                )
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
    pub(crate) fn request_fingerprint(&self, lines: &[RequestedLine]) -> String {
        let mut parts = vec![
            self.work_item_id.trim().to_string(),
            self.basis_id.trim().to_string(),
            self.purchase_type.as_str().to_string(),
            self.payment_term_code.trim().to_string(),
            self.target_warehouse_id
                .as_deref()
                .map(str::trim)
                .unwrap_or_default()
                .to_string(),
        ];
        parts.extend(lines.iter().map(|line| {
            format!(
                "{}|{}|{}",
                line.sales_order_line_id, line.quantity, line.expected_delivery_date
            )
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
    #[validate(
        length(min = 1, max = 200, message = "本次供给分配明细必须在1-200行之间"),
        nested
    )]
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
    /// 字符串类型化与集合规则由 `entities::purchase_order::sourcing_plan`
    /// 承担，本方法只负责协议错误映射；同一销售行允许按库存与采购依据拆分。
    pub(crate) fn sourcing_assignments(&self) -> Result<SourcingAssignmentSet> {
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
    pub(crate) fn request_fingerprint(&self, assignments: &[SourcingAssignment]) -> Result<String> {
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

/// 保存采购草稿请求（表头 + 完整行替换；金额由服务端逐行计算）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SavePurchaseOrderDraftRequest {
    /// 期望的乐观锁版本；与当前版本不一致时拒绝更新（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_lock_version: u64,
    /// 付款条件；缺省表示不修改。
    pub payment_term_code: Option<String>,
    /// 完整行集合（兼容服务端调用；与 `line_patches` 二选一）。
    #[serde(default)]
    pub lines: Vec<SavePurchaseOrderLine>,
    /// 以当前草稿行 ID 为键的可编辑字段快照，由服务端在事务内合并冻结来源字段。
    #[serde(default)]
    pub line_patches: Vec<SavePurchaseOrderLinePatch>,
    /// 幂等键（同内容重复保存返回同一结果）。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    #[validate(length(max = 128, message = "幂等键过长"))]
    pub idempotency_key: String,
}

impl SavePurchaseOrderDraftRequest {
    /// 校验保存请求只使用一种行载荷。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 完整行与行补丁恰好提供一种时返回 `Ok(())`。
    ///
    /// # 错误
    /// 两种都提供或都缺失，或任一补丁字段非法时返回校验错误。
    pub(crate) fn ensure_shape(&self) -> Result<()> {
        if self.lines.is_empty() == self.line_patches.is_empty() {
            return Err(Error::ValidationError(
                "完整采购行与草稿行补丁必须且只能提供一种".to_string(),
            ));
        }
        for patch in &self.line_patches {
            patch.validate()?;
        }
        Ok(())
    }

    /// 生成已规范化草稿行：完整行原样保留顺序返回；补丁路径把客户端可编辑
    /// 字段合并到服务端冻结的草稿来源行。
    ///
    /// # 参数
    /// * `existing` - 服务端当前草稿行
    ///
    /// # 返回
    /// 返回可进入领域校验与写入的完整行集合。
    ///
    /// # 错误
    /// 补丁未覆盖全部当前草稿行、包含重复行补丁、行类型被改写或行已变化时
    /// 返回校验或冲突错误。
    pub(crate) fn resolve_lines(
        &self,
        existing: &[PurchaseOrderSubmissionLine],
    ) -> Result<Vec<SavePurchaseOrderLine>> {
        if !self.lines.is_empty() {
            return Ok(self.lines.clone());
        }
        SavePurchaseOrderLinePatch::resolve_all(&self.line_patches, existing)
    }

    /// 计算保存草稿请求指纹（不含原始幂等键）。
    ///
    /// # 参数
    /// * `purchase_order_id` - 当前路径采购单 ID
    ///
    /// # 返回
    /// 返回不包含原始幂等键的稳定 SHA-256 指纹。
    ///
    /// # 错误
    /// 指纹载荷无法序列化时返回内部错误。
    ///
    /// # 关键业务约束
    /// 行顺序影响提交行序号，因此必须保留；付款条件按现有校验语义去除首尾
    /// 空白；摘要形态与存量收据一致，修改会破坏幂等兼容。
    pub(crate) fn request_fingerprint(&self, purchase_order_id: &str) -> Result<String> {
        let payload = SaveDraftFingerprintPayload {
            expected_lock_version: self.expected_lock_version,
            payment_term_code: self.payment_term_code.as_deref().map(str::trim),
            lines: &self.lines,
            line_patches: &self.line_patches,
        };
        payload_fingerprint(SAVE_ACTION, purchase_order_id, &payload).map_err(Into::into)
    }
}

/// 保存草稿请求指纹载荷。
#[derive(Serialize)]
struct SaveDraftFingerprintPayload<'a> {
    /// 客户端期望乐观锁版本。
    expected_lock_version: u64,
    /// 按业务语义规范化的付款条件。
    payment_term_code: Option<&'a str>,
    /// 保留顺序的完整草稿行。
    lines: &'a [SavePurchaseOrderLine],
    /// 保留顺序的草稿行可编辑字段快照。
    line_patches: &'a [SavePurchaseOrderLinePatch],
}

/// 采购草稿行可编辑字段快照。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SavePurchaseOrderLinePatch {
    /// 当前草稿提交行 ID。
    #[validate(custom(function = "non_blank", message = "草稿行 ID 不能为空"))]
    pub line_id: String,
    /// 行类型；必须与服务端当前草稿一致。
    pub line_type: PurchaseLineType,
    /// 商品/服务采购数量。
    pub quantity: Option<String>,
    /// 商品/服务含税单价；物流费用行表示含税费用金额。
    pub unit_cost_gross: Option<String>,
    /// 进项税率。
    pub input_tax_rate: Option<String>,
}

impl SavePurchaseOrderLinePatch {
    /// 将草稿行补丁合并为完整采购行，冻结字段只从服务端当前草稿取得。
    ///
    /// # 参数
    /// * `patches` - 客户端可编辑字段快照
    /// * `existing` - 服务端当前草稿行
    ///
    /// # 返回
    /// 返回合并后的完整行集合。
    ///
    /// # 错误
    /// 补丁未覆盖全部当前草稿行、包含重复行补丁、行类型被改写或行已变化时
    /// 返回校验或冲突错误。
    pub(crate) fn resolve_all(
        patches: &[Self],
        existing: &[PurchaseOrderSubmissionLine],
    ) -> Result<Vec<SavePurchaseOrderLine>> {
        if patches.len() != existing.len() {
            return Err(Error::ValidationError(
                "采购草稿行补丁必须覆盖全部当前草稿行".to_string(),
            ));
        }
        let mut patch_map = HashMap::with_capacity(patches.len());
        for patch in patches {
            if patch_map
                .insert(patch.line_id.trim().to_string(), patch)
                .is_some()
            {
                return Err(Error::ValidationError("采购草稿包含重复行补丁".to_string()));
            }
        }

        existing
            .iter()
            .map(|line| {
                let patch = patch_map
                    .remove(&line.base.id)
                    .ok_or_else(|| Error::ConflictError("采购草稿行已变化，请刷新后重试".to_string()))?;
                if patch.line_type != line.line_type {
                    return Err(Error::ValidationError("采购草稿行类型不可修改".to_string()));
                }
                let is_item = line.line_type == PurchaseLineType::ItemService;
                let quantity = if is_item {
                    patch
                        .quantity
                        .clone()
                        .or_else(|| line.quantity.map(|value| value.to_string()))
                } else {
                    None
                };
                Ok(SavePurchaseOrderLine {
                    line_type: line.line_type,
                    procurement_confirmation_line_id: line
                        .procurement_confirmation_line_id
                        .as_ref()
                        .map(ToString::to_string),
                    sku_id: line.sku_id.as_ref().map(ToString::to_string),
                    sku_revision_id: line.sku_revision_id.as_ref().map(ToString::to_string),
                    product_name: line.product_name_snapshot.clone(),
                    specification: line.specification_snapshot.clone(),
                    quantity: quantity.clone(),
                    base_unit_code: if is_item {
                        line.base_unit_code.clone()
                    } else {
                        None
                    },
                    unit_cost_gross: if is_item {
                        patch
                            .unit_cost_gross
                            .clone()
                            .or_else(|| line.unit_cost_gross.map(|value| value.to_string()))
                    } else {
                        None
                    },
                    input_tax_rate: patch
                        .input_tax_rate
                        .clone()
                        .or_else(|| line.input_tax_rate.map(|value| value.to_string())),
                    expected_delivery_date: if is_item {
                        line.expected_delivery_date.map(|value| value.to_string())
                    } else {
                        None
                    },
                    sales_order_line_id: line.sales_order_line_id.as_ref().map(ToString::to_string),
                    sales_order_revision_line_id: line
                        .sales_order_revision_line_id
                        .as_ref()
                        .map(ToString::to_string),
                    sales_order_submission_line_id: line
                        .sales_order_submission_line_id
                        .as_ref()
                        .map(ToString::to_string),
                    allocated_quantity: if is_item { quantity } else { None },
                    gross_amount: if is_item {
                        None
                    } else {
                        patch
                            .unit_cost_gross
                            .clone()
                            .or_else(|| Some(line.gross_amount.to_string()))
                    },
                })
            })
            .collect()
    }
}

/// 草稿行写入项。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SavePurchaseOrderLine {
    /// 行类型。
    pub line_type: PurchaseLineType,
    /// 商品/服务行对应的采购二次确认分行；物流费用行为空。
    pub procurement_confirmation_line_id: Option<String>,
    /// 商品行引用的 SKU。
    pub sku_id: Option<String>,
    /// 商品行引用的 SKU 版本。
    pub sku_revision_id: Option<String>,
    /// 商品名称快照。
    pub product_name: Option<String>,
    /// 规格快照。
    pub specification: Option<String>,
    /// 基础单位数量（商品行为必填字符串）。
    pub quantity: Option<String>,
    /// 单位代码。
    pub base_unit_code: Option<String>,
    /// 含税采购单价（商品行为必填）。
    pub unit_cost_gross: Option<String>,
    /// 进项税率（缺省 0）。
    pub input_tax_rate: Option<String>,
    /// 预计交期（`YYYY-MM-DD`）。
    pub expected_delivery_date: Option<String>,
    /// 商品行对应的销售稳定行。
    pub sales_order_line_id: Option<String>,
    /// 商品行对应的销售当前版本行。
    pub sales_order_revision_line_id: Option<String>,
    /// 商品行对应的历史销售提交行。
    pub sales_order_submission_line_id: Option<String>,
    /// 商品行对应的分配数量。
    pub allocated_quantity: Option<String>,
    /// 物流费用行含税金额（物流行为必填；商品行忽略）。
    pub gross_amount: Option<String>,
}

impl SavePurchaseOrderLine {
    /// 转换为领域草稿行编辑请求。
    ///
    /// 字符串字段原样传递；空白、数量与引用校验由
    /// `entities::purchase_order::validate_draft_line_edits` 统一完成。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回与自身字段一致的草稿行编辑请求。
    ///
    /// # 错误
    /// 无。
    pub(crate) fn to_draft_edit(&self) -> DraftLineEdit {
        DraftLineEdit {
            line_type: self.line_type,
            quantity: self.quantity.clone(),
            allocated_quantity: self.allocated_quantity.clone(),
            procurement_confirmation_line_id: self.procurement_confirmation_line_id.clone(),
            sku_id: self.sku_id.clone(),
            sku_revision_id: self.sku_revision_id.clone(),
            sales_order_line_id: self.sales_order_line_id.clone(),
            sales_order_revision_line_id: self.sales_order_revision_line_id.clone(),
            sales_order_submission_line_id: self.sales_order_submission_line_id.clone(),
        }
    }
}

/// 保存草稿结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SavePurchaseOrderDraftResult {
    /// 新乐观锁版本。
    pub lock_version: u64,
    /// 表头金额汇总（逐行舍入后汇总）。
    pub totals: TotalsView,
    /// 业务引用。
    pub reference: String,
}

/// 作废采购草稿请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct VoidPurchaseOrderRequest {
    /// 期望的采购单乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_lock_version: u64,
    /// 作废原因。
    #[validate(custom(function = "non_blank", message = "作废原因不能为空"))]
    #[validate(length(max = 512, message = "作废原因过长"))]
    pub reason: String,
    /// 业务请求幂等键。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    #[validate(length(max = 128, message = "幂等键过长"))]
    pub idempotency_key: String,
}

impl VoidPurchaseOrderRequest {
    /// 计算作废请求指纹（不含原始幂等键）。
    ///
    /// # 参数
    /// * `purchase_order_id` - 当前路径采购单 ID
    ///
    /// # 返回
    /// 返回不包含原始幂等键的稳定 SHA-256 指纹。
    ///
    /// # 错误
    /// 指纹载荷无法序列化时返回内部错误。
    ///
    /// # 关键业务约束
    /// 作废原因按实际审计语义去除首尾空白，期望版本仍属于请求载荷；摘要形态
    /// 与存量收据一致，修改会破坏幂等兼容。
    pub(crate) fn request_fingerprint(&self, purchase_order_id: &str) -> Result<String> {
        let payload = VoidDraftFingerprintPayload {
            expected_lock_version: self.expected_lock_version,
            reason: self.reason.trim(),
        };
        payload_fingerprint(VOID_ACTION, purchase_order_id, &payload).map_err(Into::into)
    }
}

/// 作废请求指纹载荷。
#[derive(Serialize)]
struct VoidDraftFingerprintPayload<'a> {
    /// 客户端期望乐观锁版本。
    expected_lock_version: u64,
    /// 去除首尾空白后的作废原因。
    reason: &'a str,
}

/// 作废采购草稿结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct VoidPurchaseOrderResult {
    /// 采购单主键。
    pub purchase_order_id: String,
    /// 作废后的稳定状态。
    pub status: String,
    /// 作废后的乐观锁版本。
    pub lock_version: u64,
    /// 是否命中已经完成的作废结果。
    pub replayed: bool,
    /// 业务引用。
    pub reference: String,
}

/// 提交财务审核请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SubmitPurchaseOrderRequest {
    /// 期望的乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_lock_version: u64,
    /// 付款条件只用于确认未改写创建时冻结值。
    pub payment_term_code: Option<String>,
    /// 提交时一并保存的草稿行可编辑字段；为空表示直接冻结当前草稿。
    #[serde(default)]
    pub line_patches: Vec<SavePurchaseOrderLinePatch>,
    /// 幂等键（重复提交只产生一条正式提交）。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    #[validate(length(max = 128, message = "幂等键过长"))]
    pub idempotency_key: String,
}

impl SubmitPurchaseOrderRequest {
    /// 计算提交请求指纹（不含原始幂等键）。
    ///
    /// # 参数
    /// * `purchase_order_id` - 当前路径采购单 ID
    ///
    /// # 返回
    /// 返回不包含原始幂等键的稳定 SHA-256 指纹。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 同一幂等键用于不同期望版本或草稿补丁时必须冲突；形态文本按字段顺序显式
    /// 编码（不依赖 Debug 派生），摘要形态与存量收据一致，修改会破坏幂等兼容。
    pub(crate) fn request_fingerprint(&self, purchase_order_id: &str) -> String {
        digest_parts([
            purchase_order_id.to_string(),
            self.expected_lock_version.to_string(),
            submit_request_shape(&self.payment_term_code, &self.line_patches),
        ])
    }
}

/// 构造提交请求的稳定形态文本。
///
/// 历史提交指纹直接使用 Rust Debug 派生输出（`format!("{:?}|{:?}", ...)`），
/// DTO 字段改名会静默改变指纹并破坏存量收据回放；本函数按字段顺序显式重放
/// 同一字节形态，字段改名不再影响指纹。
pub(super) fn submit_request_shape(
    payment_term_code: &Option<String>,
    line_patches: &[SavePurchaseOrderLinePatch],
) -> String {
    let patches = line_patches
        .iter()
        .map(|patch| {
            format!(
                "SavePurchaseOrderLinePatch {{ line_id: {:?}, line_type: {}, quantity: {:?}, unit_cost_gross: {:?}, input_tax_rate: {:?} }}",
                patch.line_id,
                submit_line_type_shape(patch.line_type),
                patch.quantity,
                patch.unit_cost_gross,
                patch.input_tax_rate,
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("{:?}|[{}]", payment_term_code, patches)
}

/// 返回行类型的历史 Debug 派生字节形态。
///
/// # 参数
/// * `line_type` - 采购行类型
///
/// # 返回
/// 返回与历史 Debug 派生输出一致的固定字符串。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 输出必须与提交指纹存量形态逐字节一致，禁止改名或改变大小写。
fn submit_line_type_shape(line_type: PurchaseLineType) -> &'static str {
    match line_type {
        PurchaseLineType::ItemService => "ItemService",
        PurchaseLineType::LogisticsFee => "LogisticsFee",
    }
}

/// 提交财务审核结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SubmitPurchaseOrderResult {
    /// 采购单主键。
    pub purchase_order_id: String,
    /// 采购单号。
    pub purchase_no: String,
    /// 形成的不可变提交。
    pub submission_id: String,
    /// 提交序号。
    pub submission_no: String,
    /// 审核待办。
    pub work_item_id: String,
    /// 审核待办自身的乐观锁版本。
    pub task_version: u64,
    /// 待办锁定的不可变采购提交版本。
    pub subject_version: String,
    /// 新乐观锁版本。
    pub lock_version: u64,
    /// 业务引用。
    pub reference: String,
}

/// 财务审核结果（通过/驳回共用形状）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseReviewResult {
    /// 已完成的审核待办。
    pub work_item_id: String,
    /// 待办终态，固定为 `COMPLETED`。
    pub work_item_status: String,
    /// 完成后的待办版本。
    pub task_version: String,
    /// 本次审核锁定的不可变采购提交版本。
    pub subject_version: String,
    /// 审核结论（`APPROVED`/`REJECTED`）。
    pub review_result: String,
    /// 通过时形成的生效版本。
    pub revision_id: Option<String>,
    /// 通过时形成的版本号。
    pub revision_no: Option<u32>,
    /// 通过时形成的应付分录。
    pub payable_entry_id: Option<String>,
    /// 新乐观锁版本。
    pub lock_version: u64,
    /// 业务引用。
    pub reference: String,
}
