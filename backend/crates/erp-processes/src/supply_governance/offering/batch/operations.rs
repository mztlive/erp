//! 三种批量操作复用单条供给用例。
use application_core::AuditActor;
use async_trait::async_trait;
use erp_supply::dto::supplier_offering::{
    CreateSupplierOfferingRequest, ReviseSupplierOfferingRequest, UpdateSupplierOfferingAvailabilityRequest,
};
use erp_supply::service::supplier_offering::CommandPreparation;
use persistence_core::NoTransaction;
use serde::Serialize;
use serde_json::Value;

use super::{BatchOperation, Targeted};
use crate::supply_governance::SupplierOfferingProcess;
use crate::{Error, Result};

/// 序列化已完成命令，不携带数据库错误细节。
fn result_value<T: Serialize>(value: T) -> Result<Value> {
    serde_json::to_value(value).map_err(|_| Error::Internal("序列化供给结果失败".into()))
}

/// 预检只回放已完成命令，待写入准备不跨请求缓存。
fn replay<P, R: Serialize>(prepared: CommandPreparation<P, R>) -> Result<Option<Value>> {
    match prepared {
        CommandPreparation::Apply(_) => Ok(None),
        CommandPreparation::Replay(result) => result_value(result).map(Some),
    }
}

#[async_trait]
impl BatchOperation for CreateSupplierOfferingRequest {
    /// 供应商订货编码构成唯一供给身份。
    fn identity(&self) -> String {
        format!("{}:{}:{}", self.supplier_id.len(), self.supplier_id, self.supplier_sku_code.trim())
    }
    /// 批量创建固定一个供应商。
    fn supplier(&self) -> Option<&str> {
        Some(&self.supplier_id)
    }
    /// 复用原新增命令键。
    fn key(&mut self) -> &mut String {
        &mut self.idempotency_key
    }
    /// 使用真实创建预检，包括授权、身份和资质。
    async fn prepare(&self, process: &SupplierOfferingProcess, actor: &AuditActor) -> Result<Option<Value>> {
        replay(
            process
                .domain()
                .prepare_create(self, actor, process.qualification.as_ref(), &mut NoTransaction)
                .await?,
        )
    }
    /// 每行独立创建供给、首版条款、可供状态与审计。
    async fn execute(self, process: &SupplierOfferingProcess, actor: &AuditActor) -> Result<Value> {
        result_value(process.create(self, actor).await?)
    }
}

#[async_trait]
impl BatchOperation for Targeted<ReviseSupplierOfferingRequest> {
    /// 同批一个供给只追加一次修订。
    fn identity(&self) -> String {
        self.offering_id.clone()
    }
    /// 复用原修订命令键。
    fn key(&mut self) -> &mut String {
        &mut self.command.idempotency_key
    }
    /// 使用真实修订预检，包括授权、资格和期望修订号。
    async fn prepare(&self, process: &SupplierOfferingProcess, actor: &AuditActor) -> Result<Option<Value>> {
        replay(
            process
                .domain()
                .prepare_revise(
                    &self.offering_id,
                    &self.command,
                    actor,
                    process.qualification.as_ref(),
                    &mut NoTransaction,
                )
                .await?,
        )
    }
    /// 追加条款，保留当前实时可供数量。
    async fn execute(self, process: &SupplierOfferingProcess, actor: &AuditActor) -> Result<Value> {
        result_value(process.revise(&self.offering_id, self.command, actor).await?)
    }
}

#[async_trait]
impl BatchOperation for Targeted<UpdateSupplierOfferingAvailabilityRequest> {
    /// 同批一个供给只更新一次可供状态。
    fn identity(&self) -> String {
        self.offering_id.clone()
    }
    /// 复用原可供命令键。
    fn key(&mut self) -> &mut String {
        &mut self.command.idempotency_key
    }
    /// 可供批量更新必须提供列表读取时的版本。
    async fn prepare(&self, process: &SupplierOfferingProcess, actor: &AuditActor) -> Result<Option<Value>> {
        if self.command.expected_version.is_none() {
            return Err(Error::ValidationError("缺少可供状态版本，请刷新供给列表".into()));
        }
        replay(
            process
                .domain()
                .prepare_availability(&self.offering_id, &self.command, actor, &mut NoTransaction)
                .await?,
        )
    }
    /// 只更新可供状态与数量，不创建商业条款修订。
    async fn execute(self, process: &SupplierOfferingProcess, actor: &AuditActor) -> Result<Value> {
        result_value(process.update_availability(&self.offering_id, self.command, actor).await?)
    }
}
