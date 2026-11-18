//! 选品册状态及允许的动作。

use erp_core::common::state::DocumentState;
use serde::{Deserialize, Serialize};

/// 选品册业务状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BookletStatus {
    /// 已保存创建字段，尚无完整可预览结果。
    Draft,
    /// 后端正在冻结快照或生成套餐及主图。
    Preparing,
    /// 已有完整可预览陈列项。
    PendingPublish,
    /// 在链接有效期内且尚未提交。
    Published,
    /// 已形成唯一销售方案。
    Submitted,
    /// 未提交即被主动关闭或到期。
    Closed,
    /// 发布前终止。
    Voided,
}

impl BookletStatus {
    /// 将选品册状态转成用于持久化和查询的稳定代码，与中文展示文案分开。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::Preparing => "PREPARING",
            Self::PendingPublish => "PENDING_PUBLISH",
            Self::Published => "PUBLISHED",
            Self::Submitted => "SUBMITTED",
            Self::Closed => "CLOSED",
            Self::Voided => "VOIDED",
        }
    }

    /// 为界面或响应展示提供选品册状态的中文名称；机器筛选和存储不使用该名称。
    pub fn label(self) -> &'static str {
        match self {
            Self::Draft => "草稿",
            Self::Preparing => "准备中",
            Self::PendingPublish => "待发布",
            Self::Published => "已发布",
            Self::Submitted => "已提交",
            Self::Closed => "已关闭",
            Self::Voided => "已作废",
        }
    }

    /// 判断是否为业务终态。
    /// 结果：已提交、已关闭、已作废返回 `true`。
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Submitted | Self::Closed | Self::Voided)
    }

    /// 判断是否允许编辑草稿规则或商品池来源内容。
    /// 结果：仅草稿允许改筛选、勾选或档位规则。
    pub fn allows_draft_edit(self) -> bool {
        matches!(self, Self::Draft)
    }

    /// 判断是否允许启动准备任务。
    /// 结果：草稿或待发布返回 `true`。准备中禁止再次启动。
    pub fn allows_prepare(self) -> bool {
        matches!(self, Self::Draft | Self::PendingPublish)
    }

    /// 判断是否允许删除陈列项。
    /// 结果：仅待发布允许删除陈列。
    pub fn allows_delete_display(self) -> bool {
        matches!(self, Self::PendingPublish)
    }

    /// 判断是否允许发布。
    /// 结果：仅待发布允许发布。
    pub fn allows_publish(self) -> bool {
        matches!(self, Self::PendingPublish)
    }

    /// 判断是否允许作废。
    /// 结果：草稿或待发布允许作废。
    pub fn allows_void(self) -> bool {
        matches!(self, Self::Draft | Self::PendingPublish)
    }

    /// 判断是否允许客户改会话或提交。
    /// 结果：仅已发布允许写入会话。
    pub fn allows_session_write(self) -> bool {
        matches!(self, Self::Published)
    }

    /// 判断公开页是否只展示结束态。
    /// 结果：已关闭、已作废、已撤销或已到期时返回 `true`。已提交且链接仍有效时不是结束态。
    ///
    /// 输入约定：
    /// - `link_revoked`：链接访问是否已撤销
    /// - `expired`：服务端是否已到期
    pub fn public_is_ended(self, link_revoked: bool, expired: bool) -> bool {
        if link_revoked || expired {
            return true;
        }
        matches!(self, Self::Closed | Self::Voided)
    }

    /// 判断公开页是否只读回执。
    /// 结果：已提交且链接未撤销、未到期时返回 `true`。
    ///
    /// 输入约定：
    /// - `link_revoked`：链接访问是否已撤销
    /// - `expired`：服务端是否已到期
    pub fn public_is_receipt(self, link_revoked: bool, expired: bool) -> bool {
        matches!(self, Self::Submitted) && !link_revoked && !expired
    }
}

impl DocumentState for BookletStatus {
    // 校验选品册状态迁移时查本状态可直接到达的后继状态；无后继即不能再迁移。
    // 结果：`Draft` 的后继为 `Preparing`、`Voided`；`Preparing` 的后继为 `PendingPublish`、`Draft`；
    // `PendingPublish` 的后继为 `Preparing`、`Published`、`Voided`；`Published` 的后继为 `Submitted`、`Closed`。
    // `Submitted`、`Closed`、`Voided` 返回空切片。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Draft => &[Self::Preparing, Self::Voided],
            Self::Preparing => &[Self::PendingPublish, Self::Draft],
            Self::PendingPublish => &[Self::Preparing, Self::Published, Self::Voided],
            Self::Published => &[Self::Submitted, Self::Closed],
            Self::Submitted | Self::Closed | Self::Voided => &[],
        }
    }
}

#[cfg(test)]
mod tests {
    use erp_core::common::state::ensure_transition;

    use super::BookletStatus;

    #[test]
    fn published_cannot_return_to_pending() {
        assert!(ensure_transition(BookletStatus::Published, BookletStatus::PendingPublish).is_err());
    }

    #[test]
    fn submitted_is_terminal_even_when_link_revoked() {
        assert!(BookletStatus::Submitted.is_terminal());
        assert!(BookletStatus::Submitted.public_is_ended(true, false));
        assert!(!BookletStatus::Submitted.public_is_receipt(true, false));
        assert!(BookletStatus::Submitted.public_is_receipt(false, false));
    }
}
