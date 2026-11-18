//! 域 D06 `access_control` 的 用户角色绑定 DTO。

use application_core::non_blank;
use erp_core::common::time::Instant;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::access_control::{UserRole, UserRoleData, UserRoleRevokeData};
use crate::entity::rbac::RoleId;
use crate::{Error, Result};

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
    pub effective_from: Instant,
    /// 到期时间（秒级时间戳）。
    pub effective_to: Option<Instant>,
    /// 分配人。
    pub assigned_by: String,
    /// 撤权时间（秒级时间戳）。
    pub revoked_at: Option<Instant>,
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
    // 将用户角色绑定映射为管理响应，保留授予、有效期和撤权事实。
    fn from(binding: UserRole) -> Self {
        Self {
            id: binding.base.id,
            user_id: binding.user_id,
            role_id: binding.role_id.to_string(),
            effective_from: binding.effective_from,
            effective_to: binding.effective_to,
            assigned_by: binding.assigned_by,
            revoked_at: binding.revoked_at,
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
    /// 未提供生效时间时使用当前时刻。
    ///
    /// 输入无效或时间戳超出可表示范围时返回校验错误。
    ///
    /// `assigned_by`：分配人（账号或系统身份）。
    pub fn into_data(self, assigned_by: &str) -> Result<UserRoleData> {
        self.validate()?;
        Ok(UserRoleData {
            user_id: self.user_id,
            role_id: self.role_id,
            effective_from: self.effective_from.map(parse_time).transpose()?.unwrap_or_else(Instant::now),
            effective_to: self.effective_to.map(parse_time).transpose()?,
            assigned_by: assigned_by.to_string(),
        })
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
    /// 返回实体层撤权数据；没有原因说明时不写入说明。
    pub fn into_revoke_data(self) -> UserRoleRevokeData {
        let data = UserRoleRevokeData::new(self.revoke_reason_code);
        match self.revoke_reason_text {
            Some(text) => data.with_revoke_reason_text(text),
            None => data,
        }
    }
}

// 角色绑定请求进入实体前，将时间戳解析为有效时间点；符号溢出或超出日历表示范围时拒绝。
fn parse_time(seconds: u64) -> Result<Instant> {
    let seconds =
        i64::try_from(seconds).map_err(|_| Error::ValidationError("时间戳超出可表示范围".into()))?;
    Instant::try_from_unix_secs(seconds).map_err(|error| Error::ValidationError(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RoleId;
    #[test]
    fn role_assignment_rejects_invalid_times_and_preserves_seconds() {
        let request = |from, to| AssignUserRoleRequest {
            user_id: "user".into(),
            role_id: RoleId::parse("role").unwrap(),
            effective_from: from,
            effective_to: to,
        };
        for seconds in [0, i64::MAX as u64, u64::MAX] {
            assert!(request(Some(seconds), None).into_data("admin").is_err());
            assert!(request(None, Some(seconds)).into_data("admin").is_err());
        }
        let data = request(Some(10), Some(20)).into_data("admin").unwrap();
        assert_eq!(data.effective_from.unix_secs(), 10);
        assert_eq!(data.effective_to.unwrap().unix_secs(), 20);
        assert!(request(None, None).into_data("admin").unwrap().effective_from.unix_secs() > 0);
    }
}
