use application_core::non_blank;
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::save::SavePurchaseOrderLinePatch;
use crate::entity::purchase_order::{PurchaseLineType, digest_parts};

/// 提交命令的服务端固定审计动作。
pub const PURCHASE_SUBMIT_ACTION: &str = "purchase_order.submit";

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
    pub fn request_fingerprint(&self, purchase_order_id: &str) -> String {
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
pub(crate) fn submit_request_shape(
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
