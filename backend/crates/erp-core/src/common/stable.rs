//! `StableBase`：稳定基础资料与可编辑草稿公共字段（P0-1.4 共享基元任务）。
//!
//! 对应数据模型 4.3「稳定基础资料和可编辑草稿」：`status`、`current_revision_id`、
//! `created_by`、`updated_by`。持久化元数据（`id`/`version`/`created_at`/`updated_at`/
//! `deleted_at`）仍由 `entity_core::BaseModel` 承担，两者组合使用；
//! `BaseModel.version` ≡ 数据模型 `lock_version`（见 common/README.md）。

use serde::{Deserialize, Serialize};

/// 稳定基础资料与可编辑草稿的公共字段。
///
/// `Status` 为域内固定的状态枚举，约束为 `Copy + PartialEq`；
/// serde 派生使用默认 bound（额外要求 `Status: Serialize + Deserialize`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StableBase<Status>
where
    Status: Copy + PartialEq,
{
    /// 当前业务状态或启停状态。
    pub status: Status,
    /// 当前生效修订 ID；没有版本对象时为 `None`。
    pub current_revision_id: Option<String>,
    /// 创建人（账号或系统身份）。
    pub created_by: String,
    /// 最后更新人。
    pub updated_by: String,
}

impl<Status: Copy + PartialEq> StableBase<Status> {
    /// 创建稳定对象公共字段。
    ///
    /// # 参数
    /// * `status` - 初始业务状态
    /// * `created_by` - 创建人（账号或系统身份）
    ///
    /// # 返回
    /// 返回公共字段实例：`updated_by` 与 `created_by` 相同，
    /// `current_revision_id` 为 `None`。
    pub fn new(status: Status, created_by: impl Into<String>) -> Self {
        let created_by = created_by.into();
        Self {
            status,
            current_revision_id: None,
            updated_by: created_by.clone(),
            created_by,
        }
    }

    /// 记录一次更新。
    ///
    /// # 参数
    /// * `updated_by` - 本次更新的执行人
    ///
    /// # 返回
    /// 无返回值；仅更新 `updated_by` 字段。
    pub fn touch(&mut self, updated_by: impl Into<String>) {
        self.updated_by = updated_by.into();
    }

    /// 读取当前业务状态。
    ///
    /// # 返回
    /// 返回当前状态（`Copy`，不产生克隆语义）。
    pub fn status(&self) -> Status {
        self.status
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    enum DemoStatus {
        Active,
        Disabled,
    }

    #[test]
    fn new_touch_and_status() {
        let mut base = StableBase::new(DemoStatus::Active, "alice");
        assert_eq!(base.status(), DemoStatus::Active);
        assert_eq!(base.created_by, "alice");
        assert_eq!(base.updated_by, "alice");
        assert!(base.current_revision_id.is_none());

        base.touch("bob");
        assert_eq!(base.updated_by, "bob");
        assert_eq!(base.created_by, "alice", "touch 不修改创建人");
    }

    #[test]
    fn serde_roundtrip_with_derived_bounds() {
        let mut base = StableBase::new(DemoStatus::Active, "alice");
        base.current_revision_id = Some("rev-1".to_string());

        let json = serde_json::to_string(&base).unwrap();
        let back: StableBase<DemoStatus> = serde_json::from_str(&json).unwrap();
        assert_eq!(back.status, DemoStatus::Active);
        assert_eq!(back.current_revision_id.as_deref(), Some("rev-1"));
        assert_eq!(back.updated_by, "alice");
    }
}
