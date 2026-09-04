//! 无任务直接对账的终态结论（领域类型，INT-E23）。
//!
//! wire 枚举与稳定代码仍由服务 DTO 拥有；本类型只表达结论到追加式决定动作与
//! 派生状态的确定性映射，Service 经 `From` 转换后消费，不做二次矩阵维护。

use super::{ResolutionAction, ResultingStatus};

/// 无任务直接对账的终态结论。
///
/// 仅两种正式结论；非终结动作不经本类型，直接使用原动作类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectConclusion {
    /// 确认不存在业务错误。
    ConfirmNoError,
    /// 确认差异有效。
    ConfirmValidDifference,
}

impl DirectConclusion {
    /// 从终态结论派生追加式决定的固定动作。
    ///
    /// # 返回
    /// 确认无误派生 `ConfirmNoError` 动作，确认有效差异派生
    /// `ConfirmValidDifference` 动作；一一对应，无默认分支。
    pub fn resolution_action(self) -> ResolutionAction {
        match self {
            Self::ConfirmNoError => ResolutionAction::ConfirmNoError,
            Self::ConfirmValidDifference => ResolutionAction::ConfirmValidDifference,
        }
    }

    /// 从终态结论派生追加式决定后的固定状态。
    ///
    /// # 返回
    /// 返回派生动作唯一允许的终态，由 [`ResolutionAction::derived_status`] 独占。
    pub fn resulting_status(self) -> ResultingStatus {
        self.resolution_action().derived_status()
    }
}

#[cfg(test)]
mod tests {
    use super::{DirectConclusion, ResolutionAction, ResultingStatus};

    #[test]
    fn both_conclusions_map_to_typed_action_and_terminal_status() {
        assert_eq!(
            DirectConclusion::ConfirmNoError.resolution_action(),
            ResolutionAction::ConfirmNoError
        );
        assert_eq!(
            DirectConclusion::ConfirmNoError.resulting_status(),
            ResultingStatus::ConfirmedNoError
        );
        assert_eq!(
            DirectConclusion::ConfirmValidDifference.resolution_action(),
            ResolutionAction::ConfirmValidDifference
        );
        assert_eq!(
            DirectConclusion::ConfirmValidDifference.resulting_status(),
            ResultingStatus::ConfirmedValidDifference
        );
        assert!(DirectConclusion::ConfirmNoError.resulting_status().is_terminal());
        assert!(DirectConclusion::ConfirmValidDifference
            .resulting_status()
            .is_terminal());
    }
}
