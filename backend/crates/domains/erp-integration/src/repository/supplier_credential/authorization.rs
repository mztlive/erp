use super::*;
use crate::entity::supplier_credential::SupplierCredentialAuthorizationRequest;

impl SupplierCredentialRepository<'_> {
    /// 在任何授权HTTP前原子占用固定凭据，首次创建必须已有本集合唯一索引。
    /// # 参数
    /// 固定引用、部署绑定、原版本与候选身份命令、外层审计事务执行器。
    /// # 返回
    /// 原子新领取为真；同历史操作返回原记录及假，调用方禁止再发HTTP。
    /// # 错误
    /// 身份冲突、未知发送未到期、版本冲突、唯一冲突或持久化失败时拒绝。
    pub async fn reserve_authorization(
        &self,
        id: &str,
        binding: &SupplierCredentialBinding,
        request: SupplierCredentialAuthorizationRequest,
        executor: &mut dyn Executor,
    ) -> Result<(SupplierCredential, bool)> {
        let Some(current) = self.load(id, binding, executor).await? else {
            let record = SupplierCredential::reserve_authorization(id.into(), binding.clone(), &request)?;
            mongo_ops::insert_one(&self.collection(), &record, executor).await?;
            return Ok((record, true));
        };

        let mut next = current.clone();
        if !next.claim_authorization(&request)? {
            return Ok((current, false));
        }

        let record = self
            .replace(filters::authorization_claim(&current, &request)?, &next, executor)
            .await?
            .ok_or_else(changed)?;
        Ok((record, true))
    }

    /// 在authorizeForVOP发送前保存原所有者、操作及版本的发送边界。
    /// # 参数
    /// 原领取快照、服务器时间及外层执行器。
    /// # 返回
    /// 已持久化发送未知记录；竞争失败返回None且不得发送HTTP。
    /// # 错误
    /// 状态、租期或数据库失败时拒绝。
    pub async fn mark_authorization_sent(
        &self,
        claimed: &SupplierCredential,
        now: Instant,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierCredential>> {
        let mut next = claimed.clone();
        next.mark_authorization_sent(now)?;
        self.replace(filters::authorization_claimed(claimed, now)?, &next, executor).await
    }

    /// 在授权码兑换GET之前保存第二个发送边界，之后不能按账号密码拒绝释放候选。
    /// # 参数
    /// 原授权发送快照、服务器时间及外层执行器。
    /// # 返回
    /// 兑换发送未知记录；原CAS失效返回None且不得兑换。
    /// # 错误
    /// 已经兑换、租期已到或持久化失败时拒绝。
    pub async fn mark_authorization_exchange_sent(
        &self,
        sent: &SupplierCredential,
        now: Instant,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierCredential>> {
        let mut next = sent.clone();
        next.mark_authorization_exchange_sent(now)?;
        self.replace(filters::authorization_exchange(sent, now)?, &next, executor).await
    }

    /// 保存协议层已核实的authorizeForVOP拒绝，首次候选账号才可解除锁定。
    /// # 参数
    /// 未登记兑换的原发送快照、服务器时刻及执行器。
    /// # 返回
    /// 保留原拒绝回执的记录；原CAS失效返回None。
    /// # 错误
    /// 已兑换、非原操作或持久化失败时拒绝；网络错误不得调用本入口。
    pub async fn reject_authorization(
        &self,
        sent: &SupplierCredential,
        now: Instant,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierCredential>> {
        let mut next = sent.clone();
        next.reject_authorization(now)?;
        self.replace(filters::authorization_sent(sent)?, &next, executor).await
    }

    /// 保存原授权兑换结果、确认账号身份及可选官方UID HMAC。
    /// # 参数
    /// 原兑换快照、受控密文、已核实UID HMAC、当前时间及执行器。
    /// # 返回
    /// 新令牌及完成回执；旧操作CAS失效返回None。
    /// # 错误
    /// 账号或UID变化、无有效兑换边界、密文或持久化失败时拒绝。
    pub async fn complete_authorization(
        &self,
        sent: &SupplierCredential,
        payload: SupplierCredentialPayload,
        authorized_uid_fingerprint: Option<String>,
        now: Instant,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierCredential>> {
        let mut next = sent.clone();
        next.complete_authorization(payload, authorized_uid_fingerprint, now)?;
        self.replace(filters::authorization_sent(sent)?, &next, executor).await
    }
}
