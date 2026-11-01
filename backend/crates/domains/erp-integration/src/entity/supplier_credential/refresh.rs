use super::*;

impl SupplierCredentialRefreshState {
    /// 返回稳定刷新阶段，状态显示不得格式化完整凭据对象。
    /// # 参数
    /// 无。
    /// # 返回
    /// 固定阶段代码。
    /// # 错误
    /// 不返回错误。
    pub fn code(&self) -> &'static str {
        match self {
            Self::Ready => "READY",
            Self::Claimed { .. } => "CLAIMED",
            Self::SentUnknown { .. } => "SENT_UNKNOWN",
            Self::ReauthorizationRequired { .. } => "REAUTHORIZATION_REQUIRED",
            Self::AuthorizationClaimed { .. } => "AUTHORIZATION_CLAIMED",
            Self::AuthorizationSentUnknown { .. } => "AUTHORIZATION_SENT_UNKNOWN",
            Self::AuthorizationRejected { .. } => "AUTHORIZATION_REJECTED",
        }
    }

    /// 返回原刷新发送期限，未知状态不会因该期限到达而自动重发。
    /// # 参数
    /// 无。
    /// # 返回
    /// 原领取或发送截止时刻；Ready无期限。
    /// # 错误
    /// 不返回错误。
    pub fn deadline(&self) -> Option<Instant> {
        match self {
            Self::Ready | Self::AuthorizationRejected { .. } => None,
            Self::Claimed { lease_until, .. }
            | Self::SentUnknown { lease_until, .. }
            | Self::ReauthorizationRequired { lease_until, .. }
            | Self::AuthorizationClaimed { lease_until, .. }
            | Self::AuthorizationSentUnknown { lease_until, .. } => Some(*lease_until),
        }
    }

    fn reauthorization_source(&self, expires_at: Instant, now: Instant) -> Result<Option<String>> {
        match self {
            Self::SentUnknown { operation_id, lease_until, .. } if *lease_until <= now => {
                Ok(Some(operation_id.clone()))
            },
            Self::ReauthorizationRequired { operation_id, .. } => Ok(Some(operation_id.clone())),
            Self::Ready if expires_at <= now => Ok(None),
            Self::Claimed { operation_id, .. } if expires_at <= now => Ok(Some(operation_id.clone())),
            _ => Err(Error::ConflictError("原授权未到期、刷新仍在发送期限内或不需要重新授权".into())),
        }
    }
}

impl SupplierCredential {
    /// 领取一次未发送刷新；未知发送永不因租期到达而重新领取。
    /// # 参数
    /// 所见版本、唯一刷新操作与所有者、有界租期和服务器当前时间。
    /// # 返回
    /// 领取成功返回真，已有活跃领取或发送未知返回假。
    /// # 错误
    /// 身份、版本、重复已完成操作或时间边界无效时拒绝。
    pub fn claim_refresh(&mut self, expected: u64, claim: &SupplierCredentialRefreshClaim) -> Result<bool> {
        self.require_version(expected)?;
        identity(&claim.operation_id, 128)?;
        identity(&claim.owner_id, 128)?;
        if !(1..=300).contains(&claim.lease_seconds)
            || self.last_completed_refresh_operation_id.as_deref() == Some(claim.operation_id.as_str())
        {
            return Err(Error::ValidationError("刷新租期或原操作身份无效".into()));
        }
        let allowed = matches!(self.refresh, SupplierCredentialRefreshState::Ready)
            || matches!(self.refresh, SupplierCredentialRefreshState::Claimed { lease_until, .. } if lease_until <= claim.now);
        if !allowed {
            return Ok(false);
        }
        let until = claim
            .now
            .unix_secs()
            .checked_add(
                i64::try_from(claim.lease_seconds)
                    .map_err(|_| Error::ValidationError("刷新租期超限".into()))?,
            )
            .ok_or_else(|| Error::ValidationError("刷新租期超限".into()))?;
        let base = self.next_base(claim.now)?;
        self.refresh = SupplierCredentialRefreshState::Claimed {
            owner_id: claim.owner_id.clone(),
            operation_id: claim.operation_id.clone(),
            lease_until: Instant::from_unix_secs(until),
        };
        self.base = base;
        Ok(true)
    }

    /// 在HTTP之前可靠登记可能已发送；后续失败不能返回Ready后自动再发。
    /// # 参数
    /// `now` 为发送前服务器时刻。
    /// # 返回
    /// 保留原操作及所有者的发送未知阶段。
    /// # 错误
    /// 未领取、领取已到期或版本边界无效时拒绝。
    pub fn mark_refresh_sent(&mut self, now: Instant) -> Result<()> {
        let SupplierCredentialRefreshState::Claimed { owner_id, operation_id, lease_until } = &self.refresh
        else {
            return Err(Error::ConflictError("原刷新没有有效的未发送领取".into()));
        };
        if *lease_until <= now {
            return Err(Error::ConflictError("原刷新领取已到期，不能发送".into()));
        }
        let state = SupplierCredentialRefreshState::SentUnknown {
            owner_id: owner_id.clone(),
            operation_id: operation_id.clone(),
            lease_until: *lease_until,
            sent_at: now,
        };
        let base = self.next_base(now)?;
        self.refresh = state;
        self.base = base;
        Ok(())
    }

    /// 仅原发送身份保存核实成功的新令牌，允许原响应晚于领取期限返回。
    /// # 参数
    /// `payload` 为本次原刷新成功响应的受控密文，`now` 为保存时间。
    /// # 返回
    /// 新令牌及原完成操作身份已经同时更新。
    /// # 错误
    /// 原状态不是发送未知、新令牌无效或版本超限时拒绝。
    pub fn complete_refresh(&mut self, payload: SupplierCredentialPayload, now: Instant) -> Result<()> {
        let SupplierCredentialRefreshState::SentUnknown { operation_id, .. } = &self.refresh else {
            return Err(Error::ConflictError("原刷新不允许保存成功结果".into()));
        };
        payload.validate(now)?;
        let operation_id = operation_id.clone();
        let base = self.next_base(now)?;
        self.encrypted_payload = payload.encrypted_payload;
        self.payload_fingerprint = payload.payload_fingerprint;
        self.expires_at = payload.expires_at;
        self.refresh = SupplierCredentialRefreshState::Ready;
        self.last_completed_refresh_operation_id = Some(operation_id);
        self.base = base;
        Ok(())
    }

    /// 核实InvalidGrant或撤销后保存重新授权要求，禁止回退旧access token。
    /// # 参数
    /// `now` 为协议层核实失败响应的服务器时刻。
    /// # 返回
    /// 保留原发送身份及截止时刻的授权失效状态。
    /// # 错误
    /// 非原发送未知阶段或版本边界无效时拒绝。
    pub fn invalidate_refresh(&mut self, now: Instant) -> Result<()> {
        let SupplierCredentialRefreshState::SentUnknown { owner_id, operation_id, lease_until, sent_at } =
            &self.refresh
        else {
            return Err(Error::ConflictError("原刷新不允许标记授权失效".into()));
        };
        let state = SupplierCredentialRefreshState::ReauthorizationRequired {
            owner_id: owner_id.clone(),
            operation_id: operation_id.clone(),
            lease_until: *lease_until,
            sent_at: *sent_at,
            invalidated_at: now,
        };
        let base = self.next_base(now)?;
        self.refresh = state;
        self.base = base;
        Ok(())
    }

    /// 释放本次尚未发送的领取；任何已发送状态不能使用本入口。
    /// # 参数
    /// `now` 为当前服务器时刻。
    /// # 返回
    /// 有效未发送领取退回Ready。
    /// # 错误
    /// 领取已到期、原状态不符或版本超限时拒绝。
    pub fn release_claim(&mut self, now: Instant) -> Result<()> {
        if !matches!(self.refresh, SupplierCredentialRefreshState::Claimed { lease_until, .. } if lease_until > now)
        {
            return Err(Error::ConflictError("原刷新已发送或领取无效，不能释放".into()));
        }
        let base = self.next_base(now)?;
        self.refresh = SupplierCredentialRefreshState::Ready;
        self.base = base;
        Ok(())
    }

    /// 以明确新授权恢复同一凭据引用，记录原失效、未知刷新或已过期授权及核验依据。
    /// # 参数
    /// 新授权密文、显式恢复命令和服务器时刻。
    /// # 返回
    /// 首次恢复返回真；同操作同依据重放返回假。
    /// # 错误
    /// 非新授权、发送期限未过、身份版本不符、重复异载荷或历史已满时拒绝。
    pub fn reauthorize(
        &mut self,
        payload: SupplierCredentialPayload,
        recovery: &SupplierCredentialReauthorization,
        now: Instant,
    ) -> Result<bool> {
        self.require_unbound_import()?;
        validate_reauthorization(recovery)?;
        payload.validate_shape()?;
        let fingerprint = payload.payload_fingerprint.clone();
        if let Some(previous) =
            self.reauthorization_history.iter().find(|entry| entry.operation_id == recovery.operation_id)
        {
            return replay_reauthorization(previous, recovery, &fingerprint);
        }
        payload.validate(now)?;
        self.require_version(recovery.expected_version)?;
        let operation = self.refresh.reauthorization_source(self.expires_at, now)?;
        if self.reauthorization_history.len() >= 256 {
            return Err(Error::BusinessLogicError("技术凭据授权恢复历史已达上限，需受控归档后处理".into()));
        }
        let base = self.next_base(now)?;
        self.reauthorization_history.push(SupplierCredentialReauthorizationEvent {
            operation_id: recovery.operation_id.clone(),
            actor_id: recovery.actor_id.clone(),
            evidence_reference: recovery.evidence_reference.clone(),
            previous_refresh_operation_id: operation,
            previous_version: self.base.version,
            occurred_at: now,
            payload_fingerprint: fingerprint,
        });
        self.encrypted_payload = payload.encrypted_payload;
        self.payload_fingerprint = payload.payload_fingerprint;
        self.expires_at = payload.expires_at;
        self.refresh = SupplierCredentialRefreshState::Ready;
        self.base = base;
        Ok(true)
    }
}

fn validate_reauthorization(recovery: &SupplierCredentialReauthorization) -> Result<()> {
    if !recovery.new_authorization {
        return Err(Error::ValidationError("必须明确登记新的授权，不能将重试当作重新授权".into()));
    }
    identity(&recovery.operation_id, 128)?;
    identity(&recovery.actor_id, 128)?;
    identity(&recovery.evidence_reference, 1024)
}

fn replay_reauthorization(
    previous: &SupplierCredentialReauthorizationEvent,
    recovery: &SupplierCredentialReauthorization,
    fingerprint: &str,
) -> Result<bool> {
    if previous.actor_id != recovery.actor_id
        || previous.evidence_reference != recovery.evidence_reference
        || previous.previous_version != recovery.expected_version
        || previous.payload_fingerprint != fingerprint
    {
        return Err(Error::ConflictError("同一重新授权操作不能用于不同依据或令牌".into()));
    }
    Ok(false)
}
