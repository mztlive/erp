//! 记录读取授权管理请求、分页响应与输入校验。
use erp_core::validation::normalize_required_text_ref;
use serde::Deserialize;

use crate::entity::resource_access::{GrantSource, GrantTarget, SourceKey, SourceUpdate};
use crate::service::resource_access::GrantRecompute;
use crate::{Error, Result};

/// 管理员按接收账号查看记录读取授权来源时返回的分页视图。
#[derive(Debug, Clone, serde::Serialize)]
pub struct GrantSourcePage {
    /// 本页最多 100 条授权来源，包含已终止记录。
    pub items: Vec<GrantSource>,
    /// 下一页使用的排他主键游标；没有后续页时为空。
    pub next_cursor: Option<String>,
}

/// 创建或重放人工读取授权的管理请求。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManualGrantRequest {
    /// 用于识别人工授权重放的稳定操作号。
    pub command_id: String,
    /// 接收记录读取授权的后台账号 ID。
    pub recipient: String,
    /// 要授予读取权限的完整单据列表。
    pub targets: Vec<GrantTarget>,
    /// 管理员记录的授权或撤销原因。
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

/// 按期望版本永久终止来源的管理请求。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevokeRequest {
    /// 要永久终止的稳定来源键。
    pub source: SourceKey,
    /// 撤销前调用方读取的来源版本。
    pub expected_version: u64,
    /// 管理员记录的授权或撤销原因。
    pub reason: String,
}

/// 按接收账号分页列出读取授权来源的查询条件。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourcesQuery {
    /// 接收记录读取授权的后台账号 ID。
    pub recipient: String,
    /// 上一页末尾的排他游标；首屏查询留空。
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
        validate_grant_cursor(self.after.as_deref())
    }
}

/// 重排选定范围内正式授权事件的管理请求。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecomputeRequest {
    /// 只允许按固定规则、资源或精确来源等预设范围重算。
    pub selection: GrantRecompute,
    /// 上一页末尾的排他游标；首次重算留空。
    pub after: Option<String>,
}

impl RecomputeRequest {
    /// 进入授权重算前校验排他游标，允许未传游标；空串、有首尾空白或超过 4096 字符的游标拒绝执行。
    pub fn validate(&self) -> Result<()> {
        validate_grant_cursor(self.after.as_deref())
    }
}

/// 检查排他游标非空、无首尾空白且不超过 4096 字符，避免将无效续页请求当作全量重扫。
pub fn validate_grant_cursor(after: Option<&str>) -> Result<()> {
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

#[cfg(test)]
mod tests {
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
    // 管理请求与查询均拒绝协议外字段。
    #[test]
    fn management_requests_reject_unknown_fields() {
        let mut revoke = json!({"source":{"rule":"manual", "business_type":"manual",
            "business_id":"c", "recipient":"u"}, "expected_version":1, "reason":"核查"});
        let mut sources = json!({"recipient":"u", "after":"cursor"});
        let mut recompute = json!({"selection":{"kind":"all"}});
        assert!(serde_json::from_value::<RevokeRequest>(revoke.clone()).is_ok());
        serde_json::from_value::<SourcesQuery>(sources.clone()).unwrap().validate().unwrap();
        serde_json::from_value::<RecomputeRequest>(recompute.clone()).unwrap().validate().unwrap();

        revoke["fact_version"] = json!(9);
        sources["unexpected"] = json!(true);
        recompute["unexpected"] = json!(true);
        assert!(serde_json::from_value::<RevokeRequest>(revoke).is_err());
        assert!(serde_json::from_value::<SourcesQuery>(sources).is_err());
        assert!(serde_json::from_value::<RecomputeRequest>(recompute).is_err());
    }

    // 列表查询和重算对同一个无效游标给出相同提示。
    #[test]
    fn sources_and_recompute_keep_cursor_error_messages() {
        for (after, message, trimmed) in [
            ("", "缺少分页位置，请回到第一页重新查询", false),
            (" cursor", "分页位置无效，请回到第一页重新查询", true),
            (&"x".repeat(4097), "分页位置无效，请回到第一页重新查询", false),
        ] {
            let sources = SourcesQuery { recipient: "u".into(), after: Some(after.into()) };
            let recompute = RecomputeRequest { selection: GrantRecompute::All, after: Some(after.into()) };
            for result in [sources.validate(), recompute.validate()] {
                match (result.unwrap_err(), trimmed) {
                    (Error::Logic(erp_core::Error::LogicError(actual)), false)
                    | (Error::ValidationError(actual), true) => assert_eq!(actual, message),
                    (error, _) => panic!("游标错误分类不符: {error:?}"),
                }
            }
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
            assert!(validate_grant_cursor(Some(value)).is_err());
        }
        validate_grant_cursor(Some("cursor")).unwrap();
    }
}
