use super::*;

/// 由有权限的操作人显式配置同一账号、同一应用的外部授权结果。
#[derive(Clone)]
pub struct SupplierCredentialConfiguration {
    pub operation_id: String,
    pub actor_id: String,
    pub expected_version: Option<u64>,
}

/// 仅保存操作回执及密钥保护的摘要，不保存令牌明文。
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupplierCredentialConfigurationEvent {
    pub operation_id: String,
    pub actor_id: String,
    pub previous_version: Option<u64>,
    pub previous_operation_id: Option<String>,
    pub payload_fingerprint: String,
    pub occurred_at: Instant,
}

impl SupplierCredentialConfiguration {
    fn validate(&self) -> Result<()> {
        identity(&self.operation_id, 128)?;
        identity(&self.actor_id, 128)?;
        if self.expected_version.is_some_and(|value| value == 0 || i64::try_from(value).is_err()) {
            return Err(Error::ValidationError("令牌配置所见版本无效".into()));
        }
        Ok(())
    }

    fn event(
        &self,
        payload: &SupplierCredentialPayload,
        previous: Option<&SupplierCredential>,
        now: Instant,
    ) -> SupplierCredentialConfigurationEvent {
        SupplierCredentialConfigurationEvent {
            operation_id: self.operation_id.clone(),
            actor_id: self.actor_id.clone(),
            previous_version: self.expected_version,
            previous_operation_id: previous.and_then(|record| match &record.refresh {
                SupplierCredentialRefreshState::Ready => None,
                SupplierCredentialRefreshState::Claimed { operation_id, .. }
                | SupplierCredentialRefreshState::SentUnknown { operation_id, .. }
                | SupplierCredentialRefreshState::ReauthorizationRequired { operation_id, .. }
                | SupplierCredentialRefreshState::AuthorizationClaimed { operation_id, .. }
                | SupplierCredentialRefreshState::AuthorizationSentUnknown { operation_id, .. }
                | SupplierCredentialRefreshState::AuthorizationRejected { operation_id, .. } => {
                    Some(operation_id.clone())
                },
            }),
            payload_fingerprint: payload.payload_fingerprint.clone(),
            occurred_at: now,
        }
    }
}

impl SupplierCredential {
    /// 首次配置令牌，建立加密记录和可查询的操作回执。
    /// # 参数
    /// 固定身份、部署绑定、受控密文、操作命令及当前时间。
    /// # 返回
    /// 版本为1的可用凭据，不声明已核验令牌所属京东账号。
    /// # 错误
    /// 已携带原版本、命令或密文无效时拒绝。
    pub fn configured(
        id: String,
        binding: SupplierCredentialBinding,
        payload: SupplierCredentialPayload,
        command: &SupplierCredentialConfiguration,
        now: Instant,
    ) -> Result<Self> {
        command.validate()?;
        if command.expected_version.is_some() {
            return Err(Error::ConflictError("原令牌记录不存在，请刷新状态".into()));
        }
        let event = command.event(&payload, None, now);
        let mut record = Self::new(id, binding, payload, now)?;
        record.manually_configured = true;
        record.configuration_history.push(event);
        Ok(record)
    }

    /// 按所见版本替换令牌；同操作仅返回回执，不能回滚后续刷新。
    /// # 参数
    /// 新密文、操作人及原版本、服务器时间。
    /// # 返回
    /// 新配置返回真，同操作同内容重放返回假。
    /// # 错误
    /// 活跃授权或刷新、版本变化、操作冲突或历史容量耗尽时拒绝。
    pub fn configure_tokens(
        &mut self,
        payload: SupplierCredentialPayload,
        command: &SupplierCredentialConfiguration,
        now: Instant,
    ) -> Result<bool> {
        command.validate()?;
        payload.validate_shape()?;
        if let Some(event) =
            self.configuration_history.iter().find(|entry| entry.operation_id == command.operation_id)
        {
            if event.actor_id != command.actor_id
                || event.previous_version != command.expected_version
                || event.payload_fingerprint != payload.payload_fingerprint
            {
                return Err(Error::ConflictError("同一令牌配置操作不能更换内容或操作人".into()));
            }
            return Ok(false);
        }
        self.require_version(command.expected_version.unwrap_or(0))?;
        payload.validate(now)?;
        self.require_configuration_slot(&command.operation_id, now)?;
        let base = self.next_base(now)?;
        self.configuration_history.push(command.event(&payload, Some(self), now));
        self.encrypted_payload = payload.encrypted_payload;
        self.payload_fingerprint = payload.payload_fingerprint;
        self.expires_at = payload.expires_at;
        self.refresh = SupplierCredentialRefreshState::Ready;
        self.manually_configured = true;
        self.base = base;
        Ok(true)
    }

    fn require_configuration_slot(&self, operation_id: &str, now: Instant) -> Result<()> {
        if self.refresh.deadline().is_some_and(|deadline| deadline > now)
            || self.configuration_history.len() >= 256
            || self.authorization_history.iter().any(|event| event.operation_id == operation_id)
            || self.reauthorization_history.iter().any(|event| event.operation_id == operation_id)
        {
            return Err(Error::ConflictError("原授权操作尚未结束或令牌配置回执冲突，请刷新状态".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(seconds: i64) -> Instant {
        Instant::from_unix_secs(seconds)
    }
    fn payload(marker: char) -> SupplierCredentialPayload {
        SupplierCredentialPayload {
            encrypted_payload: format!("sealed-{marker}"),
            payload_fingerprint: marker.to_string().repeat(64),
            expires_at: at(1000),
        }
    }
    fn command(version: Option<u64>, operation: &str) -> SupplierCredentialConfiguration {
        SupplierCredentialConfiguration {
            operation_id: operation.into(),
            actor_id: "operator".into(),
            expected_version: version,
        }
    }
    fn configured() -> SupplierCredential {
        SupplierCredential::configured(
            "credential".into(),
            SupplierCredentialBinding {
                provider: SupplierCredentialProvider::JdVop,
                account_scope: "account".into(),
                app_key: "app".into(),
            },
            payload('a'),
            &command(None, "first"),
            at(100),
        )
        .unwrap()
    }

    #[test]
    fn replay_cannot_restore_old_tokens_after_refresh_or_change_actor() {
        let mut record = configured();
        assert!(record.manually_configured);
        assert!(record.account_identity_fingerprint.is_none());
        record
            .claim_refresh(
                1,
                &SupplierCredentialRefreshClaim {
                    owner_id: "worker".into(),
                    operation_id: "refresh".into(),
                    now: at(101),
                    lease_seconds: 30,
                },
            )
            .unwrap();
        record.mark_refresh_sent(at(102)).unwrap();
        record.complete_refresh(payload('b'), at(103)).unwrap();
        let saved = record.clone();
        assert!(!record.configure_tokens(payload('a'), &command(None, "first"), at(104)).unwrap());
        assert_eq!(record, saved);
        let mut replay = command(None, "first");
        replay.actor_id = "another".into();
        assert!(record.configure_tokens(payload('a'), &replay, at(105)).is_err());
        assert!(record.configure_tokens(payload('b'), &command(None, "first"), at(105)).is_err());
        assert!(record.configure_tokens(payload('a'), &command(Some(1), "stale"), at(105)).is_err());
        assert_eq!(record, saved);
    }

    #[test]
    fn configuration_blocks_active_send_and_recovers_expired_unknown_without_erasing_identity() {
        let mut record = configured();
        record.account_identity_fingerprint = Some("c".repeat(64));
        record.authorized_uid_fingerprint = Some("d".repeat(64));
        record
            .claim_refresh(
                1,
                &SupplierCredentialRefreshClaim {
                    owner_id: "worker".into(),
                    operation_id: "refresh".into(),
                    now: at(101),
                    lease_seconds: 30,
                },
            )
            .unwrap();
        record.mark_refresh_sent(at(102)).unwrap();
        let saved = record.clone();
        let replacement = command(Some(record.base.version), "replace");
        assert!(record.configure_tokens(payload('b'), &replacement, at(130)).is_err());
        assert_eq!(record, saved);
        assert!(record.configure_tokens(payload('b'), &replacement, at(131)).unwrap());
        assert_eq!(record.base.version, saved.base.version + 1);
        assert_eq!(record.account_identity_fingerprint, saved.account_identity_fingerprint);
        assert_eq!(record.authorized_uid_fingerprint, saved.authorized_uid_fingerprint);
        assert!(matches!(record.refresh, SupplierCredentialRefreshState::Ready));
        assert_eq!(record.configuration_history.len(), 2);
        assert_eq!(record.configuration_history[1].previous_operation_id.as_deref(), Some("refresh"));
    }

    #[test]
    fn active_oauth_is_fenced_and_expired_candidate_is_not_promoted_to_verified_account() {
        let mut record = configured();
        record.authorization_candidate_identity_fingerprint = Some("c".repeat(64));
        record.refresh = SupplierCredentialRefreshState::AuthorizationSentUnknown {
            owner_id: "worker".into(),
            operation_id: "oauth".into(),
            lease_until: at(200),
            sent_at: at(101),
            exchange_sent_at: None,
        };
        let replacement = command(Some(1), "replace");
        assert!(record.configure_tokens(payload('b'), &replacement, at(199)).is_err());
        assert!(record.configure_tokens(payload('b'), &replacement, at(200)).unwrap());
        assert!(record.account_identity_fingerprint.is_none());
        assert_eq!(record.authorization_candidate_identity_fingerprint, Some("c".repeat(64)));
    }
}
