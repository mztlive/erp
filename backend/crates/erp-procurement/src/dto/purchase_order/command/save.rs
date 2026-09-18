use std::collections::HashMap;

use application_core::non_blank;
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::super::query::TotalsView;
use crate::entity::purchase_order::{
    DraftLineEdit, PurchaseLineType, PurchaseOrderSubmissionLine, payload_fingerprint,
};
use crate::{Error, Result};

/// 保存草稿命令的服务端固定审计动作（同时参与请求指纹）。
pub const SAVE_ACTION: &str = "purchase_order.update";

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
    /// 由必填幂等键构造保存草稿请求。
    ///
    /// # 参数
    /// * `idempotency_key` - 幂等键（同内容重复保存返回同一结果）
    ///
    /// # 返回
    /// 返回零版本、行载荷为空的保存请求。
    ///
    /// # 错误
    /// 无。
    pub fn new(idempotency_key: impl Into<String>) -> Self {
        Self {
            expected_lock_version: 0,
            payment_term_code: None,
            lines: Vec::new(),
            line_patches: Vec::new(),
            idempotency_key: idempotency_key.into(),
        }
    }

    /// 设置期望的乐观锁版本。
    ///
    /// # 参数
    /// * `expected_lock_version` - 期望的乐观锁版本
    ///
    /// # 返回
    /// 返回更新后的保存请求。
    ///
    /// # 错误
    /// 无。
    pub fn with_expected_lock_version(mut self, expected_lock_version: u64) -> Self {
        self.expected_lock_version = expected_lock_version;
        self
    }

    /// 设置付款条件。
    ///
    /// # 参数
    /// * `payment_term_code` - 付款条件
    ///
    /// # 返回
    /// 返回更新后的保存请求。
    ///
    /// # 错误
    /// 无。
    pub fn with_payment_term_code(mut self, payment_term_code: impl Into<String>) -> Self {
        self.payment_term_code = Some(payment_term_code.into());
        self
    }

    /// 设置完整行集合。
    ///
    /// # 参数
    /// * `lines` - 完整行集合
    ///
    /// # 返回
    /// 返回更新后的保存请求。
    ///
    /// # 错误
    /// 无。
    pub fn with_lines(mut self, lines: Vec<SavePurchaseOrderLine>) -> Self {
        self.lines = lines;
        self
    }

    /// 设置草稿行补丁集合。
    ///
    /// # 参数
    /// * `line_patches` - 草稿行可编辑字段快照
    ///
    /// # 返回
    /// 返回更新后的保存请求。
    ///
    /// # 错误
    /// 无。
    pub fn with_line_patches(mut self, line_patches: Vec<SavePurchaseOrderLinePatch>) -> Self {
        self.line_patches = line_patches;
        self
    }

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
    pub fn ensure_shape(&self) -> Result<()> {
        if self.lines.is_empty() == self.line_patches.is_empty() {
            return Err(Error::ValidationError("完整采购行与草稿行补丁必须且只能提供一种".to_string()));
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
    pub fn resolve_lines(
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
    pub fn request_fingerprint(&self, purchase_order_id: &str) -> Result<String> {
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
    pub fn resolve_all(
        patches: &[Self],
        existing: &[PurchaseOrderSubmissionLine],
    ) -> Result<Vec<SavePurchaseOrderLine>> {
        if patches.len() != existing.len() {
            return Err(Error::ValidationError("采购草稿行补丁必须覆盖全部当前草稿行".to_string()));
        }
        let mut patch_map = HashMap::with_capacity(patches.len());
        for patch in patches {
            if patch_map.insert(patch.line_id.trim().to_string(), patch).is_some() {
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
                    patch.quantity.clone().or_else(|| line.quantity.map(|value| value.to_string()))
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
                    base_unit_code: if is_item { line.base_unit_code.clone() } else { None },
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
                        patch.unit_cost_gross.clone().or_else(|| Some(line.gross_amount.to_string()))
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
    /// `crate::entity::purchase_order::validate_draft_line_edits` 统一完成。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回与自身字段一致的草稿行编辑请求。
    ///
    /// # 错误
    /// 无。
    pub fn to_draft_edit(&self) -> DraftLineEdit {
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
