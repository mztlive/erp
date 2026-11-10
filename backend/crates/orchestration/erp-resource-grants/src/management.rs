//! 管理请求只接受来源身份和固定维护范围，不接受客户端业务分配证据。
use erp_core::validation::normalize_required_text_ref;
use erp_identity::entity::resource_access::{GrantTarget, SourceKey, SourceUpdate};
use erp_identity::service::resource_access::{GrantRecompute, ResourceAccessService};
use erp_identity::{Error, Result};
pub use erp_workflow::repository::grant_history::GrantHistoryKind;
use serde::{Deserialize, Serialize};

use crate::ResourceGrantProcess;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManualGrantRequest {
    pub command_id: String,
    pub recipient: String,
    pub targets: Vec<GrantTarget>,
    pub reason: String,
}
impl ManualGrantRequest {
    /// 固定人工规则和版本，将稳定操作号转换为来源身份。
    /// # 参数
    /// 无；消费已解码请求。
    /// # 返回
    /// 首次人工授权的完整目标；同操作号异载荷由来源实体拒绝。
    /// # 错误
    /// 非法身份、空原因或目标由领域发布入口拒绝。
    pub fn into_update(self) -> SourceUpdate {
        SourceUpdate {
            key: SourceKey {
                rule: "manual".into(),
                business_type: "manual".into(),
                business_id: self.command_id,
                recipient: self.recipient,
            },
            rule_version: 1,
            fact_version: 1,
            targets: self.targets,
            reason: self.reason,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevokeRequest {
    pub source: SourceKey,
    pub expected_version: u64,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourcesQuery {
    pub recipient: String,
    pub after: Option<String>,
}
impl SourcesQuery {
    /// 校验来源查询的账号和游标。
    /// # 参数
    /// 无。
    /// # 返回
    /// 参数满足有界读取合同。
    /// # 错误
    /// 空白账号或过长游标时拒绝。
    pub fn validate(&self) -> Result<()> {
        if normalize_required_text_ref(&self.recipient, "接收账号不能为空", 128, "接收账号过长")?
            != self.recipient
        {
            return Err(Error::ValidationError("账号不能带首尾空白".into()));
        }
        validate_cursor(self.after.as_deref())
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecomputeRequest {
    pub selection: GrantRecompute,
    pub after: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackfillRequest {
    pub kind: GrantHistoryKind,
    pub after: Option<String>,
    #[serde(default = "default_dry_run")]
    pub dry_run: bool,
}
fn default_dry_run() -> bool {
    true
}

#[derive(Debug, Default, Serialize)]
pub struct BackfillReport {
    pub scanned: usize,
    pub eligible: usize,
    pub appended: usize,
    pub existing: usize,
    pub terminated: usize,
    pub invalid: Vec<BackfillProblem>,
    pub next_cursor: Option<String>,
    pub dry_run: bool,
}
#[derive(Debug, Serialize)]
pub struct BackfillProblem {
    pub record_id: String,
    pub reason: String,
}

/// 校验维护查询的排他游标，禁止无效页默默变成全量重扫。
/// # 参数
/// after 为上一页返回的游标。
/// # 返回
/// None 或合法游标时成功。
/// # 错误
/// 空白或超过 4096 字符时拒绝。
pub fn validate_cursor(after: Option<&str>) -> Result<()> {
    if let Some(value) = after
        && normalize_required_text_ref(value, "游标不能为空", 4096, "游标过长")? != value
    {
        return Err(Error::ValidationError("游标不能带首尾空白".into()));
    }
    Ok(())
}

impl ResourceGrantProcess {
    /// 取得绑定真实目标校验的人工授权服务。
    /// # 参数
    /// 无。
    /// # 返回
    /// grant/revoke/sources 均自行校验管理员权限的服务。
    /// # 错误
    /// 构造不执行 I/O。
    pub fn management(&self) -> ResourceAccessService {
        self.access(None)
    }
}

#[cfg(test)]
mod tests {
    use erp_identity::entity::resource_access::GrantSource;
    use serde_json::json;

    use super::*;
    #[test]
    fn management_does_not_accept_automatic_evidence_or_arbitrary_versions() {
        let request = json!({"command_id":"c", "recipient":"u", "targets":[{"resource":"sales_order","id":"S001"}], "reason":"核查"});
        let update = serde_json::from_value::<ManualGrantRequest>(request.clone()).unwrap().into_update();
        let mut source = GrantSource::new(update.key.clone()).unwrap();
        assert!(
            source
                .apply(
                    update.rule_version,
                    update.fact_version,
                    update.targets.clone(),
                    "admin",
                    &update.reason
                )
                .unwrap()
        );
        assert!(!source.apply(1, 1, update.targets.clone(), "admin", &update.reason).unwrap());
        let mut changed = update.targets.clone();
        changed[0].id = "S002".into();
        assert!(source.apply(1, 1, changed, "admin", &update.reason).is_err());
        let mut forged = request;
        forged["fact_version"] = json!(9);
        assert!(serde_json::from_value::<ManualGrantRequest>(forged).is_err());
        let backfill: BackfillRequest =
            serde_json::from_value(json!({"kind":"invoice_reassignments"})).unwrap();
        assert!(backfill.dry_run);
        for value in ["", " ", " a"] {
            assert!(validate_cursor(Some(value)).is_err());
        }
        validate_cursor(Some("cursor")).unwrap();
    }
}
