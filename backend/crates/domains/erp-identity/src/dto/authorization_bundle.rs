//! 授权文件预览、应用和按对象导出的协议。
use application_core::CommandFingerprint;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::entity::authorization_bundle::{
    PolicyBinding, PolicyDocument, PolicyMode, PolicyRole, PolicyVersion,
};
use crate::{Error, Result, RoleId};

/// 确认审核后应用授权文件；服务端重新生成差异，不接受客户端指定差异。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplyPolicyRequest {
    /// 已审核的授权文件；应用前会重新规范化并核验。
    pub document: PolicyDocument,
    /// 审核时的策略版本；首次提交发现版本已变时须重新预览。
    pub expected_policy_version: u64,
    /// 预览返回的审核摘要，用于确认操作者、目标事实及待提交的变更未变。
    pub review_hash: CommandFingerprint,
    /// 本次操作号；去首尾空白后须为 1 至 128 字节，重试沿用原值和请求。
    pub idempotency_key: String,
}

/// 导出时须明确选择角色或人员，不以空选择代表全公司。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportPolicyRequest {
    /// 待导出的角色 ID，最多 100 项。
    pub role_ids: Vec<String>,
    /// 待导出的人员账号 ID，最多 100 项。
    pub user_ids: Vec<String>,
}

/// 供预览审核的单项配置差异；前后值不包含密码或会话。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolicyChange {
    /// 变更类型：角色 `role`、绑定 `binding` 或人员范围 `data_scope`。
    pub kind: String,
    /// 变更目标：角色 ID、账号 ID，或由账号、资源和动作组成的范围标识。
    pub target: String,
    /// 供比较的原配置；角色或范围尚不存在时为 `null`。
    pub before: Value,
    /// 预计写入后的配置，结构由变更类型决定。
    pub after: Value,
    /// 受此变更影响的账号 ID，用于审核影响范围。
    pub affected_user_ids: Vec<String>,
}

/// 审核页面展示的服务端计划，绑定操作者、授权版本和目标事实。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyPreview {
    /// 规范化后的待审核文件；确认时应与摘要一同提交。
    pub document: PolicyDocument,
    /// 生成本次审核计划时的策略版本。
    pub policy_version: u64,
    /// 绑定本次文件、操作者、授权及目标事实的摘要，应用时必须匹配。
    pub review_hash: CommandFingerprint,
    /// 服务端计算的实际配置差异；没有变更时为空。
    pub changes: Vec<PolicyChange>,
    /// 供审核参考的系统政策说明，不作为待写配置。
    pub policy_notes: Vec<String>,
}

/// 授权文件提交回执；幂等重放沿用原结果，不再次写入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyApplyResult {
    /// 定位提交回执和相关审计的稳定命令 ID。
    pub command_id: String,
    /// 本次提交完成时的策略版本；无变更时不增加。
    pub policy_version: u64,
    /// 实际应用的配置差异项数；重放返回原统计。
    pub change_count: usize,
    /// 是否通过同一命令的既有回执返回结果。
    pub replayed: bool,
}

/// 导出所选对象的显式配置；系统规则单列，不编成可执行授权。
#[derive(Debug, Clone, Serialize)]
pub struct PolicyExport {
    /// 同一快照导出的配置文件，可重新提交预览。
    pub document: PolicyDocument,
    /// 导出读取所用的授权策略版本。
    pub policy_version: u64,
    /// 仅供说明的系统政策，不作为文件中的可编辑授权。
    pub policy_notes: Vec<String>,
}

impl ExportPolicyRequest {
    /// 导出读库前，把角色和人员选择转成规范化的只读取证条件；拒绝空选择、超限、重复或非法标识。
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
