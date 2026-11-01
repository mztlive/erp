//! 授权文件预览、应用和按对象导出的协议。
use application_core::CommandFingerprint;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::entity::authorization_bundle::{
    PolicyBinding, PolicyDocument, PolicyMode, PolicyRole, PolicyVersion,
};
use crate::{Error, Result, RoleId};

/// 服务端重新生成并校验预览后应用；客户端不能指定差异。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplyPolicyRequest {
    pub document: PolicyDocument,
    pub expected_policy_version: u64,
    pub review_hash: CommandFingerprint,
    pub idempotency_key: String,
}

/// 有界、显式的导出选择。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportPolicyRequest {
    pub role_ids: Vec<String>,
    pub user_ids: Vec<String>,
}

/// 逐项真实配置差异；值不包含密码或会话。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolicyChange {
    pub kind: String,
    pub target: String,
    pub before: Value,
    pub after: Value,
    pub affected_user_ids: Vec<String>,
}

/// 已绑定操作人、授权版本及目标事实的可审核计划。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyPreview {
    pub document: PolicyDocument,
    pub policy_version: u64,
    pub review_hash: CommandFingerprint,
    pub changes: Vec<PolicyChange>,
    pub policy_notes: Vec<String>,
}

/// 已提交命令结果；重放返回原版本及变更统计。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyApplyResult {
    pub command_id: String,
    pub policy_version: u64,
    pub change_count: usize,
    pub replayed: bool,
}

/// 可执行配置与非配置政策分开提供，避免把系统规则保存成附加授权。
#[derive(Debug, Clone, Serialize)]
pub struct PolicyExport {
    pub document: PolicyDocument,
    pub policy_version: u64,
    pub policy_notes: Vec<String>,
}

impl ExportPolicyRequest {
    /// 生成只读选择，限制批次且不隐式选择全公司。
    /// # 参数
    /// self 为调用方明确提供的角色及人员集合。
    /// # 返回
    /// 仅用于读取事实的规范化选择文件。
    /// # 错误
    /// 空选择、超限、重复或非法标识时拒绝。
    pub(crate) fn selection(self) -> Result<PolicyDocument> {
        if self.role_ids.len() > 100
            || self.user_ids.len() > 100
            || (self.role_ids.is_empty() && self.user_ids.is_empty())
        {
            return Err(Error::ValidationError("导出须选择1至100个角色或人员".into()));
        }
        let roles = self
            .role_ids
            .into_iter()
            .map(|id| {
                Ok(PolicyRole {
                    id: RoleId::parse(id)?,
                    name: "导出选择".into(),
                    mode: PolicyMode::Replace,
                    permissions: vec![],
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let bindings = self
            .user_ids
            .into_iter()
            .map(|user_id| PolicyBinding { user_id, mode: PolicyMode::Replace, role_ids: vec![] })
            .collect();
        PolicyDocument { version: PolicyVersion::V1, roles, bindings, data_scopes: vec![] }.normalized()
    }
}
