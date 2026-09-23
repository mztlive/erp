//! 域 D17 `inventory` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；时间一律秒级时间戳；数量一律十进制字符串
//! （`erp_core::money::Quantity` 自定义序列化）。契约目标形状（items/total/
//! page/page_size）与 D01 source_registry 保持一致，本域按域内对象名提供
//! 各列表视图。

use erp_core::ids::{SkuId, WarehouseId};
use erp_core::money::Quantity;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::inventory::{
    AdjustmentReasonType, MovementDirection, MovementType, ReservationStatus, StockAdjustmentState,
};

/// 库存余额列表允许的排序字段白名单（api-contract §4：Service 层校验，禁止任意字段透传）。
pub const STOCK_BALANCE_SORT_FIELDS: &[&str] = &["sku_id", "created_at"];
/// 库存流水列表允许的排序字段白名单。
pub const STOCK_MOVEMENT_SORT_FIELDS: &[&str] = &["occurred_at", "recorded_at", "created_at"];
/// 库存预占列表允许的排序字段白名单。
pub const STOCK_RESERVATION_SORT_FIELDS: &[&str] = &["created_at", "updated_at"];
/// 库存调整单列表允许的排序字段白名单。
pub const STOCK_ADJUSTMENT_SORT_FIELDS: &[&str] = &["created_at", "adjustment_no"];

/// 排序方向。
pub use application_core::SortDir;

/// 归一化后的分页查询 DTO（Service → Repository 共用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageParams {
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数（已 clamp 到 1–100）。
    pub page_size: u32,
    /// 排序字段（已过白名单校验，`&'static str` 保证来源只可能是白名单）。
    pub sort_by: &'static str,
    /// 排序方向。
    pub sort_dir: SortDir,
}

impl PageParams {
    /// 返回 Repository 筛选共用的排序字段与方向。
    ///
    /// # 返回
    /// 返回 `(排序字段, 是否升序)`；调用方四类列表查询共用，避免重复映射。
    pub(crate) fn sort_selection(&self) -> (Option<String>, bool) {
        (Some(self.sort_by.to_string()), matches!(self.sort_dir, SortDir::Asc))
    }
}

/// 契约目标形状的分页响应（api-contract §3）：`items` + `total` + `page` + `page_size`。
pub use application_core::PageView;
/// 校验排序参数（白名单 + 方向），返回归一化排序字段与方向。
///
/// # 参数
/// * `sort_by` - 可选排序字段；空白视为未提供
/// * `sort_dir` - 可选排序方向；空白视为未提供
/// * `allowed_fields` - 白名单
///
/// # 返回
/// 返回 `(排序字段, 方向)`；未提供时默认 `("created_at", Desc)`。
///
/// # 错误
/// 字段不在白名单或方向不是 `asc`/`desc` 时返回 `ValidationError`。
pub use application_core::normalize_sort;

/// 库存余额列表视图（W10 台账，含仓库与 SKU 基础信息展示字段）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StockBalanceView {
    /// 实体主键。
    pub id: String,
    /// 仓库。
    pub warehouse_id: String,
    /// 仓库代码（D11 跨域投影）。
    pub warehouse_code: String,
    /// 仓库名称（D11 当前修订快照）。
    pub warehouse_name: String,
    /// SKU。
    pub sku_id: String,
    /// SKU 编号（D10 稳定身份）。
    pub sku_code: String,
    /// SKU 名称（D10 当前修订快照）。
    pub sku_name: String,
    /// 规格快照（D10 当前修订快照，可能为空）。
    pub spec_summary: Option<String>,
    /// 账面现存。
    pub on_hand_quantity: Quantity,
    /// 有效预占。
    pub reserved_quantity: Quantity,
    /// 可用数量。
    pub available_quantity: Quantity,
    /// 乐观锁版本（`BaseModel.version` ≡ 数据模型 `lock_version`）。
    pub version: String,
    /// 已应用最后流水。
    pub last_movement_id: Option<String>,
    /// 最后流水业务时间（秒级时间戳）。
    pub last_movement_at: Option<i64>,
    /// 最后流水类型（可空）。
    pub last_movement_type: Option<MovementType>,
    /// 是否存在有效预占。
    pub has_active_reservation: bool,
    /// 服务端按当前调用人签发的余额动作。
    pub allowed_actions: Vec<String>,
}

/// 库存流水列表视图（正式事实）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StockMovementView {
    /// 已授权流水行引用仓库的当前名称，独立于筛选目录。
    pub warehouse_name: Option<String>,
    /// 实体主键。
    pub id: String,
    /// 仓库。
    pub warehouse_id: String,
    /// SKU。
    pub sku_id: String,
    /// 流水类型。
    pub movement_type: MovementType,
    /// 流水方向。
    pub direction: MovementDirection,
    /// 正数数量。
    pub quantity: Quantity,
    /// 来源单据标识。
    pub source_document_id: String,
    /// 来源单据号（可读展示；无法解析时为空，前端回退显示主键）。
    pub source_document_no: Option<String>,
    /// 来源单据行标识。
    pub source_line_id: Option<String>,
    /// 业务实际发生时间（秒级时间戳）。
    pub occurred_at: i64,
    /// ERP 记录时间（秒级时间戳）。
    pub recorded_at: i64,
    /// ERP 记录人。
    pub recorded_by: String,
}

/// 库存预占列表视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StockReservationView {
    /// 实体主键。
    pub id: String,
    /// 仓库。
    pub warehouse_id: String,
    /// SKU。
    pub sku_id: String,
    /// 唯一归属销售明细。
    pub sales_order_line_id: String,
    /// 当前有效预占。
    pub reserved_quantity: Quantity,
    /// 已消耗数量。
    pub consumed_quantity: Quantity,
    /// 已释放数量。
    pub released_quantity: Quantity,
    /// 预占状态。
    pub status: ReservationStatus,
    /// 乐观锁版本。
    pub version: u64,
}

/// 库存调整单列表视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StockAdjustmentView {
    /// 实体主键。
    pub id: String,
    /// 调整单号。
    pub adjustment_no: String,
    /// 仓库。
    pub warehouse_id: String,
    /// 调整原因类型。
    pub reason_type: AdjustmentReasonType,
    /// 当前状态。
    pub status: StockAdjustmentState,
    /// 仓储经办人。
    pub prepared_by: String,
    /// 审批快照申请人；未提交时为空，不得用 `created_by` 顶替。
    pub submitted_by: Option<String>,
    /// 当前开放审批人。
    pub current_assignee: Option<String>,
    /// 仓储复核人。
    pub reviewed_by: Option<String>,
    /// 成本影响确认人。
    pub finance_reviewed_by: Option<String>,
    /// 原因说明（可空）。
    pub note: Option<String>,
    /// 业务发生时间（秒级时间戳；可空）。
    pub occurred_at: Option<i64>,
    /// 乐观锁版本。
    pub version: String,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 库存调整明细视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StockAdjustmentLineView {
    /// 实体主键。
    pub id: String,
    /// 调整 SKU。
    pub sku_id: String,
    /// 调整数量。
    pub quantity: Quantity,
    /// 调整方向。
    pub direction: MovementDirection,
}

/// 库存调整单详情视图（表头 + 明细 + 过账流水 + 只读审批）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StockAdjustmentDetailView {
    /// 表头。
    pub adjustment: StockAdjustmentView,
    /// 调整明细。
    pub lines: Vec<StockAdjustmentLineView>,
    /// 过账形成的正式库存流水（未过账为空）。
    pub posted_movements: Vec<StockMovementView>,
    /// 统一只读审批结构。客户端不得据此选择定义或审批人。
    pub approval: DocumentApprovalView,
}

/// 单据详情返回的统一只读审批结构。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalView {
    /// `PROCESS_REQUIRED` 或 `NO_APPROVAL`。
    pub requirement: String,
    /// 创建时冻结的定义摘要；未绑定为空。
    pub definition: Option<DocumentApprovalDefinitionView>,
    /// 已启动后的实例摘要；未提交为空。
    pub instance: Option<DocumentApprovalInstanceView>,
    /// 有界最近历史。
    pub recent_history: Vec<DocumentApprovalHistoryItemView>,
    /// 完整历史分页游标。
    pub history_page: DocumentApprovalHistoryPageView,
    /// 服务端允许的动作；不含选择定义或审批人。
    pub allowed_actions: Vec<String>,
    /// 当前调用人可提交草稿时返回的服务端权威 CAS 令牌。
    pub submit_command: Option<SubmitStockAdjustmentApprovalTokenView>,
    /// 当前调用人可执行普通撤回时返回的不可伪造运行时 CAS 令牌。
    pub cancel_command: Option<CancelStockAdjustmentApprovalTokenView>,
}

/// 库存调整提交审批的服务端权威 CAS 令牌。
///
/// 客户端必须原样回传目标冻结版本，禁止根据当前值自行递增。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SubmitStockAdjustmentApprovalTokenView {
    /// 当前库存调整单版本。
    pub expected_version: String,
    /// 本次提交应形成的目标冻结版本。
    pub expected_subject_version: String,
}

/// 库存调整普通撤回的当前运行时 CAS 令牌。
///
/// 所有版本均序列化为十进制字符串，禁止浏览器以 JS number 承载乐观锁。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CancelStockAdjustmentApprovalTokenView {
    /// 当前库存调整单版本。
    pub expected_version: String,
    /// 当前审批实例 ID。
    pub approval_process_instance_id: String,
    /// 冻结提交版本。
    pub expected_subject_version: String,
    /// 当前实例版本。
    pub expected_instance_version: String,
    /// 当前执行版本。
    pub expected_execution_version: String,
    /// 运行中实例的唯一开放任务版本；人员失效阻塞实例为空。
    pub expected_task_version: Option<String>,
}

/// 绑定定义只读摘要。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalDefinitionView {
    /// 定义主键。
    pub id: String,
    /// 定义名称。
    pub name: String,
    /// 定义业务版本。
    pub version: u32,
    /// 节点摘要。单据详情不展开审批人。
    pub nodes: Vec<DocumentApprovalNodeView>,
}

/// 定义节点只读摘要。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalNodeView {
    /// 节点键。
    pub key: String,
    /// 节点名称。
    pub name: String,
}

/// 运行实例只读摘要。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalInstanceView {
    /// 实例主键。
    pub id: String,
    /// 实例状态。
    pub status: String,
    /// 当前轮次。
    pub current_round_no: u32,
    /// 当前节点键。
    pub current_node: Option<String>,
    /// 当前执行冻结的节点名称。
    pub current_node_name: Option<String>,
    /// 当前审批人。
    pub current_assignee: Option<String>,
    /// 当前执行冻结的审批人显示名。
    pub current_assignee_name: Option<String>,
    /// 最近驳回原因。
    pub latest_rejection: Option<String>,
    /// 冻结提交版本（十进制字符串）。
    pub subject_version: String,
    /// 当前实例乐观锁版本（十进制字符串）。
    pub instance_version: String,
    /// 当前执行 ID；终态为空。
    pub current_execution_id: Option<String>,
    /// 当前执行版本（十进制字符串）；终态为空。
    pub current_execution_version: Option<String>,
    /// 当前开放任务 ID；BLOCKED/终态为空。
    pub current_task_id: Option<String>,
    /// 当前开放任务版本（十进制字符串）；BLOCKED/终态为空。
    pub current_task_version: Option<String>,
}

/// 有界历史项。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalHistoryItemView {
    /// 执行主键。
    pub execution_id: String,
    /// 轮次。
    pub round_no: u32,
    /// 节点键。
    pub node_key: String,
    /// 结束结果。
    pub result: String,
}

/// 完整历史分页。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalHistoryPageView {
    /// 下一页游标。
    pub next_cursor: Option<String>,
    /// 是否还有更多。
    pub has_more: bool,
}

/// 库存余额详情视图（W10 详情：余额 + 最近流水 + 有效预占 + 未过账调整）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StockBalanceDetailView {
    /// 余额行。
    pub balance: StockBalanceView,
    /// 最近 8 条库存流水（按业务时间倒序）。
    pub recent_movements: Vec<StockMovementView>,
    /// 有效预占（未释放/未全消耗）。
    pub active_reservations: Vec<StockReservationView>,
    /// 未过账的调整单（草稿/待复核/待确认/驳回）。
    pub pending_adjustments: Vec<StockAdjustmentView>,
}

/// 库存调整明细输入（创建时随调整单提交）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StockAdjustmentLineInput {
    /// 调整 SKU。
    pub sku_id: SkuId,
    /// 调整数量（正数）。
    pub quantity: Quantity,
    /// 调整方向。
    pub direction: MovementDirection,
}

/// 库存调整单创建请求（HTTP 契约：表头 + 明细一次提交，初始状态为草稿）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateStockAdjustmentRequest {
    /// 发起调整时所依据的库存余额行。
    #[validate(length(min = 1, max = 128, message = "库存余额 id 不能为空"))]
    pub balance_id: String,
    /// 发起调整时看到的库存余额版本；创建事务内不一致时拒绝。
    #[validate(range(min = 1, message = "库存余额版本必须大于 0"))]
    pub expected_balance_version: u64,
    /// 调整单号（全局唯一）。
    #[validate(length(min = 1, max = 64, message = "调整单号长度必须在1-64之间"))]
    pub adjustment_no: String,
    /// 仓库。
    pub warehouse_id: WarehouseId,
    /// 调整原因类型。
    pub reason_type: AdjustmentReasonType,
    /// 调整明细（1–100 行）。
    #[validate(length(min = 1, max = 100, message = "调整明细行数必须在1-100之间"))]
    pub lines: Vec<StockAdjustmentLineInput>,
    /// 原因说明（可空）。
    pub note: Option<String>,
    /// 业务发生时间（秒级时间戳；可空）。
    pub occurred_at: Option<i64>,
}

/// 库存调整单更新请求（携带乐观锁版本；仅草稿/驳回可更新）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdateStockAdjustmentRequest {
    /// 期望的乐观锁版本；与当前版本不一致时拒绝更新（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 调整原因类型；缺省表示不修改。
    pub reason_type: Option<AdjustmentReasonType>,
    /// 明细数量更新（按行覆盖；缺省表示不修改）。
    #[validate(nested)]
    pub lines: Option<Vec<StockAdjustmentLineUpdateInput>>,
    /// 原因说明；缺省表示不修改，空串清除。
    pub note: Option<String>,
    /// 业务发生时间（秒级时间戳）；缺省表示不修改。
    pub occurred_at: Option<i64>,
}

/// 库存调整明细数量/方向更新（行必须属于该调整单；仅草稿/驳回可更新）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct StockAdjustmentLineUpdateInput {
    /// 明细行主键。
    #[validate(length(min = 1, max = 128, message = "明细行 id 不能为空"))]
    pub line_id: String,
    /// 调整数量（正数）。
    #[validate(length(min = 1, max = 32, message = "调整数量不能为空"))]
    pub quantity: String,
    /// 调整方向（可空：不传保持不变；盘盈必增、盘亏/损坏必减）。
    pub direction: Option<MovementDirection>,
}

impl StockAdjustmentLineUpdateInput {
    /// 由必填明细行主键与调整数量构造明细更新。
    ///
    /// # 参数
    /// * `line_id` - 明细行主键
    /// * `quantity` - 调整数量（正数）
    ///
    /// # 返回
    /// 返回方向为空的明细更新。
    ///
    /// # 错误
    /// 无。
    pub fn new(line_id: impl Into<String>, quantity: impl Into<String>) -> Self {
        Self { line_id: line_id.into(), quantity: quantity.into(), direction: None }
    }

    /// 设置调整方向。
    ///
    /// # 参数
    /// * `direction` - 调整方向
    ///
    /// # 返回
    /// 返回更新后的明细更新。
    ///
    /// # 错误
    /// 无。
    pub fn with_direction(mut self, direction: MovementDirection) -> Self {
        self.direction = Some(direction);
        self
    }
}

/// 库存调整提交时冻结的余额版本。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ExpectedStockBalanceVersion {
    /// 库存余额行主键。
    #[validate(length(min = 1, max = 128, message = "库存余额 id 不能为空"))]
    pub balance_id: String,
    /// 用户编辑时看到的余额版本。
    #[validate(range(min = 1, message = "库存余额版本必须大于 0"))]
    pub expected_version: u64,
}

/// 库存调整单保存最终草稿并提交审批的原子命令。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct SubmitStockAdjustmentRequest {
    /// 期望的单据乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_version: u64,
    /// 服务端详情令牌下发的本次目标冻结版本。
    #[validate(range(min = 1, message = "审批主题版本必须大于 0"))]
    pub expected_subject_version: u32,
    /// 调整原因类型；与明细方向在服务端共同校验。
    pub reason_type: AdjustmentReasonType,
    /// 最终明细值。提交事务内先覆盖草稿，再冻结审批快照。
    #[validate(length(min = 1, max = 100, message = "调整明细行数必须在1-100之间"))]
    #[validate(nested)]
    pub lines: Vec<StockAdjustmentLineUpdateInput>,
    /// 提交时冻结的全部库存余额版本。
    #[validate(length(min = 1, max = 100, message = "库存余额版本行数必须在1-100之间"))]
    #[validate(nested)]
    pub balances: Vec<ExpectedStockBalanceVersion>,
    /// 原因说明；空串表示清除草稿中的说明。
    #[validate(length(max = 512, message = "原因说明不能超过512个字符"))]
    pub note: String,
    /// 业务发生时间（秒级时间戳）。
    pub occurred_at: i64,
    /// 业务请求幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 查询库存调整提交结果的稳定命令身份。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct StockAdjustmentSubmitResultQuery {
    /// 原提交命令要形成的冻结审批主题版本。
    #[validate(range(min = 1, message = "审批主题版本必须大于 0"))]
    pub expected_subject_version: u32,
    /// 原提交命令的幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键长度必须在1-128之间"))]
    pub idempotency_key: String,
}

/// 撤回库存调整审批请求。
///
/// 请求显式绑定原审批实例与运行事实版本，使同一幂等键在单据修改、重新提交
/// 后仍可按原实例安全回放，且不会从终态反推执行或任务版本。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CancelStockAdjustmentApprovalRequest {
    /// 期望的库存调整单乐观锁版本。
    #[validate(range(min = 1, message = "库存调整单版本必须大于 0"))]
    pub expected_version: u64,
    /// 审批实例 ID，也是取消命令收据的稳定作用域。
    #[validate(length(min = 1, max = 128, message = "审批实例 id 不能为空"))]
    pub approval_process_instance_id: String,
    /// 冻结提交版本。
    #[validate(range(min = 1, message = "审批主题版本必须大于 0"))]
    pub expected_subject_version: u32,
    /// 期望实例版本。
    #[validate(range(min = 1, message = "审批实例版本必须大于 0"))]
    pub expected_instance_version: u64,
    /// 期望当前执行版本。
    #[validate(range(min = 1, message = "审批执行版本必须大于 0"))]
    pub expected_execution_version: u64,
    /// 运行中实例的唯一开放任务版本；人员失效阻塞实例必须为空。
    #[validate(range(min = 1, message = "审批任务版本必须大于 0"))]
    pub expected_task_version: Option<u64>,
    /// 非空撤回原因。
    #[validate(length(min = 1, max = 512, message = "撤回原因不能为空"))]
    pub reason: String,
    /// 业务请求幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::ids::{SalesOrderLineId, SkuId, WarehouseId};
    use erp_core::money::Quantity;
    use validator::Validate;

    use super::{
        SortDir, StockAdjustmentLineUpdateInput, StockAdjustmentView, StockBalanceView, normalize_sort,
    };
    use crate::dto::{
        StockAdjustmentListParams, StockBalanceListParams, StockMovementListParams,
        StockReservationListParams,
    };
    use crate::entity::inventory::{
        AdjustmentReasonType, MovementDirection, MovementType, ReservationStatus, StockAdjustmentState,
    };

    #[test]
    fn sort_whitelist_rejects_unknown_fields_and_directions() {
        assert!(normalize_sort(&Some("quantity".to_string()), &None, &["sku_id", "created_at"]).is_err());
        assert!(normalize_sort(&None, &Some("up".to_string()), &["created_at"]).is_err());

        let (field, direction) = normalize_sort(
            &Some(" sku_id ".to_string()),
            &Some(" asc ".to_string()),
            &["sku_id", "created_at"],
        )
        .unwrap();
        assert_eq!(field, "sku_id");
        assert_eq!(direction, SortDir::Asc);

        let (field, direction) = normalize_sort(&None, &None, &["created_at"]).unwrap();
        assert_eq!(field, "created_at");
        assert_eq!(direction, SortDir::Desc);
    }

    #[test]
    fn balance_list_params_normalize_paging_and_filters() {
        let params = StockBalanceListParams {
            warehouse_id: Some(WarehouseId::new("wh-1")),
            sku_id: Some(SkuId::new("sku-1")),
            page: Some(2),
            page_size: Some(50),
            sort_by: Some("sku_id".to_string()),
            sort_dir: Some("asc".to_string()),
            ..Default::default()
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.warehouse_id.as_deref(), Some("wh-1"));
        assert_eq!(query.sku_id.as_deref(), Some("sku-1"));
        assert_eq!(query.paging.page, 2);
        assert_eq!(query.paging.page_size, 50);
        assert_eq!(query.paging.sort_by, "sku_id");
    }

    #[test]
    fn tier_a_list_params_default_to_empty() {
        assert!(StockBalanceListParams::default().warehouse_id.is_none());
        assert!(StockMovementListParams::default().movement_type.is_none());
        assert!(StockReservationListParams::default().status.is_none());
        assert!(StockAdjustmentListParams::default().status.is_none());
    }

    #[test]
    fn tier_c_inventory_filters_default_to_first_page_size_20() {
        use crate::repository::{
            StockAdjustmentFilter, StockBalanceFilter, StockMovementFilter, StockReservationFilter,
        };
        assert_eq!((StockMovementFilter::default().page, StockMovementFilter::default().page_size), (1, 20));
        assert_eq!(
            (StockReservationFilter::default().page, StockReservationFilter::default().page_size),
            (1, 20)
        );
        assert_eq!(
            (StockAdjustmentFilter::default().page, StockAdjustmentFilter::default().page_size),
            (1, 20)
        );
        assert_eq!((StockBalanceFilter::default().page, StockBalanceFilter::default().page_size), (1, 20));
    }

    #[test]
    fn tier_b_line_update_constructor_sets_identity_only() {
        let input = StockAdjustmentLineUpdateInput::new("line-1", "10");
        assert_eq!(input.line_id, "line-1");
        assert!(input.direction.is_none());
        let directed =
            StockAdjustmentLineUpdateInput::new("line-1", "10").with_direction(MovementDirection::Increase);
        assert!(directed.direction.is_some());
    }

    #[test]
    fn movement_list_params_reject_inverted_time_range() {
        let params = StockMovementListParams {
            movement_type: Some(MovementType::PurchaseReceiptIn),
            occurred_from: Some(1_800_000_000),
            occurred_to: Some(1_700_000_000),
            ..Default::default()
        };
        assert!(params.normalized().is_err(), "时间区间倒挂必须拒绝");
    }

    #[test]
    fn list_params_reject_unbounded_page_size() {
        let balance =
            StockBalanceListParams { page: Some(0), page_size: Some(u32::MAX), ..Default::default() };
        assert!(balance.validate().is_err());

        let reservation = StockReservationListParams {
            status: Some(ReservationStatus::Active),
            sales_order_line_id: Some(SalesOrderLineId::new("so-line-1")),
            page: Some(1),
            page_size: Some(u32::MAX),
            ..Default::default()
        };
        assert!(reservation.validate().is_err());

        let adjustment = StockAdjustmentListParams {
            status: Some(StockAdjustmentState::InApproval),
            page: Some(1),
            page_size: Some(u32::MAX),
            ..Default::default()
        };
        assert!(adjustment.validate().is_err());
    }

    /// 提交与撤回请求拒绝客户端选择定义或审批人。
    #[test]
    fn submit_and_cancel_reject_client_chosen_definition_or_assignee() {
        use super::{CancelStockAdjustmentApprovalRequest, SubmitStockAdjustmentRequest};

        assert!(
            serde_json::from_value::<SubmitStockAdjustmentRequest>(serde_json::json!({
                "expected_version": 1,
                "idempotency_key": "k1",
                "reviewed_by": "forged"
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<CancelStockAdjustmentApprovalRequest>(serde_json::json!({
                "expected_version": 1,
                "approval_process_instance_id": "instance-1",
                "expected_subject_version": 1,
                "expected_instance_version": 1,
                "expected_execution_version": 1,
                "expected_task_version": 1,
                "reason": "改单",
                "idempotency_key": "k2",
                "assignee": "forged"
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<SubmitStockAdjustmentRequest>(serde_json::json!({
                "expected_version": 1,
                "idempotency_key": "k1",
                "definition_id": "def-1"
            }))
            .is_err()
        );
        let submit: SubmitStockAdjustmentRequest = serde_json::from_value(serde_json::json!({
            "expected_version": 1,
            "expected_subject_version": 1,
            "reason_type": "STOCK_LOSS",
            "lines": [{
                "line_id": "line-1",
                "quantity": "2",
                "direction": "DECREASE"
            }],
            "balances": [{
                "balance_id": "balance-1",
                "expected_version": 3
            }],
            "note": "盘点差异",
            "occurred_at": 1_700_000_000,
            "idempotency_key": "k1"
        }))
        .unwrap();
        assert_eq!(submit.expected_version, 1);
    }

    /// 余额响应版本必须保持十进制字符串，不能经过 JS number。
    #[test]
    fn stock_balance_view_serializes_version_as_decimal_string() {
        let quantity = Quantity::from_str("1").unwrap();
        let value = serde_json::to_value(StockBalanceView {
            id: "balance-1".to_string(),
            warehouse_id: "warehouse-1".to_string(),
            warehouse_code: "WH-1".to_string(),
            warehouse_name: "主仓".to_string(),
            sku_id: "sku-1".to_string(),
            sku_code: "SKU-1".to_string(),
            sku_name: "商品".to_string(),
            spec_summary: None,
            on_hand_quantity: quantity,
            reserved_quantity: quantity,
            available_quantity: quantity,
            version: "9007199254740993".to_string(),
            last_movement_id: None,
            last_movement_at: None,
            last_movement_type: None,
            has_active_reservation: false,
            allowed_actions: vec!["CREATE_ADJUSTMENT".to_string()],
        })
        .unwrap();
        assert_eq!(value["version"], serde_json::json!("9007199254740993"));
        assert_eq!(value["allowed_actions"], serde_json::json!(["CREATE_ADJUSTMENT"]));
    }

    /// 库存调整列表、详情及 PUT 响应的版本不得经过 JSON number。
    #[test]
    fn stock_adjustment_view_serializes_version_as_decimal_string() {
        let value = serde_json::to_value(StockAdjustmentView {
            id: "adjustment-1".to_string(),
            adjustment_no: "ADJ-1".to_string(),
            warehouse_id: "warehouse-1".to_string(),
            reason_type: AdjustmentReasonType::StockGain,
            status: StockAdjustmentState::Draft,
            prepared_by: "operator-1".to_string(),
            submitted_by: Some("applicant-1".to_string()),
            current_assignee: Some("handler-1".to_string()),
            reviewed_by: None,
            finance_reviewed_by: None,
            note: None,
            occurred_at: None,
            version: "9007199254740993".to_string(),
            created_at: 1,
        })
        .unwrap();
        assert_eq!(value["version"], serde_json::json!("9007199254740993"));
        assert_eq!(value["submitted_by"], serde_json::json!("applicant-1"));
        assert_ne!(value["submitted_by"], serde_json::json!("creator-1"));
    }

    #[test]
    fn balance_list_rejects_personnel_owner_params() {
        for raw in [
            serde_json::json!({"owner_user_ids": "user-1"}),
            serde_json::json!({"operator_user_ids": "user-1"}),
            serde_json::json!({"handler_user_ids": "user-1"}),
            serde_json::json!({"applicant_user_ids": "user-1"}),
            serde_json::json!({"org_unit_ids": "org-1"}),
        ] {
            assert!(serde_json::from_value::<StockBalanceListParams>(raw).is_err());
        }
        let ok: StockBalanceListParams =
            serde_json::from_value(serde_json::json!({"page": 1, "scope_version": "v1"})).unwrap();
        assert_eq!(ok.scope_version.as_deref(), Some("v1"));
    }

    #[test]
    fn movement_and_adjustment_consume_registered_people_filters() {
        let movement: StockMovementListParams = serde_json::from_value(serde_json::json!({
            "operator_user_ids": "op-2,op-1",
            "scope_version": "v1"
        }))
        .unwrap();
        let query = movement.normalized().unwrap();
        assert_eq!(
            query.operator_user_ids.as_deref(),
            Some(["op-1".to_string(), "op-2".to_string()].as_slice())
        );
        assert!(
            serde_json::from_value::<StockMovementListParams>(serde_json::json!({
                "applicant_user_ids": "user-1"
            }))
            .is_err()
        );
        let adjustment: StockAdjustmentListParams = serde_json::from_value(serde_json::json!({
            "operator_user_ids": "op-1",
            "applicant_user_ids": "app-1",
            "handler_user_ids": "h-1"
        }))
        .unwrap();
        let query = adjustment.normalized().unwrap();
        assert_eq!(query.operator_user_ids.as_deref(), Some(["op-1".to_string()].as_slice()));
        assert_eq!(query.applicant_user_ids.as_deref(), Some(["app-1".to_string()].as_slice()));
        assert_eq!(query.handler_user_ids.as_deref(), Some(["h-1".to_string()].as_slice()));
        assert!(
            serde_json::from_value::<StockAdjustmentListParams>(serde_json::json!({
                "owner_user_ids": "user-1"
            }))
            .is_err()
        );
        let me: StockMovementListParams =
            serde_json::from_value(serde_json::json!({"operator_user_ids": "me"})).unwrap();
        assert!(me.normalized().is_err());
    }
}
