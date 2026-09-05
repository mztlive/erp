//! 销项/进项发票登记、过账与红冲命令 DTO。

use entities::common::time::BusinessDate;
use entities::ids::{PartyId, ReceivableAccountId, WorkItemId};
use entities::money::Amount;
use entities::receivable::{InvoiceDirection, InvoiceKind};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::query::non_blank;

// ---------------------------------------------------------------------------
// 发票（invoice，D18 拥有实体；D19 经本域 Repository 复用）
// ---------------------------------------------------------------------------

/// 发票登记请求（W11 登记草稿销项发票；登记过账与分配走 `post`）。
///
/// 客户端不得提交定义 ID 或审批人；未知字段失败关闭。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CreateInvoiceRequest {
    /// 发票方向（销项 `Sales` / 进项 `Purchase`；D19 进项登记复用本域 DTO）。
    pub invoice_direction: InvoiceDirection,
    /// 蓝红类型（红票走 `red_issue` 接口，此处仅蓝票草稿）。
    pub invoice_kind: InvoiceKind,
    /// 客户或供应商。
    pub party_id: PartyId,
    /// 发票代码（无代码数电票为空）。
    pub invoice_code: Option<String>,
    /// 发票号码。
    #[validate(custom(function = "non_blank", message = "发票号码不能为空"))]
    pub invoice_no: String,
    /// 开票日期（`YYYY-MM-DD`）。
    pub invoice_date: BusinessDate,
    /// 含税金额。
    pub gross_amount: Amount,
    /// 不含税金额。
    pub net_amount: Amount,
    /// 税额。
    pub tax_amount: Amount,
    /// 发票尾差（可正可负）。
    #[serde(default)]
    pub rounding_adjustment_amount: Option<Amount>,
    /// 尾差原因。
    pub rounding_reason: Option<String>,
}

/// 销项发票分配请求行（§8.3-2：同一往来主体与方向、蓝票受发票有效余额与
/// 目标子账可开票额度双侧上限）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SalesInvoiceAllocationLineRequest {
    /// 销售单可开票对象（应收往来子账）。
    pub receivable_account_id: ReceivableAccountId,
    /// 分配含税金额。
    pub allocated_gross_amount: Amount,
    /// 分配不含税金额。
    pub allocated_net_amount: Amount,
    /// 分配税额。
    pub allocated_tax_amount: Amount,
}

/// 发票登记过账请求（资金入口，规范化号码唯一索引构成去重机制）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct PostInvoiceRequest {
    /// 当前开放销项开票执行任务。
    pub work_item_id: WorkItemId,
    /// 查询所得任务乐观锁版本。
    #[validate(custom(function = "non_blank", message = "任务版本不能为空"))]
    #[validate(length(max = 20, message = "任务版本不能超过 20 个字符"))]
    pub expected_task_version: String,
    /// 发票分配行（合计必须等于发票含税金额）。
    #[validate(length(min = 1, message = "至少提供一条发票分配"))]
    pub allocations: Vec<SalesInvoiceAllocationLineRequest>,
}

/// 销项发票原子登记请求。
///
/// 已有草稿时提交 `invoice_id + expected_version`；新登记时提交完整 `invoice`。
/// 服务端在一个事务内完成单据注册、发票创建、分配、子账进度与审计。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CommitInvoiceRequest {
    /// 当前开放销项开票执行任务。
    pub work_item_id: WorkItemId,
    /// 查询所得任务乐观锁版本。
    #[validate(custom(function = "non_blank", message = "任务版本不能为空"))]
    #[validate(length(max = 20, message = "任务版本不能超过 20 个字符"))]
    pub expected_task_version: String,
    /// 已有发票草稿主键。
    pub invoice_id: Option<String>,
    /// 已有草稿期望乐观锁版本。
    pub expected_version: Option<u64>,
    /// 新发票完整字段；提交已有草稿时为空。
    pub invoice: Option<CreateInvoiceRequest>,
    /// 销项发票分配行。
    #[validate(length(min = 1, message = "至少提供一条发票分配"))]
    pub allocations: Vec<SalesInvoiceAllocationLineRequest>,
    /// 业务请求幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 红票原子开具请求。
///
/// 客户端只提交原票上的业务意图；服务端在事务内读取有效分配并生成反向行，
/// 禁止客户端搬运原分配 ID、净额或税额。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CommitRedInvoiceRequest {
    /// 红票号码；为空时按幂等键生成稳定号码。
    pub invoice_no: Option<String>,
    /// 本次红冲含税金额；为空时红冲全部剩余有效分配。
    pub amount: Option<Amount>,
    /// 红冲业务原因，写入审计日志。
    #[validate(custom(function = "non_blank", message = "红冲原因不能为空"))]
    pub reason: String,
    /// 业务请求幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键不能为空"))]
    pub idempotency_key: String,
}
