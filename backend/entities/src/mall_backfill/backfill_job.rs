//! `mall_consumption_backfill_job` 与 `mall_consumption_backfill_item`：历史消费回填
//! 作业与明细（数据模型 §6.17）。
//!
//! 回填作业覆盖半开范围 `[range_start, range_end)`，其中 `range_end` 必须等于本切换
//! 的 `T`（跨实体一致性，依赖切换表查询，由 P3 校验：P3 条目 §6.17 回填范围与
//! 切换一致）。作业状态沿固定邻接推进（见 [`BackfillJobStatus`]）。
//!
//! `cost_basis`（`ACTUAL`/`STANDARD`/`NONE`）与 D29 消费成本评估共用同一取值集合；
//! 按 P1 §3 跨域规则本域内保留自身定义，若需统一下沉为共享值对象，列为
//! 「地基修订候选」（`entities/src/common/` 增加 `CostBasis` 并迁移两域引用）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::state::{ensure_transition, DocumentState};
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{
    FileAssetId, InboxMessageId, MallConsumptionBackfillItemId, MallConsumptionBackfillJobId,
    MallConsumptionCutoverId, MallOrderFactId,
};
use crate::money::Amount;
use crate::validation::{normalize_optional_text, normalize_required_text};

/// 目标商城代码最大长度。
const MALL_ID_MAX_LEN: usize = 64;
/// 业务事实键最大长度。
const BUSINESS_FACT_KEY_MAX_LEN: usize = 256;
/// 来源回填记录引用最大长度。
const SOURCE_EVENT_REF_MAX_LEN: usize = 256;
/// 错误码最大长度。
const ERROR_CODE_MAX_LEN: usize = 64;
/// 错误详情最大长度。
const ERROR_DETAIL_MAX_LEN: usize = 512;

/// 回填作业状态（数据模型 §6.17：待执行、运行中、部分完成、完成、失败）。
///
/// 固定邻接：待执行 → 运行中；运行中 → 部分完成 | 完成 | 失败；部分完成 →
/// 运行中（沿原任务和原范围续跑）| 完成 | 失败；失败 → 运行中（重跑沿原任务
/// 和原范围续跑）；完成为不可逆终态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackfillJobStatus {
    /// 待执行。
    Pending,
    /// 运行中。
    Running,
    /// 部分完成。
    PartiallyCompleted,
    /// 完成（不可逆终态）。
    Completed,
    /// 失败。
    Failed,
}

impl BackfillJobStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Pending => "待执行",
            Self::Running => "运行中",
            Self::PartiallyCompleted => "部分完成",
            Self::Completed => "完成",
            Self::Failed => "失败",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::PartiallyCompleted => "partially_completed",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

impl DocumentState for BackfillJobStatus {
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Pending => &[Self::Running],
            Self::Running => &[Self::PartiallyCompleted, Self::Completed, Self::Failed],
            Self::PartiallyCompleted => &[Self::Running, Self::Completed, Self::Failed],
            Self::Failed => &[Self::Running],
            Self::Completed => &[],
        }
    }
}

/// 回填明细结果类型（数据模型 §6.17：新增、重复、待归集、失败）。
///
/// 固定枚举：明细结果在回填执行时一次性确定，不定义状态迁移。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackfillItemResult {
    /// 新增：已形成正式事实。
    New,
    /// 重复：与实时或其他批次重叠去重。
    Duplicate,
    /// 待归集：事实已保存但归集条件暂缺。
    PendingAttribution,
    /// 失败：未通过接收校验。
    Failed,
}

impl BackfillItemResult {
    /// 返回结果类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::New => "新增",
            Self::Duplicate => "重复",
            Self::PendingAttribution => "待归集",
            Self::Failed => "失败",
        }
    }

    /// 返回结果类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Duplicate => "duplicate",
            Self::PendingAttribution => "pending_attribution",
            Self::Failed => "failed",
        }
    }
}

/// 回填成本口径（数据模型 §6.17：`ACTUAL`、`STANDARD`、`NONE`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BackfillCostBasis {
    /// 实际成本：商城订单快照成本。
    Actual,
    /// 标准成本：消费时点供给版本价。
    Standard,
    /// 无成本：商城无成本且无有效供给版本。
    None,
}

impl BackfillCostBasis {
    /// 返回成本口径的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Actual => "实际成本",
            Self::Standard => "标准成本",
            Self::None => "无成本",
        }
    }

    /// 返回成本口径的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Actual => "ACTUAL",
            Self::Standard => "STANDARD",
            Self::None => "NONE",
        }
    }
}

/// 回填作业创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MallConsumptionBackfillJobData {
    /// 来源商城。
    pub mall_id: String,
    /// 对应唯一 `T`。
    pub cutover_id: MallConsumptionCutoverId,
    /// 半开回填范围起点。
    pub range_start: Instant,
    /// 半开回填范围终点（必须等于本切换的 `T`）。
    pub range_end: Instant,
    /// 来源统计总笔数。
    pub total_count: u64,
    /// 来源统计总金额。
    pub total_amount: Amount,
}

/// 回填作业实体（数据模型 §6.17）。
///
/// 创建时状态为 `Pending`，各统计计数为 0；执行过程中沿固定邻接推进状态并
/// 更新统计计数与回填报告。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct MallConsumptionBackfillJob {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 来源商城。
    pub mall_id: String,
    /// 对应唯一 `T`。
    pub cutover_id: MallConsumptionCutoverId,
    /// 半开回填范围起点。
    pub range_start: Instant,
    /// 半开回填范围终点。
    pub range_end: Instant,
    /// 作业状态。
    pub status: BackfillJobStatus,
    /// 来源统计总笔数。
    pub total_count: u64,
    /// 来源统计总金额。
    pub total_amount: Amount,
    /// 与实时或其他批次重叠去重数量。
    pub deduplicated_count: u64,
    /// 实际成本口径笔数。
    pub actual_count: u64,
    /// 标准成本口径笔数。
    pub standard_count: u64,
    /// 无成本口径笔数。
    pub none_count: u64,
    /// 未归集数量。
    pub unattributed_count: u64,
    /// 可审计回填报告。
    pub report_file_id: Option<FileAssetId>,
}

impl MallConsumptionBackfillJob {
    /// 创建回填作业。
    ///
    /// 完成 mall_id 校验与规范化；`range_end` 必须晚于 `range_start`（半开范围
    /// 非空）；`total_amount` 必须非负。状态固定为 `Pending`。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::MallConsumptionBackfillJobId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的回填作业实体。
    ///
    /// # 错误
    /// 当 mall_id 为空/超长、范围倒挂或总金额为负时返回错误。
    pub fn new(id: MallConsumptionBackfillJobId, data: MallConsumptionBackfillJobData) -> Result<Self> {
        let mall_id = normalize_required_text(
            data.mall_id,
            "来源商城不能为空",
            MALL_ID_MAX_LEN,
            "来源商城代码过长",
        )?;
        if data.range_end <= data.range_start {
            return Err(Error::from("回填范围终点必须晚于起点"));
        }
        if data.total_amount.to_decimal().is_sign_negative() {
            return Err(Error::from("回填总金额不能为负"));
        }

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            mall_id,
            cutover_id: data.cutover_id,
            range_start: data.range_start,
            range_end: data.range_end,
            status: BackfillJobStatus::Pending,
            total_count: data.total_count,
            total_amount: data.total_amount,
            deduplicated_count: 0,
            actual_count: 0,
            standard_count: 0,
            none_count: 0,
            unattributed_count: 0,
            report_file_id: None,
        })
    }

    /// 推进作业状态。
    ///
    /// 沿固定邻接推进（§6.17）；完成为不可逆终态。重跑/续跑沿原任务和原范围，
    /// 不能用另一正式批次制造重叠（P3 调度校验）。
    ///
    /// # 参数
    /// * `to` - 目标状态
    ///
    /// # 返回
    /// 迁移合法返回 `Ok(())`。
    ///
    /// # 错误
    /// 目标不在后继列表中且与当前状态不同时返回 `InvalidStateTransition`。
    pub fn transition_to(&mut self, to: BackfillJobStatus) -> Result<()> {
        ensure_transition(self.status, to)?;
        self.status = to;
        Ok(())
    }

    /// 更新执行进度统计与回填报告。
    ///
    /// 统计口径（§6.17）：重叠去重、实际/标准/无成本笔数与未归集数量。
    /// 任一计数不得为负、不得超过来源总笔数；五项合计（含去重）不得超过
    /// 来源总笔数。已完成作业不允许再更新。
    ///
    /// # 参数
    /// * `deduplicated_count` - 重叠去重数量
    /// * `actual_count` - 实际成本口径笔数
    /// * `standard_count` - 标准成本口径笔数
    /// * `none_count` - 无成本口径笔数
    /// * `unattributed_count` - 未归集数量
    /// * `report_file_id` - 可审计回填报告（`None` 表示不修改）
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 作业已完成、任一计数越界或五项合计超过总笔数时返回错误。
    pub fn update_progress(
        &mut self,
        deduplicated_count: u64,
        actual_count: u64,
        standard_count: u64,
        none_count: u64,
        unattributed_count: u64,
        report_file_id: Option<FileAssetId>,
    ) -> Result<()> {
        if self.status == BackfillJobStatus::Completed {
            return Err(Error::from("已完成作业不允许再更新进度"));
        }
        let counted = deduplicated_count + actual_count + standard_count + none_count + unattributed_count;
        if counted > self.total_count {
            return Err(Error::from("各口径统计合计不得超过来源总笔数"));
        }

        self.deduplicated_count = deduplicated_count;
        self.actual_count = actual_count;
        self.standard_count = standard_count;
        self.none_count = none_count;
        self.unattributed_count = unattributed_count;
        if report_file_id.is_some() {
            self.report_file_id = report_file_id;
        }
        Ok(())
    }
}

/// 回填明细创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MallConsumptionBackfillItemData {
    /// 回填批次。
    pub job_id: MallConsumptionBackfillJobId,
    /// 事实身份。
    pub business_fact_key: String,
    /// 来源回填记录。
    pub source_event_reference: String,
    /// 统一接收。
    pub inbox_message_id: InboxMessageId,
    /// 形成的正式事实。
    pub mall_order_fact_id: Option<MallOrderFactId>,
    /// 结果类型。
    pub result: BackfillItemResult,
    /// 成本口径。
    pub cost_basis: BackfillCostBasis,
    /// 失败原因代码。
    pub error_code: Option<String>,
    /// 失败原因详情。
    pub error_detail: Option<String>,
}

/// 回填明细实体（数据模型 §6.17）。
///
/// 结果类型与成本口径在回填执行时一次性确定，不可变，只提供 `new()`。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct MallConsumptionBackfillItem {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 回填批次。
    pub job_id: MallConsumptionBackfillJobId,
    /// 事实身份。
    pub business_fact_key: String,
    /// 来源回填记录。
    pub source_event_reference: String,
    /// 统一接收。
    pub inbox_message_id: InboxMessageId,
    /// 形成的正式事实。
    pub mall_order_fact_id: Option<MallOrderFactId>,
    /// 结果类型。
    pub result: BackfillItemResult,
    /// 成本口径。
    pub cost_basis: BackfillCostBasis,
    /// 失败原因代码。
    pub error_code: Option<String>,
    /// 失败原因详情。
    pub error_detail: Option<String>,
}

impl MallConsumptionBackfillItem {
    /// 创建回填明细。
    ///
    /// 强制结果一致性（§6.17）：
    /// - `Failed` 必须携带 `error_code`，且不得携带正式事实；
    /// - `New` 必须携带正式事实，且不得携带错误信息；
    /// - `Duplicate` 不得携带正式事实与错误信息；
    /// - `PendingAttribution` 不得携带错误信息。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::MallConsumptionBackfillItemId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的回填明细实体。
    ///
    /// # 错误
    /// 当必填文本为空/超长或结果与引用字段不一致时返回错误。
    pub fn new(id: MallConsumptionBackfillItemId, data: MallConsumptionBackfillItemData) -> Result<Self> {
        let business_fact_key = normalize_required_text(
            data.business_fact_key,
            "业务事实键不能为空",
            BUSINESS_FACT_KEY_MAX_LEN,
            "业务事实键过长",
        )?;
        let source_event_reference = normalize_required_text(
            data.source_event_reference,
            "来源回填记录不能为空",
            SOURCE_EVENT_REF_MAX_LEN,
            "来源回填记录过长",
        )?;
        let error_code = normalize_optional_text(data.error_code, "错误码", ERROR_CODE_MAX_LEN)?;
        let error_detail = normalize_optional_text(data.error_detail, "错误详情", ERROR_DETAIL_MAX_LEN)?;
        validate_item_result(
            data.result,
            &error_code,
            &error_detail,
            data.mall_order_fact_id.clone(),
        )?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            job_id: data.job_id,
            business_fact_key,
            source_event_reference,
            inbox_message_id: data.inbox_message_id,
            mall_order_fact_id: data.mall_order_fact_id,
            result: data.result,
            cost_basis: data.cost_basis,
            error_code,
            error_detail,
        })
    }
}

/// 校验明细结果与错误信息、正式事实引用的组合一致性。
///
/// # 参数
/// * `result` - 结果类型
/// * `error_code` - 错误码（已规范化）
/// * `error_detail` - 错误详情（已规范化）
/// * `mall_order_fact_id` - 正式事实引用
///
/// # 返回
/// 组合一致返回 `Ok(())`。
///
/// # 错误
/// `Failed` 缺错误码/带事实，`New` 缺事实，`Duplicate` 带事实或错误信息时返回错误。
fn validate_item_result(
    result: BackfillItemResult,
    error_code: &Option<String>,
    error_detail: &Option<String>,
    mall_order_fact_id: Option<MallOrderFactId>,
) -> Result<()> {
    let has_error = error_code.is_some() || error_detail.is_some();
    match result {
        BackfillItemResult::Failed => {
            if error_code.is_none() {
                return Err(Error::from("失败明细必须携带错误码"));
            }
            if mall_order_fact_id.is_some() {
                return Err(Error::from("失败明细不得携带正式事实"));
            }
        }
        BackfillItemResult::New => {
            if mall_order_fact_id.is_none() {
                return Err(Error::from("新增明细必须携带形成的正式事实"));
            }
            if has_error {
                return Err(Error::from("新增明细不得携带错误信息"));
            }
        }
        BackfillItemResult::Duplicate => {
            if mall_order_fact_id.is_some() {
                return Err(Error::from("重复明细不得携带正式事实"));
            }
            if has_error {
                return Err(Error::from("重复明细不得携带错误信息"));
            }
        }
        BackfillItemResult::PendingAttribution => {
            if has_error {
                return Err(Error::from("待归集明细不得携带错误信息"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        BackfillCostBasis, BackfillItemResult, BackfillJobStatus, MallConsumptionBackfillItem,
        MallConsumptionBackfillItemData, MallConsumptionBackfillJob, MallConsumptionBackfillJobData,
    };
    use crate::common::state::{ensure_transition, DocumentState};
    use crate::common::time::Instant;
    use crate::ids::{
        FileAssetId, InboxMessageId, MallConsumptionBackfillItemId, MallConsumptionBackfillJobId,
        MallConsumptionCutoverId, MallOrderFactId,
    };
    use crate::money::Amount;
    use std::str::FromStr;

    fn job_data() -> MallConsumptionBackfillJobData {
        MallConsumptionBackfillJobData {
            mall_id: " mall-a ".to_string(),
            cutover_id: MallConsumptionCutoverId::new("cutover-1"),
            range_start: Instant::from_unix_secs(1_600_000_000),
            range_end: Instant::from_unix_secs(1_700_000_000),
            total_count: 100,
            total_amount: Amount::from_str("5000.00").unwrap(),
        }
    }

    fn item_data(result: BackfillItemResult) -> MallConsumptionBackfillItemData {
        MallConsumptionBackfillItemData {
            job_id: MallConsumptionBackfillJobId::new("job-1"),
            business_fact_key: " mall-a:PAYMENT:SO-9:v1 ".to_string(),
            source_event_reference: " src-9 ".to_string(),
            inbox_message_id: InboxMessageId::new("inbox-9"),
            mall_order_fact_id: match result {
                BackfillItemResult::New => Some(MallOrderFactId::new("fact-9")),
                _ => None,
            },
            result,
            cost_basis: BackfillCostBasis::Actual,
            error_code: match result {
                BackfillItemResult::Failed => Some(" E_9001 ".to_string()),
                _ => None,
            },
            error_detail: match result {
                BackfillItemResult::Failed => Some(" 事实键重复 ".to_string()),
                _ => None,
            },
        }
    }

    /// happy path：作业创建为待执行，范围与统计落库。
    #[test]
    fn job_new_trims_fields_and_starts_pending() {
        let job =
            MallConsumptionBackfillJob::new(MallConsumptionBackfillJobId::new("job-1"), job_data()).unwrap();

        assert_eq!(job.mall_id, "mall-a");
        assert_eq!(job.status, BackfillJobStatus::Pending);
        assert_eq!(job.total_count, 100);
        assert_eq!(job.total_amount, Amount::from_str("5000.00").unwrap());
        assert_eq!(job.deduplicated_count, 0);
        assert_eq!(job.actual_count, 0);
        assert!(job.report_file_id.is_none());
    }

    /// 失败路径：必填空、超长、范围倒挂、负金额。
    #[test]
    fn job_new_rejects_blank_overlong_inverted_range_and_negative_amount() {
        let blank = MallConsumptionBackfillJobData {
            mall_id: "  ".to_string(),
            ..job_data()
        };
        assert!(MallConsumptionBackfillJob::new(MallConsumptionBackfillJobId::new("j2"), blank).is_err());

        let overlong = MallConsumptionBackfillJobData {
            mall_id: "x".repeat(65),
            ..job_data()
        };
        assert!(MallConsumptionBackfillJob::new(MallConsumptionBackfillJobId::new("j3"), overlong).is_err());

        let inverted = MallConsumptionBackfillJobData {
            range_end: Instant::from_unix_secs(1_500_000_000),
            ..job_data()
        };
        assert!(MallConsumptionBackfillJob::new(MallConsumptionBackfillJobId::new("j4"), inverted).is_err());

        let negative = MallConsumptionBackfillJobData {
            total_amount: Amount::from_str("-1.00").unwrap(),
            ..job_data()
        };
        assert!(MallConsumptionBackfillJob::new(MallConsumptionBackfillJobId::new("j5"), negative).is_err());
    }

    /// 状态机：合法/非法迁移与终态定向断言。
    #[test]
    fn job_status_machine_directed_edges() {
        assert!(ensure_transition(BackfillJobStatus::Pending, BackfillJobStatus::Running).is_ok());
        assert!(ensure_transition(BackfillJobStatus::Running, BackfillJobStatus::PartiallyCompleted).is_ok());
        assert!(ensure_transition(BackfillJobStatus::Running, BackfillJobStatus::Failed).is_ok());
        assert!(ensure_transition(BackfillJobStatus::PartiallyCompleted, BackfillJobStatus::Running).is_ok());
        assert!(ensure_transition(BackfillJobStatus::Failed, BackfillJobStatus::Running).is_ok());
        assert!(ensure_transition(BackfillJobStatus::Running, BackfillJobStatus::Completed).is_ok());

        assert!(ensure_transition(BackfillJobStatus::Pending, BackfillJobStatus::Completed).is_err());
        assert!(ensure_transition(BackfillJobStatus::Completed, BackfillJobStatus::Running).is_err());
        assert!(ensure_transition(BackfillJobStatus::Pending, BackfillJobStatus::Failed).is_err());
        assert_eq!(
            BackfillJobStatus::Completed.allowed_next(),
            &[] as &[BackfillJobStatus]
        );
    }

    /// 状态机 + 进度：作业沿固定邻接推进，已完成禁止更新进度。
    #[test]
    fn job_transitions_and_progress_updates() {
        let mut job =
            MallConsumptionBackfillJob::new(MallConsumptionBackfillJobId::new("job-1"), job_data()).unwrap();

        job.transition_to(BackfillJobStatus::Running).unwrap();
        job.update_progress(5, 60, 20, 10, 5, Some(FileAssetId::new("report-1")))
            .unwrap();
        assert_eq!(job.actual_count, 60);
        assert_eq!(job.report_file_id, Some(FileAssetId::new("report-1")));

        assert!(
            job.update_progress(5, 60, 20, 10, 6, None).is_err(),
            "五项合计超过总笔数"
        );
        assert!(
            job.update_progress(101, 0, 0, 0, 0, None).is_err(),
            "单项超过总笔数"
        );

        job.transition_to(BackfillJobStatus::Completed).unwrap();
        assert!(
            job.update_progress(5, 60, 20, 10, 5, None).is_err(),
            "已完成禁止更新"
        );
        assert!(job.transition_to(BackfillJobStatus::Running).is_err());
    }

    /// 明细：happy path（新增/重复/待归集/失败各形态规范化）。
    #[test]
    fn item_new_trims_fields_by_result_kind() {
        let new_item = MallConsumptionBackfillItem::new(
            MallConsumptionBackfillItemId::new("bi-1"),
            item_data(BackfillItemResult::New),
        )
        .unwrap();
        assert_eq!(new_item.business_fact_key, "mall-a:PAYMENT:SO-9:v1");
        assert_eq!(new_item.source_event_reference, "src-9");
        assert_eq!(new_item.mall_order_fact_id, Some(MallOrderFactId::new("fact-9")));
        assert!(new_item.error_code.is_none());

        let failed_item = MallConsumptionBackfillItem::new(
            MallConsumptionBackfillItemId::new("bi-2"),
            item_data(BackfillItemResult::Failed),
        )
        .unwrap();
        assert_eq!(failed_item.error_code.as_deref(), Some("E_9001"));
        assert_eq!(failed_item.error_detail.as_deref(), Some("事实键重复"));
        assert!(failed_item.mall_order_fact_id.is_none());
    }

    /// 失败路径：必填空、超长、结果与引用不一致。
    #[test]
    fn item_new_rejects_blank_overlong_and_result_mismatch() {
        let blank = MallConsumptionBackfillItemData {
            business_fact_key: "  ".to_string(),
            ..item_data(BackfillItemResult::New)
        };
        assert!(MallConsumptionBackfillItem::new(MallConsumptionBackfillItemId::new("bi-3"), blank).is_err());

        let overlong = MallConsumptionBackfillItemData {
            source_event_reference: "s".repeat(257),
            ..item_data(BackfillItemResult::New)
        };
        assert!(
            MallConsumptionBackfillItem::new(MallConsumptionBackfillItemId::new("bi-4"), overlong).is_err()
        );

        let failed_without_code = MallConsumptionBackfillItemData {
            error_code: None,
            ..item_data(BackfillItemResult::Failed)
        };
        assert!(MallConsumptionBackfillItem::new(
            MallConsumptionBackfillItemId::new("bi-5"),
            failed_without_code
        )
        .is_err());

        let failed_with_fact = MallConsumptionBackfillItemData {
            mall_order_fact_id: Some(MallOrderFactId::new("fact-9")),
            ..item_data(BackfillItemResult::Failed)
        };
        assert!(MallConsumptionBackfillItem::new(
            MallConsumptionBackfillItemId::new("bi-6"),
            failed_with_fact
        )
        .is_err());

        let new_without_fact = MallConsumptionBackfillItemData {
            mall_order_fact_id: None,
            ..item_data(BackfillItemResult::New)
        };
        assert!(MallConsumptionBackfillItem::new(
            MallConsumptionBackfillItemId::new("bi-7"),
            new_without_fact
        )
        .is_err());

        let duplicate_with_error = MallConsumptionBackfillItemData {
            error_code: Some("E_1".to_string()),
            ..item_data(BackfillItemResult::Duplicate)
        };
        assert!(MallConsumptionBackfillItem::new(
            MallConsumptionBackfillItemId::new("bi-8"),
            duplicate_with_error
        )
        .is_err());
    }
}
