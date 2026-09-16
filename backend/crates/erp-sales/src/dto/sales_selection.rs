//! 销售选品 HTTP/Service DTO。Handler 直接复用本模块类型。

use application_core::PageView;
pub use application_core::PageView as SelectionPageView;
use erp_core::common::time::{BusinessDate, Instant};
use erp_core::money::Amount;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::sales_selection::{
    BookletStatus, PoolFilterSnapshot, PoolSourceKind, PrepareKind, PrepareStage, SearchStopReason,
    SelectionForm, SubmitMode, TierRule,
};

/// 创建选品册。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateSalesSelectionBookletRequest {
    /// 幂等键。
    pub idempotency_key: String,
    /// 客户身份。
    pub customer_id: String,
    /// 显式销售负责人；必填，客户提交人不成为负责人。
    pub sales_owner_user_id: String,
    /// 业务组织；必填，取负责人有效主属组织。
    pub business_org_unit_id: String,
    /// 选品形态。
    #[serde(alias = "selection_form")]
    pub form: SelectionForm,
    /// 提交方式。
    pub submit_mode: SubmitMode,
    /// 商品池来源类型。
    #[serde(alias = "source_kind", alias = "pool_source_kind")]
    pub pool_source_kind: PoolSourceKind,
    /// 筛选条件；筛选来源必填。
    #[serde(alias = "filter", alias = "pool_filter")]
    pub pool_filter: Option<PoolFilterSnapshot>,
    /// 勾选 SKU；勾选来源必填。
    pub sku_ids: Option<Vec<String>>,
    /// 套餐档位；单品可省略。
    #[serde(default)]
    pub tiers: Vec<CreateTierRequest>,
}

/// 创建档位。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateTierRequest {
    /// 名称。
    pub name: String,
    /// 目标金额。
    pub target_amount: Amount,
    /// 容差，必须显式填写。
    pub tolerance: Amount,
    /// 期望套餐数。
    pub expected_count: u32,
    /// 每套餐 SKU 数。
    pub sku_count: u32,
}

/// 启动准备。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PrepareSalesSelectionRequest {
    /// 幂等键。
    pub idempotency_key: String,
    /// 选品册身份；路径参数优先，请求体可缺省。
    #[serde(default)]
    pub booklet_id: String,
    /// 册版本。
    pub expected_version: u64,
    /// 准备种类。
    pub kind: PrepareKind,
    /// 按档重生成的档位身份。
    #[serde(default)]
    pub tier_ids: Vec<String>,
    /// 搜索种子；重生成可变，不改变快照与规则。
    #[serde(default)]
    pub seed: u64,
    /// 整册重新准备时的筛选；缺省沿用原规则。
    #[serde(alias = "filter", alias = "pool_filter")]
    pub pool_filter: Option<PoolFilterSnapshot>,
    /// 整册重新准备时的勾选。
    pub sku_ids: Option<Vec<String>>,
    /// 整册重新准备时的档位。
    #[serde(default)]
    pub tiers: Vec<CreateTierRequest>,
}

impl PrepareSalesSelectionRequest {
    /// 构造创建后立刻排队的首次准备命令。
    ///
    /// 商品池和档位沿用刚写入的册，不重复提交筛选或勾选。
    ///
    /// # 参数
    /// * `booklet_id` - 刚创建的选品册身份
    /// * `expected_version` - 创建落库后的册版本
    /// * `idempotency_key` - 与创建请求相同的幂等键；准备操作域单独记账
    ///
    /// # 返回
    /// 返回首次准备命令。
    ///
    /// # 错误
    /// 无。
    pub fn first_prepare(
        booklet_id: impl Into<String>,
        expected_version: u64,
        idempotency_key: impl Into<String>,
    ) -> Self {
        Self {
            idempotency_key: idempotency_key.into(),
            booklet_id: booklet_id.into(),
            expected_version,
            kind: PrepareKind::FirstPrepare,
            tier_ids: Vec::new(),
            seed: 0,
            pool_filter: None,
            sku_ids: None,
            tiers: Vec::new(),
        }
    }
}

/// 删除陈列项。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct DeleteDisplayItemRequest {
    /// 册版本。
    pub expected_version: u64,
}

/// 发布。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PublishSalesSelectionRequest {
    /// 幂等键。
    pub idempotency_key: String,
    /// 册版本。
    pub expected_version: u64,
    /// 已确认的准备批次；缺省时拒绝发布并要求重新预览。
    pub batch_id: Option<String>,
}

/// 带幂等与版本的命令。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SalesSelectionCommandRequest {
    /// 幂等键。
    pub idempotency_key: String,
    /// 册版本。
    pub expected_version: u64,
}

/// 列表筛选。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
pub struct SalesSelectionBookletListParams {
    /// 服务端注入的客户访问范围，拒绝客户端覆盖。
    #[serde(skip)]
    pub authorized_customer_ids: Option<Vec<String>>,
    /// 服务端注入的选品责任范围版本；跨页必须原样回传。
    #[serde(default, deserialize_with = "deserialize_optional_csv")]
    pub scope_version: Option<String>,
    /// 当前业务负责人；去重后的稳定人员 ID，最多 100 个。
    #[serde(default, deserialize_with = "deserialize_optional_csv_vec")]
    pub owner_user_ids: Option<Vec<String>>,
    /// 当前组织筛选；与册业务组织口径一致，最多 100 个。
    #[serde(default, deserialize_with = "deserialize_optional_csv_vec")]
    pub org_unit_ids: Option<Vec<String>>,
    /// 是否包含有效下级；缺省为 false。
    pub include_descendants: Option<bool>,
    /// 客户。
    pub customer_id: Option<String>,
    /// 形态。
    #[serde(alias = "selection_form")]
    pub form: Option<SelectionForm>,
    /// 状态。
    pub status: Option<BookletStatus>,
    /// 提交方式。
    pub submit_mode: Option<SubmitMode>,
    /// 客户名称关键字。
    pub q: Option<String>,
    /// 页码。
    #[validate(range(min = 1))]
    pub page: Option<u64>,
    /// 页大小。
    #[validate(range(min = 1, max = 100))]
    pub page_size: Option<u32>,
}

/// 方案列表。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SalesSelectionProposalListParams {
    /// 服务端注入的客户访问范围，拒绝客户端覆盖。
    #[serde(skip)]
    pub authorized_customer_ids: Option<Vec<String>>,
    /// 服务端注入的方案责任范围版本；跨页必须原样回传。
    #[serde(default, deserialize_with = "deserialize_optional_csv")]
    pub scope_version: Option<String>,
    /// 当前业务负责人；去重后的稳定人员 ID，最多 100 个。
    #[serde(default, deserialize_with = "deserialize_optional_csv_vec")]
    pub owner_user_ids: Option<Vec<String>>,
    /// 当前组织筛选；与方案业务组织口径一致，最多 100 个。
    #[serde(default, deserialize_with = "deserialize_optional_csv_vec")]
    pub org_unit_ids: Option<Vec<String>>,
    /// 是否包含有效下级；缺省为 false。
    pub include_descendants: Option<bool>,
    /// 客户。
    pub customer_id: Option<String>,
    /// 选品册；指定时只返回该册唯一方案。
    #[serde(alias = "book_id")]
    pub booklet_id: Option<String>,
    /// 页码。
    #[validate(range(min = 1))]
    pub page: Option<u64>,
    /// 页大小。
    #[validate(range(min = 1, max = 100))]
    pub page_size: Option<u32>,
}

/// 公开保存会话。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SaveSelectionSessionRequest {
    /// 幂等键。
    pub idempotency_key: String,
    /// 预期会话版本。
    #[serde(alias = "expected_version", alias = "expected_session_version")]
    pub expected_session_version: u64,
    /// 完整选择。
    #[serde(alias = "selections", alias = "choices")]
    pub choices: Vec<PublicChoiceRequest>,
}

/// 公开提交。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SubmitSelectionSessionRequest {
    /// 幂等键。
    pub idempotency_key: String,
    /// 预期会话版本。
    pub expected_session_version: u64,
}

/// 公开选择项。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PublicChoiceRequest {
    /// 陈列项公开身份。
    #[serde(alias = "display_id", alias = "item_id")]
    pub item_id: String,
    /// 份数。
    pub quantity: Option<u32>,
}

/// 选品册列表行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesSelectionBookletListItemView {
    /// 身份。
    pub id: String,
    /// 与 `id` 相同，兼容内部列表字段。
    pub book_id: String,
    /// 版本。
    pub version: u64,
    /// 客户。
    pub customer_id: String,
    /// 客户名称。
    pub customer_name: String,
    /// 显式销售负责人。
    pub sales_owner_user_id: String,
    /// 业务组织。
    pub business_org_unit_id: String,
    /// 形态。
    pub form: SelectionForm,
    /// 与 `form` 相同，兼容内部列表字段。
    pub selection_form: SelectionForm,
    /// 提交方式。
    pub submit_mode: SubmitMode,
    /// 状态。
    pub status: BookletStatus,
    /// 方案身份。
    pub proposal_id: Option<String>,
    /// 创建时间。
    pub created_at: u64,
}

/// 选品册详情。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesSelectionBookletView {
    /// 固定来源类型内可编辑的原条件。
    pub pool_filter: Option<PoolFilterSnapshot>,
    /// 去重后的原勾选。
    #[serde(default)]
    pub sku_ids: Vec<String>,
    /// 身份。
    pub id: String,
    /// 与 `id` 相同，兼容详情页 `book_id`。
    pub book_id: String,
    /// 版本。
    pub version: u64,
    /// 客户。
    pub customer_id: String,
    /// 客户名称。
    pub customer_name: String,
    /// 显式销售负责人。
    pub sales_owner_user_id: String,
    /// 负责人显示名；候选只回 ID 与显示名，不回全量身份。
    pub sales_owner_name: Option<String>,
    /// 业务组织。
    pub business_org_unit_id: String,
    /// 形态。
    pub form: SelectionForm,
    /// 与 `form` 相同。
    pub selection_form: SelectionForm,
    /// 提交方式。
    pub submit_mode: SubmitMode,
    /// 状态。
    pub status: BookletStatus,
    /// 来源类型。
    pub pool_source_kind: PoolSourceKind,
    /// 与 `pool_source_kind` 相同。
    pub source_kind: PoolSourceKind,
    /// 档位。
    pub tiers: Vec<TierRule>,
    /// 当前批次。
    pub batch_id: Option<String>,
    /// 资格日期。
    pub eligibility_as_of: Option<BusinessDate>,
    /// 准备时间。
    pub prepared_at: Option<Instant>,
    /// 实际陈列数。
    pub display_count: u32,
    /// 已删数。
    pub removed_count: u32,
    /// 缺图数。
    pub missing_image_count: u32,
    /// 失败原因。
    pub last_prepare_failure: Option<String>,
    /// 准备阶段。
    pub prepare_stage: Option<PrepareStage>,
    /// 已完成档位数。
    pub completed_tier_count: Option<u32>,
    /// 逐档报告。
    pub tier_reports: Vec<TierReportView>,
    /// 陈列项。
    pub items: Vec<DisplayItemView>,
    /// 链接到期。
    pub link_expires_at: Option<Instant>,
    /// 是否已撤销。
    pub link_revoked: bool,
    /// 方案身份。
    pub proposal_id: Option<String>,
    /// 复制链接时的公开路径；更换后重试不返回旧令牌。
    pub public_path: Option<String>,
    /// 可发给客户的相对地址，与 `public_path` 相同。
    pub public_url: Option<String>,
    /// 已提交方案编号。
    pub proposal_no: Option<String>,
    /// 最近更新时间。
    pub updated_at: Instant,
}

/// 档位生成报告。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TierReportView {
    /// 档位身份。
    pub tier_id: String,
    /// 期望数。
    pub expected_count: u32,
    /// 实际数。
    pub actual_count: u32,
    /// 停止原因。
    pub stop_reason: SearchStopReason,
    /// 停止说明。
    pub stop_label: String,
    /// 图片失败数。
    pub image_failures: u32,
}

/// 陈列项视图。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DisplayItemView {
    /// 项身份。
    pub id: String,
    /// 与 `id` 相同，兼容预览卡片。
    pub item_id: String,
    /// `SINGLE_SKU` 或 `PACKAGE`。
    pub kind: String,
    /// 是否已删除。
    pub removed: bool,
    /// 档位。
    pub tier_id: Option<String>,
    /// 档位名称。
    pub tier_name: Option<String>,
    /// 名称。
    pub name: String,
    /// 规格。
    pub specification: Vec<crate::entity::sales_selection::SpecificationAttributeSnapshot>,
    /// 规格展示文案。
    pub spec_label: String,
    /// 售价。
    pub price: Amount,
    /// 与 `price` 相同。
    pub price_gross: Amount,
    /// 与目标差额。
    pub target_delta: Option<Amount>,
    /// 封面资产。
    pub cover_asset_id: Option<String>,
    /// 管理端封面引用，由前端转预览。
    pub cover_image: Option<String>,
    /// 单位。
    pub unit: Option<String>,
    /// 成员。
    pub members: Vec<DisplayMemberView>,
    /// 是否缺图。
    pub missing_image: bool,
}

/// 套餐成员。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DisplayMemberView {
    /// SKU 稳定身份；公开页不展示编号，内部预览可用。
    pub sku_id: String,
    /// 名称。
    pub name: String,
    /// 规格。
    pub specification: Vec<crate::entity::sales_selection::SpecificationAttributeSnapshot>,
    /// 单位。
    pub unit: String,
    /// 售价。
    pub price: Amount,
}

/// 方案列表行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesSelectionProposalListItemView {
    /// 身份。
    pub id: String,
    /// 编号。
    pub proposal_no: String,
    /// 客户名称。
    pub customer_name: String,
    /// 选品册。
    pub booklet_id: String,
    /// 继承所属册的显式销售负责人。
    pub sales_owner_user_id: String,
    /// 继承所属册的业务组织。
    pub business_org_unit_id: String,
    /// 形态。
    pub form: SelectionForm,
    /// 提交方式。
    pub submit_mode: SubmitMode,
    /// 提交时间。
    pub submitted_at: Instant,
}

/// 方案详情。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesSelectionProposalView {
    /// 身份。
    pub id: String,
    /// 编号。
    pub proposal_no: String,
    /// 客户。
    pub customer_id: String,
    /// 客户名称。
    pub customer_name: String,
    /// 选品册。
    pub booklet_id: String,
    /// 继承所属册的显式销售负责人。
    pub sales_owner_user_id: String,
    /// 继承所属册的业务组织。
    pub business_org_unit_id: String,
    /// 形态。
    pub form: SelectionForm,
    /// 提交方式。
    pub submit_mode: SubmitMode,
    /// 提交时间。
    pub submitted_at: Instant,
    /// 提交来源。
    pub source: crate::entity::sales_selection::ProposalSource,
    /// 合计。
    pub total_amount: Option<Amount>,
    /// 陈列行。
    pub display_lines: Vec<ProposalDisplayLineView>,
    /// SKU 行。
    pub sku_lines: Vec<ProposalSkuLineView>,
}

/// 方案陈列行视图。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProposalDisplayLineView {
    /// 陈列项。
    pub display_item_id: String,
    /// 档位。
    pub tier_id: Option<String>,
    /// 份数。
    pub quantity: Option<u32>,
    /// 售价。
    pub unit_price: Amount,
    /// 行金额。
    pub line_amount: Option<Amount>,
    /// 封面。
    pub cover_asset_id: Option<String>,
}

/// 方案 SKU 行视图。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProposalSkuLineView {
    /// 来源陈列。
    pub display_item_id: String,
    /// 名称。
    pub name: String,
    /// 规格快照。
    #[serde(default)]
    pub specification: Vec<crate::entity::sales_selection::SpecificationAttributeSnapshot>,
    /// 单位快照。
    #[serde(default)]
    pub unit: String,
    /// 数量。
    pub quantity: Option<u32>,
    /// 单价。
    pub unit_price: Amount,
    /// 行金额。
    pub line_amount: Option<Amount>,
}

/// 公开页状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PublicSelectionPageKind {
    /// 可选择。
    Selecting,
    /// 只读回执。
    Receipt,
    /// 结束。
    Ended,
}

/// 公开选品页。专用字段清单，不序列化内部实体。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublicSelectionPageView {
    /// 页面种类。
    pub kind: PublicSelectionPageKind,
    /// 客户展示名称。
    pub customer_name: Option<String>,
    /// 形态。
    pub form: Option<SelectionForm>,
    /// 提交方式。
    pub submit_mode: Option<SubmitMode>,
    /// 会话版本。
    pub session_version: Option<u64>,
    /// 陈列。
    pub items: Vec<PublicDisplayItemView>,
    /// 当前选择。
    pub choices: Vec<PublicChoiceView>,
    /// 按份采购合计。
    pub total_amount: Option<Amount>,
    /// 回执。
    pub receipt: Option<PublicReceiptView>,
}

/// 公开陈列卡片。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublicDisplayItemView {
    /// 项身份。
    pub item_id: String,
    /// 档位名称。
    pub tier_name: Option<String>,
    /// 名称。
    pub name: String,
    /// 规格。
    pub specification: Vec<crate::entity::sales_selection::SpecificationAttributeSnapshot>,
    /// 售价。
    pub price: Amount,
    /// 封面图路径。
    pub cover_path: Option<String>,
    /// 成员。
    pub members: Vec<PublicDisplayMemberView>,
}

/// 公开成员白名单，不暴露管理端商品身份。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublicDisplayMemberView {
    /// 商品名称。
    pub name: String,
    /// 规格。
    pub specification: Vec<crate::entity::sales_selection::SpecificationAttributeSnapshot>,
    /// 单位。
    pub unit: String,
    /// 快照销售价。
    pub price: Amount,
}

/// 公开已选。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublicChoiceView {
    /// 项身份。
    pub item_id: String,
    /// 份数。
    pub quantity: Option<u32>,
    /// 行金额。
    pub line_amount: Option<Amount>,
}

/// 公开回执。
impl PublicChoiceView {
    /// 以必填项身份构造公开已选；份数与金额默认为空。
    ///
    /// # 参数
    /// * `item_id` - 项身份
    ///
    /// # 返回
    /// 返回空份数的公开已选。
    ///
    /// # 错误
    /// 无。
    pub fn new(item_id: String) -> Self {
        Self { item_id, quantity: None, line_amount: None }
    }
}

/// 公开回执。

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublicReceiptView {
    /// 方案编号。
    pub proposal_no: String,
    /// 提交时间。
    pub submitted_at: Instant,
    /// 客户名称。
    pub customer_name: String,
    /// 明细。
    pub items: Vec<PublicChoiceView>,
    /// 按份采购合计。
    pub total_amount: Option<Amount>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_choice_view_new_defaults_options_to_none() {
        let view = PublicChoiceView::new("item-1".into());
        assert_eq!(view.item_id, "item-1");
        assert!(view.quantity.is_none());
    }

    #[test]
    fn first_prepare_reuses_book_rules_and_create_key() {
        let req = PrepareSalesSelectionRequest::first_prepare("book-1", 1, "idem-1");
        assert_eq!(req.booklet_id, "book-1");
        assert_eq!(req.expected_version, 1);
        assert_eq!(req.idempotency_key, "idem-1");
        assert_eq!(req.kind, PrepareKind::FirstPrepare);
        assert!(req.tier_ids.is_empty());
        assert_eq!(req.seed, 0);
        assert!(req.pool_filter.is_none());
        assert!(req.sku_ids.is_none());
        assert!(req.tiers.is_empty());
    }
}

/// 分页别名。
pub type SalesSelectionBookletPage = PageView<SalesSelectionBookletListItemView>;
/// 方案分页。
pub type SalesSelectionProposalPage = PageView<SalesSelectionProposalListItemView>;

/// 解析可选单值 CSV；空串视为未提供，未知姓名参数已在路由层拒绝。
///
/// # 参数
/// * `deserializer` - serde 反序列化器
///
/// # 返回
/// 返回单值或 `None`。
///
/// # 错误
/// 类型不符时返回反序列化错误。
fn deserialize_optional_csv<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize as _;
    let raw = Option::<String>::deserialize(deserializer)?;
    Ok(raw.map(|value| value.trim().to_string()).filter(|value| !value.is_empty()))
}

/// 解析可选多值 CSV；去空白去重，最多 100 个。
///
/// # 参数
/// * `deserializer` - serde 反序列化器
///
/// # 返回
/// 返回去重后的 ID 或 `None`。
///
/// # 错误
/// 超限或类型不符时返回反序列化错误。
fn deserialize_optional_csv_vec<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize as _;
    let raw = Option::<serde_json::Value>::deserialize(deserializer)?;
    let Some(raw) = raw else {
        return Ok(None);
    };
    let mut ids = match raw {
        serde_json::Value::String(text) => {
            text.split(',').map(str::trim).filter(|item| !item.is_empty()).map(str::to_string).collect()
        },
        serde_json::Value::Array(items) => {
            let mut ids = Vec::new();
            for item in items {
                match item {
                    serde_json::Value::String(text) => {
                        let text = text.trim();
                        if !text.is_empty() {
                            ids.push(text.to_string());
                        }
                    },
                    other => {
                        return Err(serde::de::Error::custom(format!("人员或组织条件非法: {other}")));
                    },
                }
            }
            ids
        },
        serde_json::Value::Null => return Ok(None),
        other => {
            return Err(serde::de::Error::custom(format!("人员或组织条件非法: {other}")));
        },
    };
    ids.sort();
    ids.dedup();
    if ids.len() > 100 {
        return Err(serde::de::Error::custom("人员或组织条件最多 100 个，请缩小条件"));
    }
    Ok((!ids.is_empty()).then_some(ids))
}

/// 复制链接结果。只返回当前有效相对路径，不含完整令牌日志字段。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CopyLinkView {
    /// 可发给客户的相对地址，例如 `/s/{token}`。
    pub public_url: String,
    /// 与 `public_url` 相同。
    pub public_path: String,
}

/// 内部会话只读视图。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesSelectionSessionView {
    /// 选品册。
    pub book_id: String,
    /// 当前会话版本。
    pub expected_version: u64,
    /// 已保存选择。
    pub selections: Vec<PublicChoiceView>,
    /// 按份采购合计。
    pub total_amount: Option<Amount>,
    /// 更新时间。
    pub updated_at: Instant,
}
