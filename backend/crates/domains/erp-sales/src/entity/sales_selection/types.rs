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
    /// 将选品陈列形态转成用于持久化和查询的稳定代码，与中文展示文案分开。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SingleSku => "SINGLE_SKU",
            Self::Package => "PACKAGE",
        }
    }

    /// 为界面或响应展示提供选品陈列形态的中文名称；机器筛选和存储不使用该名称。
    pub fn label(self) -> &'static str {
        match self {
            Self::SingleSku => "单品",
            Self::Package => "套餐",
        }
    }

    /// 判断是否为套餐形态。
    /// 结果：套餐形态返回 `true`。
    pub fn is_package(self) -> bool {
        matches!(self, Self::Package)
    }
}

/// 提交方式：当前仅支持按份采购。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SubmitMode {
    /// 按份采购：客户填写每个已选项的份数。
    #[default]
    ByQuantity,
}

impl SubmitMode {
    /// 将选品提交方式转成用于持久化和查询的稳定代码，与中文展示文案分开。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ByQuantity => "BY_QUANTITY",
        }
    }

    /// 为界面或响应展示提供选品提交方式的中文名称；机器筛选和存储不使用该名称。
    pub fn label(self) -> &'static str {
        match self {
            Self::ByQuantity => "按份采购",
        }
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
    /// 将选品商品池来源转成用于持久化和查询的稳定代码，与中文展示文案分开。
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
    /// 结果：按档或整册重生成返回 `true`；首次准备与整册重新准备返回 `false`。
    pub fn reuses_current_batch(self) -> bool {
        matches!(self, Self::RegeneratedTiers | Self::RegeneratedAll)
    }

    /// 判断失败时是否应回到草稿。
    /// 结果：仅首次准备失败回草稿。
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
    /// 返回阶段文案。
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
    /// 结果：排队或运行中返回 `true`。
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
    /// 返回说明文案。不得把预算耗尽说成无解。
    pub fn label(self) -> &'static str {
        match self {
            Self::ReachedExpected => "已达到期望数量",
            Self::BudgetStates | Self::BudgetTime => "本次搜索达到上限",
            Self::ExhaustedSpace => "无合法组合",
        }
    }

    /// 判断是否允许宣称无合法组合。
    /// 结果：仅完整搜索或可证明无解时返回 `true`。
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
    /// 将选品方案来源转成用于持久化和查询的稳定代码，与中文展示文案分开。
    pub fn as_str(self) -> &'static str {
        "PUBLIC_LINK"
    }
}

/// 校验幂等键。
/// 返回去空白后的非空键。
/// 失败条件：为空或超长时返回领域错误。
///
/// 输入约定：
/// - `key`：原始幂等键
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

/// 校验原始访问密码，不改变用户输入的空白。
/// 结果：8 到 64 个字符时通过。
/// 长度不足或超长时拒绝。
///
/// 输入约定：
/// - `password`：原始密码
pub fn validate_access_password(password: &str) -> Result<()> {
    if !(8..=64).contains(&password.chars().count()) {
        return Err(Error::from("访问密码必须是 8 到 64 个字符"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn by_quantity_is_the_only_supported_submit_mode() {
        assert_eq!(SubmitMode::default(), SubmitMode::ByQuantity);
        assert_eq!(SubmitMode::ByQuantity.as_str(), "BY_QUANTITY");
        assert_eq!(SubmitMode::ByQuantity.label(), "按份采购");
        assert!(serde_json::from_str::<SubmitMode>(r#""BY_QUANTITY""#).is_ok());
        assert!(serde_json::from_str::<SubmitMode>(r#""MALL_REDEEM""#).is_err());
        assert!(serde_json::from_str::<SubmitMode>(r#""PICKUP_VOUCHER""#).is_err());
    }

    #[test]
    fn passwords_are_checked_without_normalization() {
        assert!(validate_access_password("12345678").is_ok());
        assert!(validate_access_password(&"中".repeat(64)).is_ok());
        assert!(validate_access_password("1234567").is_err());
        assert!(validate_access_password(&"a".repeat(65)).is_err());
    }
}
