//! 选品册形态、提交方式与商品池来源等创建后不可改的取值。

use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

/// 选品形态：创建时选定，之后不可改。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SelectionForm {
    /// 单品：每个 SKU 一个陈列项。
    SingleSku,
    /// 套餐：按档位规则生成组合。
    Package,
}

impl SelectionForm {
    /// 返回稳定代码。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回持久化使用的代码。
    ///
    /// # 错误
    /// 无。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SingleSku => "SINGLE_SKU",
            Self::Package => "PACKAGE",
        }
    }

    /// 返回面向销售的名称。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回「单品」或「套餐」。
    ///
    /// # 错误
    /// 无。
    pub fn label(self) -> &'static str {
        match self {
            Self::SingleSku => "单品",
            Self::Package => "套餐",
        }
    }

    /// 判断是否为套餐形态。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 套餐形态返回 `true`。
    ///
    /// # 错误
    /// 无。
    pub fn is_package(self) -> bool {
        matches!(self, Self::Package)
    }
}

/// 提交方式：创建时选定，之后不可改。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SubmitMode {
    /// 按份采购：客户填写每个已选项的份数。
    ByQuantity,
    /// 商城兑换：客户只勾选范围，不填份数。
    MallRedeem,
}

impl SubmitMode {
    /// 返回稳定代码。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回持久化使用的代码。
    ///
    /// # 错误
    /// 无。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ByQuantity => "BY_QUANTITY",
            Self::MallRedeem => "MALL_REDEEM",
        }
    }

    /// 返回面向销售的名称。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回「按份采购」或「商城兑换」。
    ///
    /// # 错误
    /// 无。
    pub fn label(self) -> &'static str {
        match self {
            Self::ByQuantity => "按份采购",
            Self::MallRedeem => "商城兑换",
        }
    }

    /// 判断是否要求份数。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 按份采购返回 `true`。
    ///
    /// # 错误
    /// 无。
    pub fn requires_quantity(self) -> bool {
        matches!(self, Self::ByQuantity)
    }
}

/// 商品池来源类型：创建时写死。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PoolSourceKind {
    /// 当前筛选：按与列表相同的筛选取出 SKU。
    Filter,
    /// 当前勾选：按去重后的稳定 SKU 身份取出。
    Selection,
}

impl PoolSourceKind {
    /// 返回稳定代码。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回持久化使用的代码。
    ///
    /// # 错误
    /// 无。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Filter => "FILTER",
            Self::Selection => "SELECTION",
        }
    }
}

/// 准备任务种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PrepareKind {
    /// 草稿首次准备。
    FirstPrepare,
    /// 待发布按档重生成套餐。
    RegeneratedTiers,
    /// 待发布整册重生成套餐。
    RegeneratedAll,
    /// 待发布整册重新准备。
    RePrepare,
}

impl PrepareKind {
    /// 判断是否沿用当前批次商品池。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 按档或整册重生成返回 `true`；首次准备与整册重新准备返回 `false`。
    ///
    /// # 错误
    /// 无。
    pub fn reuses_current_batch(self) -> bool {
        matches!(self, Self::RegeneratedTiers | Self::RegeneratedAll)
    }

    /// 判断失败时是否应回到草稿。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 仅首次准备失败回草稿。
    ///
    /// # 错误
    /// 无。
    pub fn restores_draft_on_failure(self) -> bool {
        matches!(self, Self::FirstPrepare)
    }
}

/// 准备任务执行阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PrepareStage {
    /// 排队。
    Queued,
    /// 冻结快照。
    Snapshot,
    /// 组合搜索。
    Search,
    /// 图片处理。
    Images,
    /// 写入结果。
    Write,
}

impl PrepareStage {
    /// 返回面向销售的阶段名称。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回阶段文案。
    ///
    /// # 错误
    /// 无。
    pub fn label(self) -> &'static str {
        match self {
            Self::Queued => "排队",
            Self::Snapshot => "冻结商品池",
            Self::Search => "组合搜索",
            Self::Images => "图片处理",
            Self::Write => "写入结果",
        }
    }
}

/// 准备任务运行状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PrepareTaskStatus {
    /// 已创建，等待领取。
    Queued,
    /// 正在执行。
    Running,
    /// 已成功写入。
    Succeeded,
    /// 已失败并完成恢复。
    Failed,
}

impl PrepareTaskStatus {
    /// 判断任务是否仍占用册上的活动任务槽。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 排队或运行中返回 `true`。
    ///
    /// # 错误
    /// 无。
    pub fn is_active(self) -> bool {
        matches!(self, Self::Queued | Self::Running)
    }
}

/// 搜索停止原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SearchStopReason {
    /// 已达到期望数量。
    ReachedExpected,
    /// 展开状态数达到上限。
    BudgetStates,
    /// 搜索时间达到上限。
    BudgetTime,
    /// 已完整搜索有限空间。
    ExhaustedSpace,
}

impl SearchStopReason {
    /// 返回面向销售的停止说明。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回说明文案。不得把预算耗尽说成无解。
    ///
    /// # 错误
    /// 无。
    pub fn label(self) -> &'static str {
        match self {
            Self::ReachedExpected => "已达到期望数量",
            Self::BudgetStates | Self::BudgetTime => "本次搜索达到上限",
            Self::ExhaustedSpace => "无合法组合",
        }
    }

    /// 判断是否允许宣称无合法组合。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 仅完整搜索或可证明无解时返回 `true`。
    ///
    /// # 错误
    /// 无。
    pub fn allows_no_solution_claim(self) -> bool {
        matches!(self, Self::ExhaustedSpace)
    }
}

/// 方案提交来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProposalSource {
    /// 公开选品链接。
    PublicLink,
}

impl ProposalSource {
    /// 返回稳定代码。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回持久化代码。
    ///
    /// # 错误
    /// 无。
    pub fn as_str(self) -> &'static str {
        "PUBLIC_LINK"
    }
}

/// 校验幂等键。
///
/// # 参数
/// * `key` - 原始幂等键
///
/// # 返回
/// 返回去空白后的非空键。
///
/// # 错误
/// 为空或超长时返回领域错误。
pub fn normalize_idempotency_key(key: &str) -> Result<String> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Err(Error::from("请求幂等键不能为空"));
    }
    if trimmed.len() > super::limits::IDEMPOTENCY_KEY_MAX_LEN {
        return Err(Error::from("请求幂等键过长"));
    }
    Ok(trimmed.to_string())
}
