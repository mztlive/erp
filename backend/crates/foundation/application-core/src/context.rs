//! 调用人身份数据。

use std::num::NonZeroU32;

use erp_core::validation::normalize_optional_text;
use erp_core::{AccountKind, Error as CoreError, Result as CoreResult};

use crate::AuditContext;

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
    /// 将已认证的账号 ID、登录名和账号类型带入服务，供其记录业务操作人；此处不填充名称快照或请求号。
    pub fn new(actor_id: String, actor_account: String, actor_type: AccountKind) -> Self {
        Self { actor_id, actor_account, actor_type, actor_name_snapshot: None, request_id: None }
    }

    /// 将鉴权时已读取的名称固定为审计快照，不查询账号或补齐历史名称。去掉首尾空白，空白视为缺失；
    /// 超过 128 个字符或裁剪后含控制字符时拒绝。
    pub fn with_actor_name_snapshot(mut self, name: Option<String>) -> CoreResult<Self> {
        let name = normalize_optional_text(name, "操作人名称", 128)?;
        if name.as_ref().is_some_and(|value| value.chars().any(char::is_control)) {
            return Err(CoreError::from("操作人名称包含控制字符"));
        }
        self.actor_name_snapshot = name;
        Ok(self)
    }

    /// 审计展示可取鉴权时捕获的安全名称，不受后来改名影响；未捕获则返回 `None`。
    pub fn actor_name_snapshot(&self) -> Option<&str> {
        self.actor_name_snapshot.as_deref()
    }

    /// 附上 HTTP 入口已确定的追踪号，以关联同次请求的审计；内部调用不生成请求号。裁剪首尾空白，空白视为缺失；
    /// 超过 128 个字符或裁剪后含控制字符时拒绝。
    pub fn with_request_id(mut self, request_id: Option<String>) -> CoreResult<Self> {
        let request_id = normalize_optional_text(request_id, "请求编号", 128)?;
        if request_id.as_ref().is_some_and(|value| value.chars().any(char::is_control)) {
            return Err(CoreError::from("请求编号包含控制字符"));
        }
        self.request_id = request_id;
        Ok(self)
    }

    /// 读取入口捕获的追踪号以关联请求与审计，内部调用或未捕获时返回 `None`。
    pub fn request_id(&self) -> Option<&str> {
        self.request_id.as_deref()
    }

    /// 借用已认证的稳定账号 ID，供业务审计关联操作人而不依赖可变名称。
    pub fn id(&self) -> &str {
        &self.actor_id
    }

    /// 取已认证的账号类型，以区分审计中的操作人类别。
    pub fn kind(&self) -> AccountKind {
        self.actor_type
    }
    /// 借用已认证的登录账号，供审计记录本次鉴权所用的登录名。
    pub fn account(&self) -> &str {
        &self.actor_account
    }

    /// 兼容只消费 `(actor_id, actor_account, actor_type)` 的调用方；
    /// 若须保留名称快照和请求号，应转换为 [`AuditContext`]。
    pub fn into_parts(self) -> (String, String, AccountKind) {
        (self.actor_id, self.actor_account, self.actor_type)
    }
}

impl From<AuditActor> for AuditContext {
    // 用认证时捕获的值建立首条事件的审计上下文；序号为一，缺失字段不补齐，也不重新查询或规范化。
    fn from(actor: AuditActor) -> Self {
        Self {
            actor_id: actor.actor_id,
            actor_account: actor.actor_account,
            actor_type: actor.actor_type,
            actor_name_snapshot: actor.actor_name_snapshot,
            request_id: actor.request_id,
            event_sequence: NonZeroU32::MIN,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 确认显式名称会去除首尾空白，且兼容旧构造与原身份三元组。
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

    // 用多字节名称检验 128 字符上限，并阻止内部控制字符进入快照。
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

    // 确认追踪号需显式传入且受长度和控制字符限制；旧构造不伪造关联，也不改变原身份。
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
