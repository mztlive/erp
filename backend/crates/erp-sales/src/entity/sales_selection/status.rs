//! 选品册状态及允许的动作。

use serde::{Deserialize, Serialize};

use erp_core::common::state::DocumentState;

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

    /// 返回面向销售的名称。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回中文状态名。
    ///
    /// # 错误
    /// 无。
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
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 已提交、已关闭、已作废返回 `true`。
    ///
    /// # 错误
    /// 无。
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Submitted | Self::Closed | Self::Voided)
    }

    /// 判断是否允许编辑草稿规则或商品池来源内容。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 仅草稿允许改筛选、勾选或档位规则。
    ///
    /// # 错误
    /// 无。
    pub fn allows_draft_edit(self) -> bool {
        matches!(self, Self::Draft)
    }

    /// 判断是否允许启动准备任务。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 草稿或待发布返回 `true`。准备中禁止再次启动。
    ///
    /// # 错误
    /// 无。
    pub fn allows_prepare(self) -> bool {
        matches!(self, Self::Draft | Self::PendingPublish)
    }

    /// 判断是否允许删除陈列项。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 仅待发布允许删除陈列。
    ///
    /// # 错误
    /// 无。
    pub fn allows_delete_display(self) -> bool {
        matches!(self, Self::PendingPublish)
    }

    /// 判断是否允许发布。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 仅待发布允许发布。
    ///
    /// # 错误
    /// 无。
    pub fn allows_publish(self) -> bool {
        matches!(self, Self::PendingPublish)
    }

    /// 判断是否允许作废。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 草稿或待发布允许作废。
    ///
    /// # 错误
    /// 无。
    pub fn allows_void(self) -> bool {
        matches!(self, Self::Draft | Self::PendingPublish)
    }

    /// 判断是否允许客户改会话或提交。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 仅已发布允许写入会话。
    ///
    /// # 错误
    /// 无。
    pub fn allows_session_write(self) -> bool {
        matches!(self, Self::Published)
    }

    /// 判断公开页是否只展示结束态。
    ///
    /// # 参数
    /// * `link_revoked` - 链接访问是否已撤销
    /// * `expired` - 服务端是否已到期
    ///
    /// # 返回
    /// 已关闭、已作废、已撤销或已到期时返回 `true`。已提交且链接仍有效时不是结束态。
    ///
    /// # 错误
    /// 无。
    pub fn public_is_ended(self, link_revoked: bool, expired: bool) -> bool {
        if link_revoked || expired {
            return true;
        }
        matches!(self, Self::Closed | Self::Voided)
    }

    /// 判断公开页是否只读回执。
    ///
    /// # 参数
    /// * `link_revoked` - 链接访问是否已撤销
    /// * `expired` - 服务端是否已到期
    ///
    /// # 返回
    /// 已提交且链接未撤销、未到期时返回 `true`。
    ///
    /// # 错误
    /// 无。
    pub fn public_is_receipt(self, link_revoked: bool, expired: bool) -> bool {
        matches!(self, Self::Submitted) && !link_revoked && !expired
    }
}

impl DocumentState for BookletStatus {
    /// 返回合法后继。准备失败恢复走 `Draft` 或 `PendingPublish`。
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
    use super::BookletStatus;
    use erp_core::common::state::ensure_transition;

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
