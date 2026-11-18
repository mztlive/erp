use super::*;
use crate::entity::supplier_credential::SupplierCredentialConfiguration;

impl SupplierCredentialRepository<'_> {
    /// 在固定凭据引用保存页面提供的令牌，所有更新遵守原版本CAS。
    /// # 参数
    /// 部署引用及绑定、加密令牌、配置命令、当前时间及事务执行器。
    /// # 返回
    /// 当前记录和是否实际写入；重放不会覆盖后续刷新。
    /// # 错误
    /// 绑定、版本、活跃操作、唯一约束或持久化失败时拒绝。
    pub async fn configure_tokens(
        &self,
        id: &str,
        binding: &SupplierCredentialBinding,
        payload: SupplierCredentialPayload,
        command: &SupplierCredentialConfiguration,
        now: Instant,
        executor: &mut dyn Executor,
    ) -> Result<(SupplierCredential, bool)> {
        let Some(current) = self.load(id, binding, executor).await? else {
            let record = SupplierCredential::configured(id.into(), binding.clone(), payload, command, now)?;
            mongo_ops::insert_one(&self.collection(), &record, executor).await?;
            return Ok((record, true));
        };

        let mut next = current.clone();
        if !next.configure_tokens(payload, command, now)? {
            return Ok((current, false));
        }

        // 每次刷新和授权状态迁移均推进version，旧远端结果不能覆盖新配置。
        let filter = filters::version(&current, current.base.version)?;
        let saved = self.replace(filter, &next, executor).await?.ok_or_else(changed)?;
        Ok((saved, true))
    }
}
