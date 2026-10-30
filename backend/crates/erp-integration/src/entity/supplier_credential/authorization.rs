use super::authorization_types::fingerprint;
use super::*;

impl SupplierCredential {
    /// 首次授权前建立无令牌占位，唯一索引必须先于本记录持久化。
    /// # 参数
    /// 固定引用、部署绑定及含候选账号HMAC的首次领取命令。
    /// # 返回
    /// 版本1的授权领取记录；只允许新引用且未声明重新授权。
    /// # 错误
    /// 身份、核验依据、版本、租期或时间无效时拒绝。
    pub fn reserve_authorization(
        id: String,
        binding: SupplierCredentialBinding,
        request: &SupplierCredentialAuthorizationRequest,
    ) -> Result<Self> {
        identity(&id, 128)?;
        binding.validate()?;
        let lease_until = request.deadline()?;
        if request.expected_version.is_some() || request.new_authorization {
            return Err(Error::ConflictError("首次授权不能携带原版本或冒充原授权恢复".into()));
        }
        let timestamp = u64::try_from(request.claim.now.unix_secs())
            .map_err(|_| Error::ValidationError("技术凭据时间无效".into()))?;
        let mut base = BaseModel::new(id);
        base.created_at = timestamp;
        base.updated_at = timestamp;
        Ok(Self {
            base,
            provider: binding.provider,
            account_scope: binding.account_scope,
            app_key: binding.app_key,
            encrypted_payload: String::new(),
            payload_fingerprint: String::new(),
            expires_at: request.claim.now,
            refresh: SupplierCredentialRefreshState::AuthorizationClaimed {
                owner_id: request.claim.owner_id.clone(),
                operation_id: request.claim.operation_id.clone(),
                lease_until,
            },
            last_completed_refresh_operation_id: None,
            reauthorization_history: vec![],
            account_identity_fingerprint: None,
            authorization_candidate_identity_fingerprint: Some(request.account_identity_fingerprint.clone()),
            authorized_uid_fingerprint: None,
            last_completed_authorization_operation_id: None,
            authorization_history: vec![SupplierCredentialAuthorizationEvent::new(request, None)],
            manually_configured: false,
            configuration_history: vec![],
        })
    }

    /// 同一凭据上领取一次授权；任何历史同操作重放均为零写且不能再次发送。
    /// # 参数
    /// 原版本、候选账号HMAC、唯一操作及明确重新授权依据。
    /// # 返回
    /// 新领取返回真；同操作同依据只读重放返回假。
    /// # 错误
    /// 未确认账号、跨账号、活跃刷新、未知期限未过或版本冲突时拒绝。
    pub fn claim_authorization(&mut self, request: &SupplierCredentialAuthorizationRequest) -> Result<bool> {
        let lease_until = request.deadline()?;
        if let Some(event) =
            self.authorization_history.iter().find(|event| event.operation_id == request.claim.operation_id)
        {
            event.require_replay(request)?;
            return Ok(false);
        }
        let expected = request
            .expected_version
            .ok_or_else(|| Error::ConflictError("原凭据授权必须指定所见版本".into()))?;
        self.require_version(expected)?;
        if self.configuration_history.iter().any(|event| event.operation_id == request.claim.operation_id) {
            return Err(Error::ConflictError("授权操作编号已被令牌配置使用".into()));
        }
        self.require_authorization_identity(&request.account_identity_fingerprint)?;
        let previous_operation = self.authorization_source(request)?;
        if self.authorization_history.len() >= 256 {
            return Err(Error::BusinessLogicError("授权操作历史已达上限，需受控处理".into()));
        }
        let base = self.next_base(request.claim.now)?;
        self.authorization_history
            .push(SupplierCredentialAuthorizationEvent::new(request, previous_operation));
        self.authorization_candidate_identity_fingerprint =
            Some(request.account_identity_fingerprint.clone());
        self.refresh = SupplierCredentialRefreshState::AuthorizationClaimed {
            owner_id: request.claim.owner_id.clone(),
            operation_id: request.claim.operation_id.clone(),
            lease_until,
        };
        self.base = base;
        Ok(true)
    }

    /// 在authorizeForVOP之前登记可能已发送，失败和崩溃不能自动再次发送。
    /// # 参数
    /// `now`为实际发送前的服务器时刻。
    /// # 返回
    /// 保留候选身份、操作和所有者的授权发送未知状态。
    /// # 错误
    /// 非原领取、租期已过或历史不完整时拒绝。
    pub fn mark_authorization_sent(&mut self, now: Instant) -> Result<()> {
        let SupplierCredentialRefreshState::AuthorizationClaimed { owner_id, operation_id, lease_until } =
            &self.refresh
        else {
            return Err(Error::ConflictError("授权没有有效的原领取".into()));
        };
        if *lease_until <= now {
            return Err(Error::ConflictError("授权领取已到期，禁止发送".into()));
        }
        let index = self.authorization_event_index(owner_id, operation_id)?;
        let state = SupplierCredentialRefreshState::AuthorizationSentUnknown {
            owner_id: owner_id.clone(),
            operation_id: operation_id.clone(),
            lease_until: *lease_until,
            sent_at: now,
            exchange_sent_at: None,
        };
        let base = self.next_base(now)?;
        self.authorization_history[index].sent_at = Some(now);
        self.refresh = state;
        self.base = base;
        Ok(())
    }

    /// 在授权码兑换令牌之前记录第二个发送边界，之后不得按账号密码拒绝释放身份。
    /// # 参数
    /// `now`为GET access_token前的服务器时刻。
    /// # 返回
    /// 同原操作的兑换发送未知阶段。
    /// # 错误
    /// 已经兑换、租期已过或原状态不符时拒绝。
    pub fn mark_authorization_exchange_sent(&mut self, now: Instant) -> Result<()> {
        let SupplierCredentialRefreshState::AuthorizationSentUnknown {
            owner_id,
            operation_id,
            lease_until,
            exchange_sent_at: None,
            ..
        } = &self.refresh
        else {
            return Err(Error::ConflictError("授权兑换没有有效的原发送身份".into()));
        };
        if *lease_until <= now {
            return Err(Error::ConflictError("授权发送期限已到，禁止兑换".into()));
        }
        let index = self.authorization_event_index(owner_id, operation_id)?;
        let base = self.next_base(now)?;
        let SupplierCredentialRefreshState::AuthorizationSentUnknown { exchange_sent_at, .. } =
            &mut self.refresh
        else {
            return Err(Error::ConflictError("原授权状态已变化".into()));
        };
        *exchange_sent_at = Some(now);
        self.authorization_history[index].exchange_sent_at = Some(now);
        self.base = base;
        Ok(())
    }

    /// 仅记录authorizeForVOP明确拒绝，首次未确认账号可在新操作中纠正。
    /// # 参数
    /// `now`为协议层核实原authorizeForVOP拒绝的时刻。
    /// # 返回
    /// 保留拒绝回执；无既有令牌的首次候选身份解除锁定。
    /// # 错误
    /// 已开始兑换、原发送身份缺失或版本边界无效时拒绝。
    pub fn reject_authorization(&mut self, now: Instant) -> Result<()> {
        let SupplierCredentialRefreshState::AuthorizationSentUnknown {
            owner_id,
            operation_id,
            exchange_sent_at: None,
            ..
        } = &self.refresh
        else {
            return Err(Error::ConflictError("授权未得到可安全释放候选身份的明确拒绝".into()));
        };
        let index = self.authorization_event_index(owner_id, operation_id)?;
        let operation_id = operation_id.clone();
        let base = self.next_base(now)?;
        if self.account_identity_fingerprint.is_none() && self.encrypted_payload.is_empty() {
            self.authorization_candidate_identity_fingerprint = None;
        }
        self.authorization_history[index].rejected_at = Some(now);
        self.refresh =
            SupplierCredentialRefreshState::AuthorizationRejected { operation_id, rejected_at: now };
        self.base = base;
        Ok(())
    }

    /// 保存原兑换结果并确认账号及可选UID身份，旧响应不能覆盖新授权。
    /// # 参数
    /// 受控新密文、协议层核实的UID HMAC及服务器保存时刻。
    /// # 返回
    /// 新令牌、确认身份和原授权完成回执同时更新为Ready。
    /// # 错误
    /// 未登记兑换、身份变化、令牌无效或历史缺失时拒绝。
    pub fn complete_authorization(
        &mut self,
        payload: SupplierCredentialPayload,
        uid: Option<String>,
        now: Instant,
    ) -> Result<()> {
        let SupplierCredentialRefreshState::AuthorizationSentUnknown {
            owner_id,
            operation_id,
            exchange_sent_at: Some(_),
            ..
        } = &self.refresh
        else {
            return Err(Error::ConflictError("原授权未登记令牌兑换发送，不能保存结果".into()));
        };
        payload.validate(now)?;
        let index = self.authorization_event_index(owner_id, operation_id)?;
        let operation_id = operation_id.clone();
        let candidate = self
            .authorization_candidate_identity_fingerprint
            .clone()
            .ok_or_else(|| Error::ConflictError("原授权候选身份缺失".into()))?;
        self.require_authorization_identity(&candidate)?;
        self.require_authorization_uid(uid.as_deref())?;
        let base = self.next_base(now)?;
        self.authorization_history[index].completed_at = Some(now);
        self.authorization_history[index].payload_fingerprint = Some(payload.payload_fingerprint.clone());
        self.authorization_history[index].authorized_uid_fingerprint = uid.clone();
        self.account_identity_fingerprint = Some(candidate);
        self.manually_configured = false;
        self.authorization_candidate_identity_fingerprint = None;
        self.authorized_uid_fingerprint = uid.or_else(|| self.authorized_uid_fingerprint.clone());
        self.encrypted_payload = payload.encrypted_payload;
        self.payload_fingerprint = payload.payload_fingerprint;
        self.expires_at = payload.expires_at;
        self.last_completed_authorization_operation_id = Some(operation_id);
        self.refresh = SupplierCredentialRefreshState::Ready;
        self.base = base;
        Ok(())
    }

    fn require_authorization_identity(&self, candidate: &str) -> Result<()> {
        fingerprint(candidate)?;
        let locked = self
            .account_identity_fingerprint
            .as_deref()
            .or(self.authorization_candidate_identity_fingerprint.as_deref());
        if locked.is_some_and(|original| original != candidate)
            || (locked.is_none() && !self.encrypted_payload.is_empty())
        {
            return Err(Error::ConflictError("原凭据账号身份未核实或与候选账号不一致".into()));
        }
        Ok(())
    }

    fn require_authorization_uid(&self, uid: Option<&str>) -> Result<()> {
        if self.authorized_uid_fingerprint.is_some() && uid.is_none() {
            return Err(Error::ConflictError("京东响应缺少原授权已确认的账号身份，需继续核实".into()));
        }
        if let Some(uid) = uid {
            fingerprint(uid)?;
            if self.authorized_uid_fingerprint.as_deref().is_some_and(|original| original != uid) {
                return Err(Error::ConflictError("京东返回账号身份与原授权不一致".into()));
            }
        }
        Ok(())
    }

    fn authorization_source(
        &self,
        request: &SupplierCredentialAuthorizationRequest,
    ) -> Result<Option<String>> {
        let initial_rejected =
            matches!(self.refresh, SupplierCredentialRefreshState::AuthorizationRejected { .. })
                && self.account_identity_fingerprint.is_none()
                && self.encrypted_payload.is_empty();
        if !request.new_authorization && !initial_rejected {
            return Err(Error::ConflictError("原授权恢复必须明确登记新授权及核验依据".into()));
        }
        match &self.refresh {
            SupplierCredentialRefreshState::Ready if self.account_identity_fingerprint.is_some() => Ok(None),
            SupplierCredentialRefreshState::ReauthorizationRequired { operation_id, .. }
            | SupplierCredentialRefreshState::AuthorizationRejected { operation_id, .. } => {
                Ok(Some(operation_id.clone()))
            },
            SupplierCredentialRefreshState::Claimed { operation_id, lease_until, .. }
            | SupplierCredentialRefreshState::SentUnknown { operation_id, lease_until, .. }
            | SupplierCredentialRefreshState::AuthorizationClaimed { operation_id, lease_until, .. }
            | SupplierCredentialRefreshState::AuthorizationSentUnknown {
                operation_id, lease_until, ..
            } if *lease_until <= request.claim.now => Ok(Some(operation_id.clone())),
            _ => Err(Error::ConflictError("原刷新或授权仍在发送期限内，禁止并发授权".into())),
        }
    }

    fn authorization_event_index(&self, owner: &str, operation: &str) -> Result<usize> {
        self.authorization_history
            .iter()
            .position(|event| {
                event.owner_id == owner
                    && event.operation_id == operation
                    && event.completed_at.is_none()
                    && event.rejected_at.is_none()
            })
            .ok_or_else(|| Error::ConflictError("原授权操作历史或所有者不匹配".into()))
    }
}

#[cfg(test)]
mod tests;
