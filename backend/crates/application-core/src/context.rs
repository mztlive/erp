//! 调用人身份数据。

use erp_core::validation::normalize_optional_text;
use erp_core::{AccountKind, Error as CoreError, Result as CoreResult};

/// 已通过 HTTP 鉴权的审计操作人。
///
/// 该类型只携带操作人身份；审计动作、资源类型和目标由具体 Service 决定，
/// 避免协议层伪造或遗漏业务审计语义。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditActor {
    actor_id: String,
    actor_account: String,
    actor_type: AccountKind,
    actor_name_snapshot: Option<String>,
    request_id: Option<String>,
}

impl AuditActor {
    /// 创建审计操作人。
    ///
    /// # 参数
    /// * `actor_id` - 操作人账号 ID
    /// * `actor_account` - 操作人登录账号
    /// * `actor_type` - 操作人账号类型
    ///
    /// # 返回值
    /// 返回只包含鉴权身份的审计操作人。
    pub fn new(actor_id: String, actor_account: String, actor_type: AccountKind) -> Self {
        Self { actor_id, actor_account, actor_type, actor_name_snapshot: None, request_id: None }
    }

    /// 捕获本次鉴权已经读取的操作人名称，不查询账号或补齐历史名称。
    ///
    /// # 参数
    /// * `name` - 已鉴权会话名称；缺失明确传 `None`。
    /// # 返回
    /// 返回冻结安全名称的操作人。
    /// # 错误
    /// 名称超过128字符或包含控制字符时返回错误。
    pub fn with_actor_name_snapshot(mut self, name: Option<String>) -> CoreResult<Self> {
        let name = normalize_optional_text(name, "操作人名称", 128)?;
        if name.as_ref().is_some_and(|value| value.chars().any(char::is_control)) {
            return Err(CoreError::from("操作人名称包含控制字符"));
        }
        self.actor_name_snapshot = name;
        Ok(self)
    }

    /// 返回本次鉴权捕获的安全名称快照。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 已捕获名称返回引用；旧调用或未知名称返回 `None`。
    /// # 错误
    /// 无。
    pub fn actor_name_snapshot(&self) -> Option<&str> {
        self.actor_name_snapshot.as_deref()
    }

    /// 捕获当前 HTTP 请求已经确定的追踪号；内部调用不生成请求号。
    ///
    /// # 参数
    /// * `request_id` - 入口已验证的请求关联；非请求入口保持 `None`。
    /// # 返回
    /// 返回冻结当前请求关联的操作人。
    /// # 错误
    /// 请求号超过128字符或包含控制字符时返回错误。
    pub fn with_request_id(mut self, request_id: Option<String>) -> CoreResult<Self> {
        let request_id = normalize_optional_text(request_id, "请求编号", 128)?;
        if request_id.as_ref().is_some_and(|value| value.chars().any(char::is_control)) {
            return Err(CoreError::from("请求编号包含控制字符"));
        }
        self.request_id = request_id;
        Ok(self)
    }

    /// 返回入口捕获的当前请求追踪号。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 当前请求关联返回引用；内部或旧调用返回 `None`。
    /// # 错误
    /// 无。
    pub fn request_id(&self) -> Option<&str> {
        self.request_id.as_deref()
    }

    /// 返回操作人账号 ID。
    ///
    /// # 返回值
    /// 返回已认证身份中的账号 ID。
    pub fn id(&self) -> &str {
        &self.actor_id
    }

    /// 返回操作人账号类型。
    ///
    /// # 返回值
    /// 返回已认证身份中的后台账号类型。
    pub fn kind(&self) -> AccountKind {
        self.actor_type
    }
    /// 返回操作人登录账号。
    ///
    /// # 返回值
    /// 返回已认证身份中的登录账号。
    pub fn account(&self) -> &str {
        &self.actor_account
    }

    /// 消费操作人并返回身份字段。
    ///
    /// # 返回值
    /// 返回 `(actor_id, actor_account, actor_type)`。
    pub fn into_parts(self) -> (String, String, AccountKind) {
        (self.actor_id, self.actor_account, self.actor_type)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 旧构造保持名称缺失；显式名称规范化且不改变原身份三元组。
    #[test]
    fn actor_name_snapshot_keeps_original_identity_contract() {
        let actor = AuditActor::new("actor-1".into(), "sales".into(), AccountKind::Admin);
        assert_eq!(actor.actor_name_snapshot(), None);
        let actor = actor.with_actor_name_snapshot(Some("  周晓彤  ".into())).unwrap();
        assert_eq!(actor.actor_name_snapshot(), Some("周晓彤"));
        assert_eq!(actor.into_parts(), ("actor-1".into(), "sales".into(), AccountKind::Admin));
        let blank = AuditActor::new("actor-1".into(), "sales".into(), AccountKind::Admin)
            .with_actor_name_snapshot(Some("   ".into()))
            .unwrap();
        assert_eq!(blank.actor_name_snapshot(), None);
    }

    /// 名称上限与业务事件一致，以字符数计；内部控制字符不能成为快照。
    #[test]
    fn actor_name_snapshot_accepts_128_characters_and_rejects_unsafe_text() {
        let name = "名".repeat(128);
        let actor = AuditActor::new("actor-1".into(), "sales".into(), AccountKind::Admin)
            .with_actor_name_snapshot(Some(name.clone()))
            .unwrap();
        assert_eq!(actor.actor_name_snapshot(), Some(name.as_str()));
        for invalid in ["名".repeat(129), "周\n晓彤".into(), "周\u{0}晓彤".into()] {
            let result = AuditActor::new("actor-1".into(), "sales".into(), AccountKind::Admin)
                .with_actor_name_snapshot(Some(invalid));
            assert!(result.is_err());
        }
    }

    /// 请求关联由显式入口提供，旧构造不会伪造请求号，身份三元组保持原值。
    #[test]
    fn request_id_is_optional_bounded_and_independent_of_identity() {
        let actor = AuditActor::new("actor-1".into(), "sales".into(), AccountKind::Admin);
        assert_eq!(actor.request_id(), None);
        let actor = actor.with_request_id(Some(" request-1 ".into())).unwrap();
        assert_eq!(actor.request_id(), Some("request-1"));
        assert_eq!(actor.into_parts(), ("actor-1".into(), "sales".into(), AccountKind::Admin));
        for invalid in ["r".repeat(129), "request\nforged".into()] {
            let result = AuditActor::new("actor-1".into(), "sales".into(), AccountKind::Admin)
                .with_request_id(Some(invalid));
            assert!(result.is_err());
        }
    }
}
