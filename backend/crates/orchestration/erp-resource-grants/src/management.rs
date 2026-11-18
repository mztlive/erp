//! 管理请求只接受来源身份和固定维护范围，不接受客户端业务分配证据。
use erp_core::validation::normalize_required_text_ref;
use erp_identity::entity::resource_access::{GrantTarget, SourceKey, SourceUpdate};
use erp_identity::service::resource_access::{GrantRecompute, ResourceGrantService};
use erp_identity::{Error, Result};
use serde::Deserialize;

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
    /// 将操作号和接收人转换为人工授权来源，并固定规则及事实版本，供领域发布入口做幂等判断。
    /// 同来源重复提交不同载荷时冲突；非法身份、原因或目标由发布入口校验。
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
    /// 校验来源列表查询的接收账号和分页游标，拒绝空白、超长或带首尾空白的账号，以及非法游标。
    pub fn validate(&self) -> Result<()> {
        if normalize_required_text_ref(&self.recipient, "接收账号不能为空", 128, "接收账号过长")?
            != self.recipient
        {
            return Err(Error::ValidationError("请去掉账号开头和结尾的空格".into()));
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

impl RecomputeRequest {
    /// 进入授权重算前校验排他游标，允许未传游标；空串、有首尾空白或超过 4096 字符的游标拒绝执行。
    pub fn validate(&self) -> Result<()> {
        validate_cursor(self.after.as_deref())
    }
}

// 检查排他游标非空、无首尾空白且不超过 4096 字符，避免将无效续页请求当作全量重扫。
pub(crate) fn validate_cursor(after: Option<&str>) -> Result<()> {
    if let Some(value) = after
        && normalize_required_text_ref(
            value,
            "缺少分页位置，请回到第一页重新查询",
            4096,
            "分页位置无效，请回到第一页重新查询",
        )? != value
    {
        return Err(Error::ValidationError("分页位置无效，请回到第一页重新查询".into()));
    }
    Ok(())
}

impl ResourceGrantProcess {
    /// 组装绑定业务目标校验的授权管理服务，供管理员授予、撤销、查询来源或发起重算。构造不执行 I/O，各管理操作自行校验管理员权限。
    pub fn management(&self) -> ResourceGrantService {
        self.access(None)
    }
}

#[cfg(test)]
mod tests {
    use erp_identity::entity::resource_access::GrantSource;
    use serde_json::json;

    use super::*;
    #[test]
    fn recompute_validates_cursor_without_changing_selection() {
        for after in [None, Some("cursor".into()), Some("x".repeat(4096))] {
            let request = RecomputeRequest { selection: GrantRecompute::All, after };
            request.validate().unwrap();
            assert!(matches!(request.selection, GrantRecompute::All));
        }

        for after in ["".into(), " ".into(), " a".into(), "a ".into(), "x".repeat(4097)] {
            let request = RecomputeRequest { selection: GrantRecompute::All, after: Some(after) };
            assert!(request.validate().is_err());
        }
    }
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

        for value in ["", " ", " a"] {
            assert!(validate_cursor(Some(value)).is_err());
        }
        validate_cursor(Some("cursor")).unwrap();
    }
}
