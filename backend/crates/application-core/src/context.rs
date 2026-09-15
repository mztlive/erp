//! 调用人身份数据。

use erp_core::AccountKind;

/// 已通过 HTTP 鉴权的审计操作人。
///
/// 该类型只携带操作人身份；审计动作、资源类型和目标由具体 Service 决定，
/// 避免协议层伪造或遗漏业务审计语义。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditActor {
    actor_id: String,
    actor_account: String,
    actor_type: AccountKind,
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
        Self { actor_id, actor_account, actor_type }
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
