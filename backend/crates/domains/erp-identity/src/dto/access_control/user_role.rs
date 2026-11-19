//! 管理端角色绑定留痕的查询和写入数据。实际角色授权由 Casbin 绑定决定。

use application_core::non_blank;
use erp_core::common::time::Instant;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::access_control::{UserRole, UserRoleData, UserRoleRevokeData};
use crate::entity::rbac::RoleId;
use crate::{Error, Result};

/// W19 用户授权页的角色绑定记录，含分配、有效期和撤销历史，仅供展示。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct UserRoleView {
    /// 绑定记录 ID，供按记录撤销时引用。
    pub id: String,
    /// 绑定记录所属的用户 ID。
    pub user_id: String,
    /// 留痕记录中的角色 ID。
    pub role_id: String,
    /// 记录中的生效时间，以 Unix 秒表示。
    pub effective_from: Instant,
    /// 记录中的到期时间，以 Unix 秒表示；空值表示未设到期时间。
    pub effective_to: Option<Instant>,
    /// 登记分配的账号或系统身份。
    pub assigned_by: String,
    /// 记录被撤销的时间，以 Unix 秒表示；未撤销时为空。
    pub revoked_at: Option<Instant>,
    /// 执行记录撤销的账号或系统身份（若有）。
    pub revoked_by: Option<String>,
    /// 撤销该记录时保存的结构化原因代码（若有）。
    pub revoke_reason_code: Option<String>,
    /// 撤销该记录时保存的补充说明（若有）。
    pub revoke_reason_text: Option<String>,
    /// 绑定记录当前的乐观锁版本。
    pub version: u64,
    /// 绑定记录创建时间，以 Unix 秒表示。
    pub created_at: u64,
}

impl From<UserRole> for UserRoleView {
    // 将留痕实体转为响应，保留分配、有效期和撤销信息。
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

/// 按用户查询角色绑定留痕，包含已撤销记录。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UserRoleListParams {
    /// 必填用户 ID，须含非空白内容，按原值匹配。
    #[validate(custom(function = "non_blank", message = "用户ID不能为空"))]
    pub user_id: String,
}

/// 登记角色分配留痕的请求。服务端填入分配人，Casbin 绑定保持原状。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct AssignUserRoleRequest {
    /// 要登记绑定的用户 ID，须含非空白内容。
    #[validate(custom(function = "non_blank", message = "用户ID不能为空"))]
    pub user_id: String,
    /// 要登记的角色 ID，由 `RoleId` 解析校验。
    pub role_id: RoleId,
    /// 生效时间，以正数 Unix 秒表示；默认当前时刻。
    #[validate(range(min = 1, message = "生效时间必须大于 0"))]
    pub effective_from: Option<u64>,
    /// 可选到期时间，以正数 Unix 秒表示；实体要求晚于生效时间。
    #[validate(range(min = 1, message = "到期时间必须大于 0"))]
    pub effective_to: Option<u64>,
}

impl AssignUserRoleRequest {
    /// 校验请求并解析秒级时间戳，生效时间默认取当前时刻；`assigned_by` 为分配账号或系统身份。
    /// 请求不合法或时间戳超出表示范围时返回校验错误。到期时间与生效时间的顺序由实体校验。
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

/// 撤销绑定留痕的请求，须记录结构化原因；当前版本由服务端在事务内读取。
/// 撤销后仍保留该记录，Casbin 实际绑定保持原状。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct RevokeUserRoleRequest {
    /// 必填的结构化撤销原因代码，须含非空白内容，实体裁剪后限 64 个字符。
    #[validate(custom(function = "non_blank", message = "撤权原因代码不能为空"))]
    pub revoke_reason_code: String,
    /// 可选的撤销原因说明，最多 64 个字符。
    #[validate(length(max = 64, message = "撤权原因过长"))]
    pub revoke_reason_text: Option<String>,
}

impl RevokeUserRoleRequest {
    /// 将原因代码和可选说明转为实体的绑定撤销数据。
    pub fn into_revoke_data(self) -> UserRoleRevokeData {
        let data = UserRoleRevokeData::new(self.revoke_reason_code);
        match self.revoke_reason_text {
            Some(text) => data.with_revoke_reason_text(text),
            None => data,
        }
    }
}

// 登记绑定前将 Unix 秒转为时间点；超出有符号整数或日历的可表示范围时返回校验错误。
fn parse_time(seconds: u64) -> Result<Instant> {
    let seconds =
        i64::try_from(seconds).map_err(|_| Error::ValidationError("时间戳超出可表示范围".into()))?;
    Instant::try_from_unix_secs(seconds).map_err(|error| Error::ValidationError(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RoleId;
    // 验证时间转换拒绝零值和越界值，保留有效秒值，并默认当前时刻生效。
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
