//! `user_role`：用户与角色的授权绑定（数据模型 §5.1 / W19 §5.1）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::UserRoleId;
use crate::rbac::RoleId;
use crate::validation::{normalize_optional_text, normalize_required_text};

/// 用户 ID 最大长度。
const USER_ID_MAX_LEN: usize = 128;
/// 分配人标识最大长度。
const ASSIGNED_BY_MAX_LEN: usize = 128;
/// 撤权原因代码最大长度。
const REASON_CODE_MAX_LEN: usize = 64;

/// 用户角色绑定创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserRoleData {
    /// 用户 ID。
    pub user_id: String,
    /// 角色（`entities::rbac::RoleId`，带解析校验）。
    pub role_id: RoleId,
    /// 生效时间。
    pub effective_from: Instant,
    /// 到期时间；必须晚于 `effective_from`。
    pub effective_to: Option<Instant>,
    /// 分配人（账号或系统身份）。
    pub assigned_by: String,
}

/// 撤权数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserRoleRevokeData {
    /// 撤权原因代码（结构化，必填；W19 变更命令必须携带 reasonCode）。
    pub revoke_reason_code: String,
    /// 撤权原因说明。
    pub revoke_reason_text: Option<String>,
}

/// 用户角色绑定实体（数据模型 §5.1）。
///
/// 已有记录按当前、未来、已过期分开只读展示（W19 §5.1）；角色 ID 使用既有
/// `entities::rbac::RoleId` 解析校验，不重定义。同一用户同一角色同时仅一条
/// 有效绑定（跨行不变量，P3 事务 + P2 唯一索引）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct UserRole {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 用户 ID。
    pub user_id: String,
    /// 角色。
    pub role_id: RoleId,
    /// 生效时间。
    pub effective_from: Instant,
    /// 到期时间。
    pub effective_to: Option<Instant>,
    /// 分配人。
    pub assigned_by: String,
    /// 撤权时间。
    pub revoked_at: Option<Instant>,
    /// 撤权执行人。
    pub revoked_by: Option<String>,
    /// 撤权原因代码。
    pub revoke_reason_code: Option<String>,
    /// 撤权原因说明。
    pub revoke_reason_text: Option<String>,
}

impl UserRole {
    /// 创建用户角色绑定。
    ///
    /// 完成 user_id/assigned_by 的校验与规范化（trim、非空、长度上限），
    /// `role_id` 复用 `RoleId::parse` 校验；`effective_to` 必须晚于
    /// `effective_from`。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::UserRoleId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的绑定（未撤权）。
    ///
    /// # 错误
    /// 当用户 ID/分配人为空或超长、角色 ID 非法或有效期倒挂时返回错误。
    pub fn new(id: UserRoleId, data: UserRoleData) -> Result<Self> {
        let user_id = normalize_required_text(data.user_id, "用户ID不能为空", USER_ID_MAX_LEN, "用户ID过长")?;
        let assigned_by = normalize_required_text(
            data.assigned_by,
            "分配人不能为空",
            ASSIGNED_BY_MAX_LEN,
            "分配人过长",
        )?;
        if let Some(effective_to) = data.effective_to {
            if effective_to <= data.effective_from {
                return Err(Error::from("到期时间必须晚于生效时间"));
            }
        }
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            user_id,
            role_id: data.role_id,
            effective_from: data.effective_from,
            effective_to: data.effective_to,
            assigned_by,
            revoked_at: None,
            revoked_by: None,
            revoke_reason_code: None,
            revoke_reason_text: None,
        })
    }

    /// 撤权。
    ///
    /// 撤权是安全动作（W19：立即紧急撤权），必须记录结构化原因；已撤权绑定
    /// 不可重复撤权。
    ///
    /// # 参数
    /// * `data` - 撤权数据
    /// * `revoked_by` - 撤权执行人
    /// * `at` - 撤权时刻
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 当绑定已撤权、原因代码为空/超长或执行人非法时返回错误。
    pub fn revoke(
        &mut self,
        data: UserRoleRevokeData,
        revoked_by: impl Into<String>,
        at: Instant,
    ) -> Result<()> {
        if self.revoked_at.is_some() {
            return Err(Error::from("用户角色绑定已撤权"));
        }
        let revoke_reason_code = normalize_required_text(
            data.revoke_reason_code,
            "撤权原因代码不能为空",
            REASON_CODE_MAX_LEN,
            "撤权原因代码过长",
        )?;
        let revoke_reason_text =
            normalize_optional_text(data.revoke_reason_text, "撤权原因", REASON_CODE_MAX_LEN)?;
        let revoked_by = normalize_required_text(
            revoked_by.into(),
            "撤权执行人不能为空",
            ASSIGNED_BY_MAX_LEN,
            "撤权执行人过长",
        )?;
        self.revoked_at = Some(at);
        self.revoked_by = Some(revoked_by);
        self.revoke_reason_code = Some(revoke_reason_code);
        self.revoke_reason_text = revoke_reason_text;
        Ok(())
    }

    /// 判断绑定在给定时刻是否有效。
    ///
    /// # 参数
    /// * `at` - 判断时刻
    ///
    /// # 返回
    /// 未撤权、已到生效时间且未到期时返回 `true`。
    pub fn is_effective_at(&self, at: Instant) -> bool {
        if self.revoked_at.is_some() {
            return false;
        }
        if at < self.effective_from {
            return false;
        }
        match self.effective_to {
            Some(effective_to) => at <= effective_to,
            None => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{UserRole, UserRoleData, UserRoleRevokeData};
    use crate::common::time::Instant;
    use crate::ids::UserRoleId;
    use crate::rbac::RoleId;

    fn data() -> UserRoleData {
        UserRoleData {
            user_id: " user-1 ".to_string(),
            role_id: RoleId::parse("role-sales").unwrap(),
            effective_from: Instant::from_unix_secs(1_700_000_000),
            effective_to: Some(Instant::from_unix_secs(1_700_604_800)),
            assigned_by: " admin-1 ".to_string(),
        }
    }

    /// happy path：用户 ID/分配人 trim，角色 ID 保留既有解析。
    #[test]
    fn new_trims_text_and_keeps_role_id() {
        let binding = UserRole::new(UserRoleId::new("ur-1"), data()).unwrap();
        assert_eq!(binding.user_id, "user-1");
        assert_eq!(binding.assigned_by, "admin-1");
        assert_eq!(binding.role_id, RoleId::parse("role-sales").unwrap());
        assert!(binding.is_effective_at(Instant::from_unix_secs(1_700_100_000)));
        assert!(!binding.is_effective_at(Instant::from_unix_secs(1_700_000_000 - 1)));
        assert!(!binding.is_effective_at(Instant::from_unix_secs(1_700_604_801)));
    }

    /// 失败路径：必填为空被拒。
    #[test]
    fn new_rejects_empty_user_id() {
        let payload = UserRoleData {
            user_id: "  ".to_string(),
            ..data()
        };
        assert!(UserRole::new(UserRoleId::new("ur-1"), payload).is_err());
    }

    /// 失败路径：角色 ID 经既有 RoleId 解析校验（非法字符被拒）。
    #[test]
    fn role_id_parse_rejects_invalid_value() {
        assert!(RoleId::parse("非法角色").is_err(), "RoleId 解析拒绝非法字符");
        assert!(RoleId::parse("role-sales").is_ok());
    }

    /// 失败路径：关联不一致（有效期倒挂）被拒。
    #[test]
    fn new_rejects_reversed_effective_window() {
        let payload = UserRoleData {
            effective_to: Some(Instant::from_unix_secs(1_699_913_600)),
            ..data()
        };
        assert!(UserRole::new(UserRoleId::new("ur-1"), payload).is_err());
    }

    /// 撤权：必须记录原因，撤权后失效，不可重复撤权。
    #[test]
    fn revoke_requires_reason_and_deactivates() {
        let mut binding = UserRole::new(UserRoleId::new("ur-1"), data()).unwrap();
        assert!(binding
            .revoke(
                UserRoleRevokeData {
                    revoke_reason_code: "  ".to_string(),
                    revoke_reason_text: None,
                },
                "admin-2",
                Instant::from_unix_secs(1_700_100_000),
            )
            .is_err());

        binding
            .revoke(
                UserRoleRevokeData {
                    revoke_reason_code: "EMERGENCY_REVOKE".to_string(),
                    revoke_reason_text: Some(" 紧急撤权 ".to_string()),
                },
                "admin-2",
                Instant::from_unix_secs(1_700_100_000),
            )
            .unwrap();
        assert_eq!(binding.revoke_reason_code.as_deref(), Some("EMERGENCY_REVOKE"));
        assert_eq!(binding.revoked_by.as_deref(), Some("admin-2"));
        assert!(!binding.is_effective_at(Instant::from_unix_secs(1_700_200_000)));

        assert!(
            binding
                .revoke(
                    UserRoleRevokeData {
                        revoke_reason_code: "EMERGENCY_REVOKE".to_string(),
                        revoke_reason_text: None,
                    },
                    "admin-3",
                    Instant::from_unix_secs(1_700_200_000),
                )
                .is_err(),
            "已撤权绑定不可重复撤权"
        );
    }

    /// BSON 往返。
    #[test]
    fn entity_roundtrips_through_bson() {
        let binding = UserRole::new(UserRoleId::new("ur-1"), data()).unwrap();
        let roundtrip: UserRole =
            bson::deserialize_from_document(bson::serialize_to_document(&binding).unwrap()).unwrap();
        assert_eq!(roundtrip, binding);
    }
}
