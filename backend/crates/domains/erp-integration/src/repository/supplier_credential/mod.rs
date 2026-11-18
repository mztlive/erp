//! 技术凭据专用窄仓储；不暴露任意update或删除，所有状态变更均执行版本及身份CAS。

mod authorization;
mod configuration;
mod filters;

use erp_core::common::time::Instant;
use mongodb::bson::Document;
use mongodb::{Collection, Database};
use persistence_core::{Executor, mongo_ops};

use crate::entity::supplier_credential::{
    SupplierCredential, SupplierCredentialBinding, SupplierCredentialPayload,
    SupplierCredentialReauthorization, SupplierCredentialRefreshClaim, identity,
};
use crate::{Error, Result};

pub trait SupplierCredentialExt {
    const SUPPLIER_CREDENTIALS: &'static str = "supplier_credentials";

    /// 取得本域技术凭据窄仓储，不交付任意集合写入能力。与当前应用数据库绑定的凭据仓储。
    fn supplier_credentials(&self) -> SupplierCredentialRepository<'_>;
}

impl SupplierCredentialExt for Database {
    fn supplier_credentials(&self) -> SupplierCredentialRepository<'_> {
        SupplierCredentialRepository { db: self }
    }
}

pub struct SupplierCredentialRepository<'a> {
    db: &'a Database,
}

impl SupplierCredentialRepository<'_> {
    /// 读取固定引用及同账号应用的密文凭据，不返回其他绑定的记录。
    ///
    /// 当前密文及刷新状态；仅原引用不存在时返回None。身份非法、固定引用绑定不符或数据库失败时返回脱敏错误。
    pub async fn load(
        &self,
        id: &str,
        binding: &SupplierCredentialBinding,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierCredential>> {
        binding.validate()?;
        let current = self.by_id(id, executor).await?;
        if let Some(record) = &current {
            record.require_binding(binding)?;
        }
        Ok(current)
    }

    /// 新建或按所见版本普通导入，禁止借此覆盖发送未知或授权失效。
    ///
    /// 新记录或CAS更新后的原记录；None版本只允许新建。身份变化、缺少所见版本、状态冲突、唯一冲突或持久化失败时拒绝。
    ///
    /// 原引用、绑定、受控密文、可选原版本、当前时间及执行器。
    pub async fn import(
        &self,
        id: &str,
        binding: &SupplierCredentialBinding,
        payload: SupplierCredentialPayload,
        expected_version: Option<u64>,
        now: Instant,
        executor: &mut dyn Executor,
    ) -> Result<SupplierCredential> {
        let existing = self.by_id(id, executor).await?;
        match (existing, expected_version) {
            (None, None) => {
                let record = SupplierCredential::new(id.into(), binding.clone(), payload, now)?;
                mongo_ops::insert_one(&self.collection(), &record, executor).await?;
                Ok(record)
            },
            (Some(current), Some(expected)) => {
                current.require_binding(binding)?;
                let mut next = current.clone();
                next.import(payload, expected, now)?;

                let filter = filters::ordinary_import(&current, expected, now)?;
                self.replace(filter, &next, executor).await?.ok_or_else(changed)
            },
            _ => Err(Error::ConflictError("技术凭据已存在或所见版本缺失，不能隐式覆盖或新建".into())),
        }
    }

    /// 原子领取一个尚未发送的刷新，发送未知永不参加到期竞争。
    ///
    /// 胜出的领取记录；状态不允许或CAS竞争失败返回None。非法命令、版本不符或数据库失败时拒绝。
    pub async fn claim_refresh(
        &self,
        id: &str,
        binding: &SupplierCredentialBinding,
        expected_version: u64,
        claim: SupplierCredentialRefreshClaim,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierCredential>> {
        let Some(current) = self.load(id, binding, executor).await? else {
            return Ok(None);
        };

        let mut next = current.clone();
        if !next.claim_refresh(expected_version, &claim)? {
            return Ok(None);
        }

        self.replace(filters::claim(&current, expected_version, &claim)?, &next, executor).await
    }

    /// 发送刷新 HTTP 请求前，按原领取版本和所有者 CAS 登记发送结果未知。
    /// 只有取得更新后的记录才允许本轮发送一次；竞争失败返回 `None`，状态、租期或持久化出错时同样不得发送。
    pub async fn mark_refresh_sent(
        &self,
        claimed: &SupplierCredential,
        now: Instant,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierCredential>> {
        let mut next = claimed.clone();
        next.mark_refresh_sent(now)?;
        self.replace(filters::claimed(claimed, now)?, &next, executor).await
    }

    /// 按原发送身份原子保存成功令牌，不覆盖期间已经重新授权的新版本。
    ///
    /// 新密文及完成操作回执；原CAS失效返回None。状态、密文或持久化失败时拒绝；保存未知须按原操作读取回执。
    ///
    /// 原发送快照、核实成功的新密文、当前时间及执行器。
    pub async fn complete_refresh(
        &self,
        sent: &SupplierCredential,
        payload: SupplierCredentialPayload,
        now: Instant,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierCredential>> {
        let mut next = sent.clone();
        next.complete_refresh(payload, now)?;
        self.replace(filters::sent(sent)?, &next, executor).await
    }

    /// 保存已由协议层核实的授权失效，不用网络错误推断InvalidGrant。
    ///
    /// 同身份CAS成功后的重新授权要求；旧版本返回None。非发送未知或持久化失败时拒绝。
    ///
    /// 原发送快照、服务器当前时间和执行器。
    pub async fn invalidate_refresh(
        &self,
        sent: &SupplierCredential,
        now: Instant,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierCredential>> {
        let mut next = sent.clone();
        next.invalidate_refresh(now)?;
        self.replace(filters::sent(sent)?, &next, executor).await
    }

    /// 仅释放未发送且仍由原所有者持有的领取。
    ///
    /// CAS成功返回真；被替代返回假。已发送、已到期或持久化失败时拒绝。
    ///
    /// 原领取快照、当前时间及执行器。
    pub async fn release_claim(
        &self,
        claimed: &SupplierCredential,
        now: Instant,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let mut next = claimed.clone();
        next.release_claim(now)?;
        Ok(self.replace(filters::claimed(claimed, now)?, &next, executor).await?.is_some())
    }

    /// 使用显式新授权恢复原引用，并以CAS使原HTTP响应失去覆盖资格。
    ///
    /// 同身份新授权或同操作原回执，不创建第二份凭据。原发送尚未到期、版本变化、证据不完整或持久化失败时拒绝。
    ///
    /// 原引用及绑定、新密文、含所见版本和核验人的恢复命令、当前时间及执行器。
    pub async fn reauthorize(
        &self,
        id: &str,
        binding: &SupplierCredentialBinding,
        payload: SupplierCredentialPayload,
        recovery: SupplierCredentialReauthorization,
        now: Instant,
        executor: &mut dyn Executor,
    ) -> Result<SupplierCredential> {
        let current = self
            .load(id, binding, executor)
            .await?
            .ok_or_else(|| Error::NotFound("原技术凭据不存在或绑定不符".into()))?;

        let mut next = current.clone();
        if !next.reauthorize(payload, &recovery, now)? {
            return Ok(current);
        }

        self.replace(filters::reauthorization(&current, recovery.expected_version, now)?, &next, executor)
            .await?
            .ok_or_else(changed)
    }

    async fn by_id(&self, id: &str, executor: &mut dyn Executor) -> Result<Option<SupplierCredential>> {
        identity(id, 128)?;
        Ok(mongo_ops::find_one(&self.collection(), filters::id(id), executor).await?)
    }

    fn collection(&self) -> Collection<SupplierCredential> {
        self.db.collection(Database::SUPPLIER_CREDENTIALS)
    }

    async fn replace(
        &self,
        filter: Document,
        next: &SupplierCredential,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierCredential>> {
        Ok(mongo_ops::find_one_and_update_pipeline(
            &self.collection(),
            filter,
            filters::replacement(next)?,
            executor,
        )
        .await?)
    }
}

fn changed() -> Error {
    Error::ConflictError("技术凭据版本或刷新所有者已变化，请查询原结果".into())
}
