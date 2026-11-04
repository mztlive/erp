//! 发起人未读进展规则；启动和本人操作不产生新的提醒。

use serde::Serialize;

use crate::error::{Error, Result};

/// 与实例推进一起写入的提醒；同一实例始终只有一个未读标记。
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct ApprovalProgress {
    initiator_progress_version: u64,
    initiator_unread: bool,
}

impl ApprovalProgress {
    /// 为其他人的已提交操作生成提醒，本人操作保留原有未读状态。
    /// # 参数
    /// `started_by` 为发起人，`actor_id` 为本次操作人，`version` 为持久化后的版本。
    /// # 返回
    /// 他人操作返回新的未读进展，本人操作返回空。
    /// # 错误
    /// 无。
    pub fn changed(started_by: &str, actor_id: &str, version: u64) -> Option<Self> {
        (started_by != actor_id)
            .then_some(Self { initiator_progress_version: version, initiator_unread: true })
    }
}

/// 用户已实际看到的实例版本，不接受未来版本或跨用户确认。
#[derive(Debug, Clone, Copy)]
pub struct ViewedApprovalProgress(u64);

impl ViewedApprovalProgress {
    /// 校验已读确认对应当前账号发起的实例及有效历史版本。
    /// # 参数
    /// `version` 为客户端展示版本；其余参数为数据库中的当前版本、发起人和登录账号。
    /// # 返回
    /// 已校验的查看版本。
    /// # 错误
    /// 非发起人隐藏实例存在性；零、未来或超出 BSON 范围的版本返回校验错误。
    pub fn new(version: u64, current_version: u64, started_by: &str, actor_id: &str) -> Result<Self> {
        if started_by != actor_id {
            return Err(Error::NotFound("审批实例不存在".into()));
        }
        if version == 0 || version > current_version || i64::try_from(version).is_err() {
            return Err(Error::ValidationError("查看版本无效，请刷新审批进度后重试".into()));
        }
        Ok(Self(version))
    }

    /// 取得确认版本，用于原子更新时限制可清除的进展。
    /// # 参数
    /// 无。
    /// # 返回
    /// 已校验的版本。
    /// # 错误
    /// 无。
    pub fn version(self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn own_actions_do_not_create_reminders() {
        assert_eq!(ApprovalProgress::changed("starter", "starter", 2), None);
        assert_eq!(
            ApprovalProgress::changed("starter", "approver", 3),
            Some(ApprovalProgress { initiator_progress_version: 3, initiator_unread: true })
        );
    }

    #[test]
    fn read_confirmation_accepts_stale_but_not_future_or_foreign_versions() {
        assert_eq!(ViewedApprovalProgress::new(3, 5, "u", "u").unwrap().version(), 3);
        assert!(ViewedApprovalProgress::new(5, 5, "u", "u").is_ok());
        for version in [0, 6, u64::MAX] {
            assert!(ViewedApprovalProgress::new(version, 5, "u", "u").is_err());
        }
        assert!(matches!(ViewedApprovalProgress::new(3, 5, "u", "other"), Err(Error::NotFound(_))));
    }
}
