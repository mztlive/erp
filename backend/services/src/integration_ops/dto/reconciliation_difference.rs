use entities::integration_ops::{
    ReconciliationDifference, ReconciliationDifferenceResolution, ResolutionAction, ResultingStatus,
};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::Result;
use crate::query::{normalized_text, page_or_default, page_size_or_default};

use super::common::{non_blank, normalize_sort, PageParams};

/// `reconciliation_difference` 列表允许的排序字段白名单（仅差异发现时间）。
pub(crate) const DIFFERENCE_SORT_FIELDS: &[&str] = &["created_at"];

/// 对账差异登记请求（创建后不可修改，不设软删除）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateDifferenceRequest {
    /// 差异对象类型（如商城订单、供应商订单、销售单等）。
    #[validate(
        custom(function = "non_blank", message = "差异对象类型不能为空"),
        length(max = 64, message = "差异对象类型过长")
    )]
    pub business_object_type: String,
    /// 差异对象 ID。
    #[validate(
        custom(function = "non_blank", message = "差异对象ID不能为空"),
        length(max = 128, message = "差异对象ID过长")
    )]
    pub business_object_id: String,
    /// 差异分类。
    #[validate(
        custom(function = "non_blank", message = "差异分类不能为空"),
        length(max = 64, message = "差异分类过长")
    )]
    pub difference_type: String,
    /// 左侧不可变证据引用；两侧至少其一。
    #[validate(length(max = 512, message = "证据引用过长"))]
    pub left_fact_reference: Option<String>,
    /// 右侧不可变证据引用；两侧至少其一。
    #[validate(length(max = 512, message = "证据引用过长"))]
    pub right_fact_reference: Option<String>,
}

/// 对账差异列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct DifferenceListParams {
    /// 差异对象类型筛选。
    pub business_object_type: Option<String>,
    /// 差异对象 ID 筛选。
    pub business_object_id: Option<String>,
    /// 差异分类筛选。
    pub difference_type: Option<String>,
    /// 发现时间下界（秒级时间戳，含）。
    pub created_at_from: Option<i64>,
    /// 发现时间上界（秒级时间戳，含）。
    pub created_at_to: Option<i64>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的对账差异列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DifferenceListQuery {
    /// 差异对象类型筛选。
    pub business_object_type: Option<String>,
    /// 差异对象 ID 筛选。
    pub business_object_id: Option<String>,
    /// 差异分类筛选。
    pub difference_type: Option<String>,
    /// 发现时间下界。
    pub created_at_from: Option<i64>,
    /// 发现时间上界。
    pub created_at_to: Option<i64>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl DifferenceListParams {
    /// 归一化对账差异列表查询参数。
    ///
    /// 文本筛选去首尾空白、分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<DifferenceListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, DIFFERENCE_SORT_FIELDS)?;
        Ok(DifferenceListQuery {
            business_object_type: normalized_text(self.business_object_type.as_deref()),
            business_object_id: normalized_text(self.business_object_id.as_deref()),
            difference_type: normalized_text(self.difference_type.as_deref()),
            created_at_from: self.created_at_from,
            created_at_to: self.created_at_to,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 差异人工处理请求（非终态动作：领取/处理中/补证，只追加处理记录）。
///
/// 差异本身是不可变正式事实（锁版本永不变化），并发保护以「处理记录序号」为
/// 乐观锁令牌：`version` = 期望的最新处理序号（0 表示尚无处理记录），由上一次
/// 处理/解决响应回传；序号不一致或并发追加冲突时返回 409。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ProcessDifferenceRequest {
    /// 期望的最新处理序号（0 表示无处理记录；由上一次响应回传）。
    #[validate(range(max = 1_000_000, message = "处理序号不合法"))]
    pub version: Option<u64>,
    /// 处理动作：`claim` 领取（仅首条）/ `processing` 处理中 / `add_evidence` 补充证据。
    pub action: DifferenceProcessAction,
    /// 终态证据或补充证据引用（追加式，不可覆盖历史）。
    #[validate(length(max = 512, message = "证据引用过长"))]
    pub evidence_reference: Option<String>,
    /// 备注。
    #[validate(length(max = 512, message = "备注过长"))]
    pub comment: Option<String>,
}

/// 差异人工处理动作取值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DifferenceProcessAction {
    /// 领取（仅首条处理记录允许）。
    Claim,
    /// 处理中。
    Processing,
    /// 补充证据。
    AddEvidence,
}

/// 差异终结结论请求（只追加处理记录并派生终态；不完成/关闭任何任务）。
///
/// 并发保护同 [`ProcessDifferenceRequest`]：`version` 是期望的最新处理序号。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ResolveDifferenceRequest {
    /// 期望的最新处理序号（0 表示无处理记录；由上一次响应回传）。
    #[validate(range(max = 1_000_000, message = "处理序号不合法"))]
    pub version: Option<u64>,
    /// 终结结论：`confirm_no_error` 确认无误 / `confirm_valid_difference` 确认有效差异。
    pub conclusion: DifferenceConclusion,
    /// 固定原因枚举（禁止自由文本，W29 §7）。
    pub reason_code: DifferenceReasonCode,
    /// 受控证据引用（非空）。
    #[validate(
        custom(function = "non_blank", message = "受控证据不能为空"),
        length(max = 400, message = "证据引用过长")
    )]
    pub evidence_reference: String,
    /// 备注。
    #[validate(length(max = 512, message = "备注过长"))]
    pub comment: Option<String>,
}

/// 差异终结结论取值（`confirm_no_error` / `confirm_valid_difference`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DifferenceConclusion {
    /// 确认无误。
    ConfirmNoError,
    /// 确认有效差异。
    ConfirmValidDifference,
}

/// 差异终结固定原因枚举（W29 §7 至少含三项；禁止自由字符串原因）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DifferenceReasonCode {
    /// 来源已更正并重新归集。
    SourceCorrectedAndReattributed,
    /// 业务确认无误。
    BusinessConfirmedNoError,
    /// 已补偿闭环。
    CompensationClosed,
}

impl DifferenceReasonCode {
    /// 返回原因稳定代码。
    ///
    /// # 返回
    /// 返回用于写入处理记录证据引用的稳定字符串。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SourceCorrectedAndReattributed => "SOURCE_CORRECTED_AND_REATTRIBUTED",
            Self::BusinessConfirmedNoError => "BUSINESS_CONFIRMED_NO_ERROR",
            Self::CompensationClosed => "COMPENSATION_CLOSED",
        }
    }
}

/// 对账差异列表响应视图（`status` 由最后一条处理记录派生；无处理记录时为 `None`）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DifferenceView {
    /// 实体主键。
    pub id: String,
    /// 差异对象类型。
    pub business_object_type: String,
    /// 差异对象 ID。
    pub business_object_id: String,
    /// 差异分类。
    pub difference_type: String,
    /// 左侧不可变证据引用。
    pub left_fact_reference: Option<String>,
    /// 右侧不可变证据引用。
    pub right_fact_reference: Option<String>,
    /// 派生处理状态（`None` 表示尚无处理记录）。
    pub status: Option<ResultingStatus>,
    /// 乐观锁版本。
    pub version: u64,
    /// 差异发现时间（秒级时间戳）。
    pub created_at: u64,
}

/// 差异处理记录视图（不可变追加历史）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ResolutionView {
    /// 实体主键。
    pub id: String,
    /// 递增处理序号。
    pub resolution_no: u32,
    /// 解决动作。
    pub resolution_action: ResolutionAction,
    /// 动作后的派生状态。
    pub resulting_status: ResultingStatus,
    /// 终态证据引用。
    pub evidence_reference: Option<String>,
    /// 替代任务 ID（关闭重复时关联）。
    pub replacement_task_id: Option<String>,
    /// 处理人。
    pub handled_by: String,
    /// 处理时间（秒级时间戳）。
    pub handled_at: i64,
}

impl From<ReconciliationDifferenceResolution> for ResolutionView {
    /// 从处理记录实体构造视图。
    ///
    /// # 参数
    /// * `resolution` - 处理记录实体
    ///
    /// # 返回
    /// 返回处理记录视图。
    fn from(resolution: ReconciliationDifferenceResolution) -> Self {
        Self {
            id: resolution.base.id,
            resolution_no: resolution.resolution_no,
            resolution_action: resolution.resolution_action,
            resulting_status: resolution.resulting_status,
            evidence_reference: resolution.evidence_reference,
            replacement_task_id: resolution.replacement_task_id.map(|id| id.to_string()),
            handled_by: resolution.handled_by,
            handled_at: resolution.handled_at.unix_secs(),
        }
    }
}

/// 对账差异详情响应视图（差异字段 + 全部处理记录时间线）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DifferenceDetailView {
    /// 差异字段（扁平展开）。
    #[serde(flatten)]
    pub difference: DifferenceView,
    /// 处理记录时间线（按处理序号升序，不可变）。
    pub resolutions: Vec<ResolutionView>,
}

/// 差异处理动作响应视图（追加的处理记录 + 最新处理序号，供下一次动作乐观锁回传）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DifferenceActionView {
    /// 本次追加的处理记录（扁平展开）。
    #[serde(flatten)]
    pub resolution: ResolutionView,
    /// 追加后的最新处理序号（下一次处理/解决请求的 `version`）。
    pub version: u64,
}

impl From<ReconciliationDifference> for DifferenceView {
    /// 从差异实体构造视图（无处理记录时状态为 `None`）。
    ///
    /// # 参数
    /// * `difference` - 差异实体
    ///
    /// # 返回
    /// 返回差异视图。
    fn from(difference: ReconciliationDifference) -> Self {
        Self {
            id: difference.base.id,
            business_object_type: difference.business_object_type,
            business_object_id: difference.business_object_id,
            difference_type: difference.difference_type,
            left_fact_reference: difference.left_fact_reference,
            right_fact_reference: difference.right_fact_reference,
            status: None,
            version: difference.base.version,
            created_at: difference.base.created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DifferenceListParams, DifferenceReasonCode};
    use validator::Validate;

    #[test]
    fn difference_list_params_normalize_and_reject_unbounded_page_size() {
        let params = DifferenceListParams {
            business_object_type: Some(" mall_order ".to_string()),
            business_object_id: None,
            difference_type: None,
            created_at_from: None,
            created_at_to: None,
            page: Some(0),
            page_size: Some(u32::MAX),
            sort_by: None,
            sort_dir: None,
        };
        assert!(params.validate().is_err());

        let params = DifferenceListParams {
            business_object_type: Some(" mall_order ".to_string()),
            business_object_id: Some("MO-1".to_string()),
            difference_type: Some("amount_mismatch".to_string()),
            created_at_from: Some(1_700_000_000),
            created_at_to: Some(1_700_000_100),
            page: Some(3),
            page_size: Some(10),
            sort_by: Some("created_at".to_string()),
            sort_dir: Some("desc".to_string()),
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.business_object_type.as_deref(), Some("mall_order"));
        assert_eq!(query.business_object_id.as_deref(), Some("MO-1"));
        assert_eq!(query.created_at_to, Some(1_700_000_100));
        assert_eq!(query.paging.page, 3);
        assert_eq!(query.paging.page_size, 10);
    }

    #[test]
    fn reason_code_serializes_with_stable_codes() {
        use serde_json::json;

        assert_eq!(
            serde_json::to_string(&DifferenceReasonCode::SourceCorrectedAndReattributed).unwrap(),
            "\"SOURCE_CORRECTED_AND_REATTRIBUTED\""
        );
        assert_eq!(
            DifferenceReasonCode::CompensationClosed.as_str(),
            "COMPENSATION_CLOSED"
        );

        let parsed: DifferenceReasonCode =
            serde_json::from_value(json!("BUSINESS_CONFIRMED_NO_ERROR")).unwrap();
        assert_eq!(parsed, DifferenceReasonCode::BusinessConfirmedNoError);
    }
}
