//! 履约customer_acceptance请求及单域查询 DTO。
use super::non_blank;
use super::{normalize_sort, PageParams, CUSTOMER_ACCEPTANCE_SORT_FIELDS};
use crate::entity::fulfillment::{
    AcceptanceResult, AllocationAction, CustomerAcceptanceState, FulfillmentFactType,
};
use crate::Result;
use application_core::{page_or_default, page_size_or_default};
use erp_core::ids::{SalesOrderId, SalesOrderLineId};
use erp_core::money::Quantity;
use serde::{Deserialize, Serialize};
use validator::Validate;

/// 验收履约分配输入（验收行对履约事实的分配）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptanceAllocationInput {
    /// 履约事实行（发货行/电子交付/服务履约的主键，跨域多态引用）。
    pub fulfillment_line_id: String,
    /// 履约事实类型。
    pub fulfillment_fact_type: FulfillmentFactType,
    /// 正数验收数量。
    pub allocated_quantity: Quantity,
}

/// 客户验收行输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptanceLineInput {
    /// 验收明细。
    pub sales_order_line_id: SalesOrderLineId,
    /// 通过数量。
    pub accepted_quantity: Quantity,
    /// 短少数量。
    pub short_quantity: Quantity,
    /// 拒收数量。
    pub rejected_quantity: Quantity,
    /// 依据说明。
    pub reason: Option<String>,
    /// 对履约事实的分配（过账时按行守恒校验）。
    pub allocations: Vec<AcceptanceAllocationInput>,
}

/// 客户验收单创建请求（表头 + 行一次提交，初始状态为草稿）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateCustomerAcceptanceRequest {
    /// 销售单。
    pub sales_order_id: SalesOrderId,
    /// 验收时间（秒级时间戳）。
    pub accepted_at: i64,
    /// 验收结果。
    pub result: AcceptanceResult,
    /// 验收行（1–200 行）。
    #[validate(length(min = 1, max = 200, message = "验收行数必须在1-200之间"))]
    pub lines: Vec<AcceptanceLineInput>,
}

/// 客户验收过账请求（携带逐行分配；通过/短少/拒收数量以草稿行为准）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PostCustomerAcceptanceRequest {
    /// 从统一工作台进入时携带的客户验收任务主键；与期望任务版本同时提供。
    pub work_item_id: Option<String>,
    /// 从统一工作台进入时携带的任务乐观锁版本；与任务主键同时提供。
    pub expected_task_version: Option<u64>,
    /// 逐行对履约事实的分配（行内合计必须等于该行通过数量）。
    #[validate(length(min = 1, max = 200, message = "验收行数必须在1-200之间"))]
    pub lines: Vec<PostAcceptanceLineInput>,
}

/// 客户验收原子登记请求。
///
/// 前端一次提交必须发送最终表头、最终行和履约分配；服务端在同一事务内完成
/// 草稿创建或替换、分配校验与写入、过账、销售单履约进度刷新和审计。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CommitCustomerAcceptanceRequest {
    /// 从统一工作台进入时携带的客户验收任务主键；与期望任务版本同时提供。
    pub work_item_id: Option<String>,
    /// 从统一工作台进入时携带的任务乐观锁版本；与任务主键同时提供。
    pub expected_task_version: Option<u64>,
    /// 已保存草稿主键；为空时在事务内新建草稿。
    pub acceptance_id: Option<String>,
    /// 已保存草稿的期望乐观锁版本；提交已有草稿时必填。
    pub expected_acceptance_version: Option<u64>,
    /// 销售单。
    pub sales_order_id: SalesOrderId,
    /// 销售单期望乐观锁版本，防止基于过期履约事实提交。
    #[validate(range(min = 1, message = "销售单乐观锁版本必须大于 0"))]
    pub expected_sales_order_version: u64,
    /// 验收时间（秒级时间戳）。
    pub accepted_at: i64,
    /// 验收结果。
    pub result: AcceptanceResult,
    /// 最终验收行及其履约事实分配。
    #[validate(length(min = 1, max = 200, message = "验收行数必须在1-200之间"))]
    pub lines: Vec<AcceptanceLineInput>,
    /// 客户端提交标识；用于审计关联和结果未知后的安全重试。
    #[validate(custom(function = "non_blank", message = "提交标识不能为空"))]
    pub idempotency_key: String,
}

/// 客户验收过账的逐行输入。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PostAcceptanceLineInput {
    /// 验收明细。
    pub sales_order_line_id: SalesOrderLineId,
    /// 对履约事实的分配。
    #[validate(length(min = 1, max = 200, message = "分配行数必须在1-200之间"))]
    pub allocations: Vec<AcceptanceAllocationInput>,
}

/// 客户验收冲正请求（误录时新增反向验收及反向分配）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ReverseCustomerAcceptanceRequest {
    /// 期望的乐观锁版本；与当前版本不一致时拒绝冲正（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_version: u64,
    /// 冲正原因说明。
    #[validate(custom(function = "non_blank", message = "冲正原因不能为空"))]
    pub reason_text: String,
    /// 客户端提交标识；网络重试必须保持不变。
    #[validate(custom(function = "non_blank", message = "提交标识不能为空"))]
    pub idempotency_key: String,
}

/// 客户验收单列表视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CustomerAcceptanceView {
    /// 实体主键。
    pub id: String,
    /// 客户验收单号。
    pub acceptance_no: String,
    /// 销售单。
    pub sales_order_id: String,
    /// 验收时间（秒级时间戳）。
    pub accepted_at: i64,
    /// 验收结果。
    pub result: AcceptanceResult,
    /// 当前状态。
    pub status: CustomerAcceptanceState,
    /// 误录验收的反向事实。
    pub reversal_of_acceptance_id: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 客户验收行视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CustomerAcceptanceLineView {
    /// 实体主键。
    pub id: String,
    /// 稳定行号。
    pub line_no: u32,
    /// 验收明细。
    pub sales_order_line_id: String,
    /// 通过数量。
    pub accepted_quantity: Quantity,
    /// 短少数量。
    pub short_quantity: Quantity,
    /// 拒收数量。
    pub rejected_quantity: Quantity,
    /// 依据说明。
    pub reason: Option<String>,
}

/// 验收履约分配视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AcceptanceAllocationView {
    /// 实体主键。
    pub id: String,
    /// 验收结果行。
    pub customer_acceptance_line_id: String,
    /// 履约事实类型。
    pub fulfillment_fact_type: FulfillmentFactType,
    /// 履约事实行。
    pub fulfillment_line_id: String,
    /// 分配动作。
    pub allocation_action: AllocationAction,
    /// 正数验收数量。
    pub allocated_quantity: Quantity,
    /// 反向分配引用的原分配。
    pub reverses_allocation_id: Option<String>,
}

/// 客户验收单详情视图（表头 + 行 + 分配）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CustomerAcceptanceDetailView {
    /// 表头。
    pub acceptance: CustomerAcceptanceView,
    /// 验收行。
    pub lines: Vec<CustomerAcceptanceLineView>,
    /// 验收履约分配。
    pub allocations: Vec<AcceptanceAllocationView>,
}

/// 客户验收单列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CustomerAcceptanceListParams {
    /// 销售单筛选。
    pub sales_order_id: Option<SalesOrderId>,
    /// 单据状态筛选。
    pub status: Option<CustomerAcceptanceState>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`accepted_at`/`created_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的客户验收单列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CustomerAcceptanceListQuery {
    /// 销售单筛选。
    pub sales_order_id: Option<SalesOrderId>,
    /// 单据状态筛选。
    pub status: Option<CustomerAcceptanceState>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl CustomerAcceptanceListParams {
    /// 归一化客户验收单列表查询参数。
    ///
    /// 分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<CustomerAcceptanceListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, CUSTOMER_ACCEPTANCE_SORT_FIELDS)?;
        Ok(CustomerAcceptanceListQuery {
            sales_order_id: self.sales_order_id.clone(),
            status: self.status,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}
