//! 域 D06 `access_control` 的 用户角色绑定 DTO。

use application_core::non_blank;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::access_control::{UserRole, UserRoleData, UserRoleRevokeData};
use crate::entity::rbac::RoleId;

/// 用户角色绑定响应视图（W19 用户授权；含撤权历史字段，只读展示）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct UserRoleView {
    /// 实体主键。
    pub id: String,
    /// 用户 ID。
    pub user_id: String,
    /// 角色。
    pub role_id: String,
    /// 生效时间（秒级时间戳）。
    pub effective_from: u64,
    /// 到期时间（秒级时间戳）。
    pub effective_to: Option<u64>,
    /// 分配人。
    pub assigned_by: String,
    /// 撤权时间（秒级时间戳）。
    pub revoked_at: Option<u64>,
    /// 撤权执行人。
    pub revoked_by: Option<String>,
    /// 撤权原因代码。
    pub revoke_reason_code: Option<String>,
    /// 撤权原因说明。
    pub revoke_reason_text: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<UserRole> for UserRoleView {
    /// 从实体构造响应视图。
    fn from(binding: UserRole) -> Self {
        Self {
            id: binding.base.id,
            user_id: binding.user_id,
            role_id: binding.role_id.to_string(),
            effective_from: binding.effective_from.unix_secs() as u64,
            effective_to: binding.effective_to.map(|instant| instant.unix_secs() as u64),
            assigned_by: binding.assigned_by,
            revoked_at: binding.revoked_at.map(|instant| instant.unix_secs() as u64),
            revoked_by: binding.revoked_by,
            revoke_reason_code: binding.revoke_reason_code,
            revoke_reason_text: binding.revoke_reason_text,
            version: binding.base.version,
            created_at: binding.base.created_at,
        }
    }
}

/// 用户角色绑定列表查询参数（按用户展示，`user_id` 必填）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UserRoleListParams {
    /// 用户 ID。
    #[validate(custom(function = "non_blank", message = "用户ID不能为空"))]
    pub user_id: String,
}

/// 分配用户角色请求（`effective_from` 缺省为当前时刻；分配人由服务端注入）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct AssignUserRoleRequest {
    /// 用户 ID。
    #[validate(custom(function = "non_blank", message = "用户ID不能为空"))]
    pub user_id: String,
    /// 角色（`RoleId` 解析校验）。
    pub role_id: RoleId,
    /// 生效时间（秒级时间戳）；缺省为当前时刻。
    #[validate(range(min = 1, message = "生效时间必须大于 0"))]
    pub effective_from: Option<u64>,
    /// 到期时间（秒级时间戳）；必须晚于生效时间。
    #[validate(range(min = 1, message = "到期时间必须大于 0"))]
    pub effective_to: Option<u64>,
}

impl AssignUserRoleRequest {
    /// 转换为实体创建数据。
    ///
    /// # 参数
    /// * `assigned_by` - 分配人（账号或系统身份）
    ///
    /// # 返回
    /// 返回实体层创建数据。
    pub fn into_data(self, assigned_by: &str) -> UserRoleData {
        UserRoleData {
            user_id: self.user_id,
            role_id: self.role_id,
            effective_from: erp_core::common::time::Instant::from_unix_secs(
                self.effective_from
                    .map(|secs| secs as i64)
                    .unwrap_or_else(|| erp_core::common::time::Instant::now().unix_secs()),
            ),
            effective_to: self
                .effective_to
                .map(|secs| erp_core::common::time::Instant::from_unix_secs(secs as i64)),
            assigned_by: assigned_by.to_string(),
        }
    }
}

/// 撤权命令（当前绑定版本由后端在事务内读取；撤权必须记录结构化原因）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct RevokeUserRoleRequest {
    /// 撤权原因代码（结构化，必填）。
    #[validate(custom(function = "non_blank", message = "撤权原因代码不能为空"))]
    pub revoke_reason_code: String,
    /// 撤权原因说明。
    #[validate(length(max = 64, message = "撤权原因过长"))]
    pub revoke_reason_text: Option<String>,
}

impl RevokeUserRoleRequest {
    /// 转换为实体撤权数据。
    ///
    /// # 返回
    /// 返回实体层撤权数据。
    pub fn into_revoke_data(self) -> UserRoleRevokeData {
        let data = UserRoleRevokeData::new(self.revoke_reason_code);
        match self.revoke_reason_text {
            Some(text) => data.with_revoke_reason_text(text),
            None => data,
        }
    }
}
