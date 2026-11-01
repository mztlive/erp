//! 预制审计在领域端口之间传递的操作人及请求快照。

use std::num::NonZeroU32;

use erp_core::AccountKind;

/// 与具体事件、持久化元数据及领域无关的审计上下文。
///
/// 本值只搬运已捕获的字段，不生成身份、请求号或时间戳。预制快照的安全字段
/// 仍须由持久化适配器重新校验，不能因为字段已进入本值而跳过校验。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditContext {
    /// 事件发生时的操作人账号 ID。
    pub actor_id: String,
    /// 事件发生时的登录账号。
    pub actor_account: String,
    /// 操作人账号类型。
    pub actor_type: AccountKind,
    /// 原始安全名称快照；历史缺失保持缺失。
    pub actor_name_snapshot: Option<String>,
    /// 原请求关联；内部命令保持缺失。
    pub request_id: Option<String>,
    /// 同一命令内从一开始的非零事件序号。
    pub event_sequence: NonZeroU32,
}

/// 为保留平铺字段的领域预制审计实现统一的上下文复制与构建方法。
///
/// # 参数
/// * `$audit` - 已声明六个 [`AuditContext`] 同名字段的领域快照类型。
///
/// # 返回
/// 生成上下文复制方法及名称、请求号、事件序号三个构建方法。
///
/// # 错误
/// 不返回错误；方法只复制原值，安全校验继续由领域持久化适配器执行。
#[macro_export]
macro_rules! impl_audit_context {
    ($audit:ty) => {
        impl $audit {
            /// 复制原事件的操作人、请求关联与命令内顺序。
            ///
            /// # 参数
            /// 无。
            ///
            /// # 返回
            /// 返回原字段的独立快照，不读取当前账号或生成替代关联。
            ///
            /// # 错误
            /// 不返回错误；本方法不替代持久化时的安全字段校验。
            pub fn audit_context(&self) -> $crate::AuditContext {
                $crate::AuditContext {
                    actor_id: self.actor_id.clone(),
                    actor_account: self.actor_account.clone(),
                    actor_type: self.actor_type,
                    actor_name_snapshot: self.actor_name_snapshot.clone(),
                    request_id: self.request_id.clone(),
                    event_sequence: self.event_sequence,
                }
            }

            /// 携带事件发生时已经捕获的操作人名称。
            ///
            /// # 参数
            /// * `name` - 原事件名称快照；未知保持 `None`。
            ///
            /// # 返回
            /// 返回保留其他字段的预制审计。
            ///
            /// # 错误
            /// 不返回错误；持久化适配器重新校验名称。
            pub fn with_actor_name_snapshot(mut self, name: Option<String>) -> Self {
                self.actor_name_snapshot = name;
                self
            }

            /// 携带原请求关联，不生成新的请求号。
            ///
            /// # 参数
            /// * `request_id` - 原事件请求号；内部或历史事件保持 `None`。
            ///
            /// # 返回
            /// 返回保留其他字段的预制审计。
            ///
            /// # 错误
            /// 不返回错误；持久化适配器重新校验请求号。
            pub fn with_request_id(mut self, request_id: Option<String>) -> Self {
                self.request_id = request_id;
                self
            }

            /// 保留业务命令已经确定的非零事件顺序。
            ///
            /// # 参数
            /// * `event_sequence` - 命令内从一开始的原事件序号。
            ///
            /// # 返回
            /// 返回保留其他字段的预制审计。
            ///
            /// # 错误
            /// 不返回错误；参数类型保证序号非零。
            pub fn with_event_sequence(mut self, event_sequence: ::std::num::NonZeroU32) -> Self {
                self.event_sequence = event_sequence;
                self
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AuditActor;

    struct PreparedAudit {
        id: String,
        actor_id: String,
        actor_account: String,
        actor_type: AccountKind,
        actor_name_snapshot: Option<String>,
        request_id: Option<String>,
        event_sequence: NonZeroU32,
    }

    impl_audit_context!(PreparedAudit);

    #[test]
    fn authenticated_context_keeps_identity_and_optional_snapshots() {
        let actor = AuditActor::new("actor-1".into(), "sales".into(), AccountKind::Admin);
        let legacy = AuditContext::from(actor.clone());
        assert_eq!(legacy.actor_id, "actor-1");
        assert_eq!(legacy.actor_account, "sales");
        assert_eq!(legacy.actor_type, AccountKind::Admin);
        assert_eq!(legacy.actor_name_snapshot, None);
        assert_eq!(legacy.request_id, None);
        assert_eq!(legacy.event_sequence, NonZeroU32::MIN);

        let original = AuditContext::from(
            actor
                .with_actor_name_snapshot(Some("发生时名称".into()))
                .unwrap()
                .with_request_id(Some("request-original".into()))
                .unwrap(),
        );
        assert_eq!(original.actor_name_snapshot.as_deref(), Some("发生时名称"));
        assert_eq!(original.request_id.as_deref(), Some("request-original"));
        assert_eq!(original.event_sequence, NonZeroU32::MIN);
    }

    #[test]
    fn prepared_context_preserves_raw_fields_for_adapter_validation() {
        let prepared = PreparedAudit {
            id: "audit-original".into(),
            actor_id: "actor-1".into(),
            actor_account: "sales".into(),
            actor_type: AccountKind::Admin,
            actor_name_snapshot: None,
            request_id: None,
            event_sequence: NonZeroU32::MIN,
        }
        .with_actor_name_snapshot(Some("unsafe\nname".into()))
        .with_request_id(Some("unsafe\nrequest".into()))
        .with_event_sequence(NonZeroU32::new(7).unwrap());

        let context = prepared.audit_context();
        assert_eq!(context.actor_name_snapshot.as_deref(), Some("unsafe\nname"));
        assert_eq!(context.request_id.as_deref(), Some("unsafe\nrequest"));
        assert_eq!(context.event_sequence.get(), 7);
        assert_eq!(context.actor_id, "actor-1");
        assert_eq!(context.actor_account, "sales");
        assert_eq!(context.actor_type, AccountKind::Admin);
        assert_eq!(prepared.id, "audit-original");

        let cleared = prepared.with_actor_name_snapshot(None).with_request_id(None).audit_context();
        assert_eq!(cleared.actor_name_snapshot, None);
        assert_eq!(cleared.request_id, None);
        assert_eq!(cleared.event_sequence.get(), 7);
    }
}
