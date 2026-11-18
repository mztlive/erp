use super::*;

/// 页面授权只保存账号HMAC；账号密码、授权码及令牌不进入领取命令或历史。
#[derive(Clone)]
pub struct SupplierCredentialAuthorizationRequest {
    pub expected_version: Option<u64>,
    pub claim: SupplierCredentialRefreshClaim,
    pub new_authorization: bool,
    pub actor_id: String,
    pub evidence_reference: String,
    pub account_identity_fingerprint: String,
}

/// 所有授权操作均保留回执，旧操作即使晚于后续授权重放也不能再次发送HTTP。
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupplierCredentialAuthorizationEvent {
    pub operation_id: String,
    pub owner_id: String,
    pub actor_id: String,
    pub evidence_reference: String,
    pub previous_version: Option<u64>,
    pub new_authorization: bool,
    pub account_identity_fingerprint: String,
    pub previous_operation_id: Option<String>,
    pub claimed_at: Instant,
    pub sent_at: Option<Instant>,
    pub exchange_sent_at: Option<Instant>,
    pub completed_at: Option<Instant>,
    pub rejected_at: Option<Instant>,
    pub payload_fingerprint: Option<String>,
    pub authorized_uid_fingerprint: Option<String>,
}

impl SupplierCredentialAuthorizationRequest {
    pub(super) fn deadline(&self) -> Result<Instant> {
        identity(&self.claim.owner_id, 128)?;
        identity(&self.claim.operation_id, 128)?;
        identity(&self.actor_id, 128)?;
        identity(&self.evidence_reference, 1024)?;
        fingerprint(&self.account_identity_fingerprint)?;
        if !(1..=300).contains(&self.claim.lease_seconds) || self.expected_version == Some(0) {
            return Err(Error::ValidationError("授权租期或所见版本无效".into()));
        }

        let seconds = i64::try_from(self.claim.lease_seconds)
            .map_err(|_| Error::ValidationError("授权租期超限".into()))?;
        self.claim
            .now
            .unix_secs()
            .checked_add(seconds)
            .map(Instant::from_unix_secs)
            .ok_or_else(|| Error::ValidationError("授权租期超限".into()))
    }
}

impl SupplierCredentialAuthorizationEvent {
    pub(super) fn new(
        request: &SupplierCredentialAuthorizationRequest,
        previous_operation_id: Option<String>,
    ) -> Self {
        Self {
            operation_id: request.claim.operation_id.clone(),
            owner_id: request.claim.owner_id.clone(),
            actor_id: request.actor_id.clone(),
            evidence_reference: request.evidence_reference.clone(),
            previous_version: request.expected_version,
            new_authorization: request.new_authorization,
            account_identity_fingerprint: request.account_identity_fingerprint.clone(),
            previous_operation_id,
            claimed_at: request.claim.now,
            sent_at: None,
            exchange_sent_at: None,
            completed_at: None,
            rejected_at: None,
            payload_fingerprint: None,
            authorized_uid_fingerprint: None,
        }
    }

    pub(super) fn require_replay(&self, request: &SupplierCredentialAuthorizationRequest) -> Result<()> {
        if self.previous_version != request.expected_version
            || self.new_authorization != request.new_authorization
            || self.actor_id != request.actor_id
            || self.evidence_reference != request.evidence_reference
            || self.account_identity_fingerprint != request.account_identity_fingerprint
        {
            return Err(Error::ConflictError("同一授权操作不能切换账号、原版本或核验依据".into()));
        }
        Ok(())
    }
}

pub(super) fn fingerprint(value: &str) -> Result<()> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(Error::ValidationError("授权身份指纹无效".into()));
    }
    Ok(())
}
