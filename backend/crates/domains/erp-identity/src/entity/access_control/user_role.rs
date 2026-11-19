//! `user_role`：用户与角色的授权绑定（数据模型 §5.1 / W19 §5.1）。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
use erp_core::ids::UserRoleId;
use erp_core::validation::{normalize_optional_text, normalize_required_text};
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use crate::entity::rbac::RoleId;

/// 用户 ID 最大长度。
const USER_ID_MAX_LEN: usize = 128;
/// 分配人标识最大长度。
const ASSIGNED_BY_MAX_LEN: usize = 128;
/// 撤权原因代码最大长度。
const REASON_CODE_MAX_LEN: usize = 64;

/// 创建用户角色绑定时使用的授权对象、有效期和分配来源。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserRoleData {
    /// 接收角色分配的用户账号 ID。
    pub user_id: String,
    /// 本次分配的角色 ID，沿用 `RoleId` 的解析校验。
    pub role_id: RoleId,
    /// 绑定计划开始生效的时刻。
    pub effective_from: Instant,
    /// 绑定计划到期的时刻；缺省表示无预设到期时间，有值时须晚于生效时间。
    pub effective_to: Option<Instant>,
    /// 发起角色分配的账号或系统身份标识。
    pub assigned_by: String,
}

/// 撤销用户角色绑定时提交的审计原因。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserRoleRevokeData {
    /// W19 撤权命令必填的结构化原因代码（`reasonCode`）。
    pub revoke_reason_code: String,
    /// 用于补充撤权背景的可选说明，纯空白视为缺省。
    pub revoke_reason_text: Option<String>,
}

impl UserRoleRevokeData {
    /// 用结构化原因代码组装待校验的撤权数据，默认省略说明。
    pub fn new(revoke_reason_code: impl Into<String>) -> Self {
        Self { revoke_reason_code: revoke_reason_code.into(), revoke_reason_text: None }
    }

    /// 在撤权数据中附上补充说明，交由执行撤权时规范化。
    pub fn with_revoke_reason_text(mut self, revoke_reason_text: impl Into<String>) -> Self {
        self.revoke_reason_text = Some(revoke_reason_text.into());
        self
    }
}

/// 用户角色的分配与撤销留痕（数据模型 §5.1）；运行时角色资格由 Casbin 规则提供。
///
/// 管理端按有效时段区分当前、待生效和已过期的记录供只读展示（W19 §5.1），并保留撤权历史。
/// 事务写入时，部分唯一索引保证同一用户和角色最多只有一条未撤权记录。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct UserRole {
    /// 扁平存储的绑定主键、乐观锁版本与时间戳。
    #[serde(flatten)]
    pub base: BaseModel,
    /// 接受角色分配的用户账号 ID，供查询绑定历史。
    pub user_id: String,
    /// 本条记录关联的已校验角色 ID。
    pub role_id: RoleId,
    /// 绑定的起始时刻，用于区分已生效和待生效的分配。
    pub effective_from: Instant,
    /// 绑定的计划到期时刻；缺省表示无预设到期时间。
    pub effective_to: Option<Instant>,
    /// 角色分配时留存的操作者账号或系统身份标识。
    pub assigned_by: String,
    /// 实际执行撤权的时刻；有值即标记该绑定已撤销。
    pub revoked_at: Option<Instant>,
    /// 执行撤权的身份标识，供历史追溯。
    pub revoked_by: Option<String>,
    /// 执行撤权时保存的结构化原因代码，未撤权时缺省。
    pub revoke_reason_code: Option<String>,
    /// 执行撤权时保存的可选补充说明。
    pub revoke_reason_text: Option<String>,
}

impl UserRole {
    /// 用调用方分配的 `id` 和已校验的 `RoleId` 构造未撤权绑定。
    /// 用户 ID 和分配人裁剪后须非空且各限 128 字符；到期时刻须严格晚于生效时刻，
    /// 输入违反任一约束时返回错误。
    pub fn new(id: UserRoleId, data: UserRoleData) -> Result<Self> {
        let user_id = normalize_required_text(data.user_id, "用户ID不能为空", USER_ID_MAX_LEN, "用户ID过长")?;
        let assigned_by =
            normalize_required_text(data.assigned_by, "分配人不能为空", ASSIGNED_BY_MAX_LEN, "分配人过长")?;
        if let Some(effective_to) = data.effective_to
            && effective_to <= data.effective_from
        {
            return Err(Error::from("到期时间必须晚于生效时间"));
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

    /// 执行 W19 即时撤权，用 `at` 标记绑定已撤销并记录执行人及结构化原因。
    /// 拒绝重复撤权；裁剪后原因代码和执行人须非空，代码与可选说明各限 64 字符、
    /// 执行人限 128 字符。全部校验通过后才更新撤权字段。
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
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;
    use erp_core::ids::UserRoleId;

    use super::{UserRole, UserRoleData, UserRoleRevokeData};
    use crate::entity::rbac::RoleId;

    // 提供带固定有效期和首尾空白标识的销售角色分配样本。
    fn data() -> UserRoleData {
        UserRoleData {
            user_id: " user-1 ".to_string(),
            role_id: RoleId::parse("role-sales").unwrap(),
            effective_from: Instant::from_unix_secs(1_700_000_000),
            effective_to: Some(Instant::from_unix_secs(1_700_604_800)),
            assigned_by: " admin-1 ".to_string(),
        }
    }

    // 确认创建时规范化用户 ID 和分配人，并沿用已解析的角色 ID。
    #[test]
    fn new_trims_text_and_keeps_role_id() {
        let binding = UserRole::new(UserRoleId::new("ur-1"), data()).unwrap();
        assert_eq!(binding.user_id, "user-1");
        assert_eq!(binding.assigned_by, "admin-1");
        assert_eq!(binding.role_id, RoleId::parse("role-sales").unwrap());
    }

    // 确认全空白的用户 ID 无法用于创建角色绑定。
    #[test]
    fn new_rejects_empty_user_id() {
        let payload = UserRoleData { user_id: "  ".to_string(), ..data() };
        assert!(UserRole::new(UserRoleId::new("ur-1"), payload).is_err());
    }

    // 确认角色 ID 解析拒绝非法字符并接受带连字符的 ASCII 标识。
    #[test]
    fn role_id_parse_rejects_invalid_value() {
        assert!(RoleId::parse("非法角色").is_err(), "RoleId 解析拒绝非法字符");
        assert!(RoleId::parse("role-sales").is_ok());
    }

    // 确认到期早于生效的分配无法创建绑定。
    #[test]
    fn new_rejects_reversed_effective_window() {
        let payload = UserRoleData { effective_to: Some(Instant::from_unix_secs(1_699_913_600)), ..data() };
        assert!(UserRole::new(UserRoleId::new("ur-1"), payload).is_err());
    }

    // 确认撤权拒绝空白原因、保存原因代码和执行人，并阻止重复撤权。
    #[test]
    fn revoke_requires_reason_and_deactivates() {
        let mut binding = UserRole::new(UserRoleId::new("ur-1"), data()).unwrap();
        assert!(
            binding
                .revoke(UserRoleRevokeData::new("  "), "admin-2", Instant::from_unix_secs(1_700_100_000),)
                .is_err()
        );

        binding
            .revoke(
                UserRoleRevokeData::new("EMERGENCY_REVOKE").with_revoke_reason_text(" 紧急撤权 "),
                "admin-2",
                Instant::from_unix_secs(1_700_100_000),
            )
            .unwrap();
        assert_eq!(binding.revoke_reason_code.as_deref(), Some("EMERGENCY_REVOKE"));
        assert_eq!(binding.revoked_by.as_deref(), Some("admin-2"));

        assert!(
            binding
                .revoke(
                    UserRoleRevokeData::new("EMERGENCY_REVOKE"),
                    "admin-3",
                    Instant::from_unix_secs(1_700_200_000),
                )
                .is_err(),
            "已撤权绑定不可重复撤权"
        );
    }

    // 确认撤权数据默认只含原因代码，也可显式附上说明。
    #[test]
    fn revoke_data_constructor_sets_code_only() {
        let data = UserRoleRevokeData::new("EMERGENCY_REVOKE");
        assert_eq!(data.revoke_reason_code, "EMERGENCY_REVOKE");
        assert!(data.revoke_reason_text.is_none());
        let noted = UserRoleRevokeData::new("EMERGENCY_REVOKE").with_revoke_reason_text("紧急撤权");
        assert_eq!(noted.revoke_reason_text.as_deref(), Some("紧急撤权"));
    }

    // 确认 JSON 序列化与反序列化保留完整的用户角色绑定。
    #[test]
    fn entity_roundtrips_through_bson() {
        let binding = UserRole::new(UserRoleId::new("ur-1"), data()).unwrap();
        let roundtrip: UserRole = serde_json::from_str(&serde_json::to_string(&binding).unwrap()).unwrap();
        assert_eq!(roundtrip, binding);
    }
}
