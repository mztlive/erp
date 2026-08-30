//! 域 D31 `mall_backfill` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；时间一律秒级时间戳；金额一律十进制字符串。

use entities::ids::{MallConsumptionBackfillJobId, MallConsumptionCutoverId};
use entities::mall_backfill::{BackfillCostBasis, BackfillItemResult, BackfillJobStatus};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use validator::Validate;

use crate::errors::Result;
use crate::query::{normalized_text, page_or_default, page_size_or_default};

/// 回填作业列表允许的排序字段白名单（api-contract §4：Service 层校验）。
pub(crate) const BACKFILL_JOB_SORT_FIELDS: &[&str] = &["range_start", "created_at"];
/// 回填明细列表允许的排序字段白名单。
pub(crate) const BACKFILL_ITEM_SORT_FIELDS: &[&str] = &["business_fact_key", "created_at"];

/// 排序方向。
pub use crate::query::SortDir;

/// 归一化后的分页查询 DTO（Service → Repository 共用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageParams {
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数（已 clamp 到 1–100）。
    pub page_size: u32,
    /// 排序字段（已过白名单校验）。
    pub sort_by: &'static str,
    /// 排序方向。
    pub sort_dir: SortDir,
}

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
pub(crate) use crate::query::normalize_sort;

/// 契约目标形状的分页响应（api-contract §3）：`items` + `total` + `page` + `page_size`。
pub use crate::query::PageView;

/// 校验文本去除首尾空白后非空。
use crate::query::non_blank;

/// 校验金额字符串为合法非负定点数值（小数位 ≤ 2）。
fn valid_amount(value: &str) -> std::result::Result<(), validator::ValidationError> {
    let amount = entities::money::Amount::from_str(value)
        .map_err(|_| validator::ValidationError::new("不是合法定点数值"))?;
    if amount.to_decimal().is_sign_negative() {
        return Err(validator::ValidationError::new("金额不能为负"));
    }
    Ok(())
}

/// 回填作业创建请求（W30 创建回填任务草稿）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateBackfillJobRequest {
    /// 来源商城。
    #[validate(custom(function = "non_blank", message = "来源商城不能为空"))]
    pub mall_id: String,
    /// 对应唯一 `T`。
    pub cutover_id: MallConsumptionCutoverId,
    /// 半开回填范围起点（秒级时间戳）。
    #[validate(range(min = 1, message = "范围起点必须大于 0"))]
    pub range_start: u64,
    /// 半开回填范围终点（必须等于本切换的 `T`，秒级时间戳）。
    #[validate(range(min = 1, message = "范围终点必须大于 0"))]
    pub range_end: u64,
    /// 来源统计总笔数。
    pub total_count: u64,
    /// 来源统计总金额（字符串）。
    #[validate(custom(function = "valid_amount", message = "总金额非法"))]
    pub total_amount: String,
}

/// 回填作业视图（W30 任务列表/详情）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BackfillJobView {
    /// 作业 ID。
    pub id: String,
    /// 来源商城。
    pub mall_id: String,
    /// 对应唯一 `T`。
    pub cutover_id: String,
    /// 半开回填范围起点（秒级时间戳）。
    pub range_start: u64,
    /// 半开回填范围终点（秒级时间戳）。
    pub range_end: u64,
    /// 作业状态。
    pub status: BackfillJobStatus,
    /// 来源统计总笔数。
    pub total_count: u64,
    /// 来源统计总金额（字符串）。
    pub total_amount: String,
    /// 重叠去重数量。
    pub deduplicated_count: u64,
    /// 实际成本口径笔数。
    pub actual_count: u64,
    /// 标准成本口径笔数。
    pub standard_count: u64,
    /// 无成本口径笔数。
    pub none_count: u64,
    /// 未归集数量。
    pub unattributed_count: u64,
    /// 可审计回填报告文件。
    pub report_file_id: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 回填作业列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct BackfillJobListParams {
    /// 来源商城模糊筛选。
    pub mall_id: Option<String>,
    /// 作业状态筛选。
    pub status: Option<BackfillJobStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`range_start`/`created_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的回填作业列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BackfillJobListQuery {
    /// 来源商城模糊筛选。
    pub mall_id: Option<String>,
    /// 作业状态筛选。
    pub status: Option<BackfillJobStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl BackfillJobListParams {
    /// 归一化回填作业列表查询参数。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<BackfillJobListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, BACKFILL_JOB_SORT_FIELDS)?;
        Ok(BackfillJobListQuery {
            mall_id: normalized_text(self.mall_id.as_deref()),
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

/// 回填作业命令请求（W30 §8.2：`START`/`RESUME`，乐观锁 + 幂等键）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct BackfillCommandRequest {
    /// 命令类型。
    pub command: BackfillCommand,
    /// 期望的乐观锁版本；与当前版本不一致时拒绝（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 操作幂等身份（结果未知时用于查询既有任务）。
    #[validate(custom(function = "non_blank", message = "操作ID不能为空"))]
    pub operation_id: String,
    /// 幂等键（与正式任务唯一绑定，重复提交不重复启动）。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 回填命令类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BackfillCommand {
    /// 开始回填（待执行 → 运行中，沿原任务和原范围执行）。
    Start,
    /// 续跑（部分完成/失败 → 运行中，已成功记录不回滚）。
    Resume,
}

/// 回填命令结果视图（W30 §8.2）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BackfillCommandResultView {
    /// 提交状态（恒为 `COMMITTED`；重复提交返回既有任务结果）。
    pub status: String,
    /// 任务 ID。
    pub job_id: String,
    /// 任务单号（P3 与作业 ID 同值，见契约变更）。
    pub job_no: String,
    /// 操作幂等身份。
    pub operation_id: String,
    /// 幂等键。
    pub idempotency_key: String,
    /// 下一步提示。
    pub next_step: String,
}

/// 回填作业详情视图（W30 任务详情）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BackfillJobDetailView {
    /// 作业视图（含进度统计）。
    pub job: BackfillJobView,
    /// 明细总笔数。
    pub item_total_count: i64,
}

/// 回填明细视图（W30 明细页）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BackfillItemView {
    /// 明细 ID。
    pub id: String,
    /// 回填批次。
    pub job_id: String,
    /// 事实身份。
    pub business_fact_key: String,
    /// 来源回填记录。
    pub source_event_reference: String,
    /// 形成的正式事实。
    pub mall_order_fact_id: Option<String>,
    /// 结果类型。
    pub result: BackfillItemResult,
    /// 成本口径。
    pub cost_basis: BackfillCostBasis,
    /// 失败原因代码。
    pub error_code: Option<String>,
    /// 失败原因详情。
    pub error_detail: Option<String>,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 回填明细列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct BackfillItemListParams {
    /// 回填批次筛选。
    pub job_id: Option<MallConsumptionBackfillJobId>,
    /// 结果类型筛选。
    pub result: Option<BackfillItemResult>,
    /// 成本口径筛选。
    pub cost_basis: Option<BackfillCostBasis>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`business_fact_key`/`created_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的回填明细列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BackfillItemListQuery {
    /// 回填批次筛选。
    pub job_id: Option<MallConsumptionBackfillJobId>,
    /// 结果类型筛选。
    pub result: Option<BackfillItemResult>,
    /// 成本口径筛选。
    pub cost_basis: Option<BackfillCostBasis>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl BackfillItemListParams {
    /// 归一化回填明细列表查询参数。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<BackfillItemListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, BACKFILL_ITEM_SORT_FIELDS)?;
        Ok(BackfillItemListQuery {
            job_id: self.job_id.clone(),
            result: self.result,
            cost_basis: self.cost_basis,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_sort;
    use crate::mall_backfill::dto::{BackfillItemListParams, BackfillJobListParams, SortDir};

    #[test]
    fn sort_whitelist_rejects_unknown_fields_and_directions() {
        assert!(normalize_sort(&Some("name".to_string()), &None, &["range_start"]).is_err());
        assert!(normalize_sort(&None, &Some("up".to_string()), &["range_start"]).is_err());

        let (field, direction) = normalize_sort(
            &Some(" range_start ".to_string()),
            &Some(" asc ".to_string()),
            &["range_start", "created_at"],
        )
        .unwrap();
        assert_eq!(field, "range_start");
        assert_eq!(direction, SortDir::Asc);
    }

    #[test]
    fn list_params_normalize_paging_filters_and_sort_defaults() {
        let params = BackfillJobListParams {
            mall_id: Some(" mall-a ".to_string()),
            status: None,
            page: None,
            page_size: None,
            sort_by: None,
            sort_dir: None,
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.mall_id.as_deref(), Some("mall-a"));
        assert_eq!(query.paging.page, 1);
        assert_eq!(query.paging.page_size, 20);
        assert_eq!(query.paging.sort_by, "created_at");

        let params = BackfillItemListParams {
            job_id: None,
            result: None,
            cost_basis: None,
            page: Some(2),
            page_size: Some(50),
            sort_by: Some("business_fact_key".to_string()),
            sort_dir: Some("asc".to_string()),
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.paging.page, 2);
        assert_eq!(query.paging.sort_by, "business_fact_key");
    }
}
