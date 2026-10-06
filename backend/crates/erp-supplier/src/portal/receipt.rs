use application_core::{AuditActor, CommandReceipt, StructuredCommandReceipt, StructuredReceiptMatch};
use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use super::{CooperationApplication, CooperationResult, CooperationStatus};
use crate::{Error, Result};

/// 成功命令回执：规范命令事实与原始返回结果同事务存储。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Entity)]
pub struct CooperationReceipt {
    #[serde(flatten)]
    pub base: BaseModel,
    pub command: StructuredCommandReceipt,
    pub supplier_id: String,
    pub application_id: String,
    pub application_version: u64,
    pub status: CooperationStatus,
    pub result: Option<CooperationResult>,
}

/// 为合作条款动作构造带供应商范围的稳定命令身份。
///
/// # 参数
/// * `actor` - 本次真实操作人。
/// * `supplier_id` - 服务器授权供应商范围。
/// * `application_id` - 目标申请。
/// * `action` - 稳定动作代码。
/// * `key` - 客户端原幂等键。
/// * `payload` - 允许列表请求。
/// # 返回
/// 返回可用于所有重试的规范命令。
/// # 错误
/// 范围、幂等键或序列化不合法时拒绝。
pub fn cooperation_command<T: Serialize>(
    actor: &AuditActor,
    supplier_id: &str,
    application_id: &str,
    action: &str,
    key: &str,
    payload: &T,
) -> Result<CommandReceipt> {
    if supplier_id.trim().is_empty() {
        return Err(Error::Forbidden("供应商绑定无效".into()));
    }
    let canonical = CommandReceipt::from_payload(
        "sp-cooperation-",
        actor.id(),
        action,
        "supplier_cooperation_application",
        key,
        payload,
    )?;
    Ok(CommandReceipt::from_resource_parts(
        "sp-cooperation-",
        actor.id(),
        action,
        "supplier_cooperation_application",
        application_id,
        key,
        [supplier_id.to_string(), action.to_string(), canonical.fingerprint().as_str().to_string()],
    )?)
}

impl CooperationReceipt {
    /// 从业务动作实际成功结果构造独立回执。
    ///
    /// # 参数
    /// * `command` - 当前规范命令。
    /// * `application` - 同事务最终申请。
    /// # 返回
    /// 返回成功回执。
    /// # 错误
    /// 命令范围与申请不一致时拒绝。
    pub fn new(command: &CommandReceipt, application: &CooperationApplication) -> Result<Self> {
        if command.scope_id() != Some(application.base.id.as_str()) {
            return Err(Error::ValidationError("合作条款命令目标不一致".into()));
        }
        Ok(Self {
            base: BaseModel::new(command.id().into()),
            command: StructuredCommandReceipt::from_command(command)?,
            supplier_id: application.supplier_id.clone(),
            application_id: application.base.id.clone(),
            application_version: application.base.version,
            status: application.status,
            result: application.result.clone(),
        })
    }

    /// 重试只恢复同一供应商、同一原命令的实际结果。
    ///
    /// # 参数
    /// * `command` - 本次重试的规范命令。
    /// * `supplier_id` - 当前授权范围。
    /// # 返回
    /// 匹配时返回原回执。
    /// # 错误
    /// 范围、命令身份、载荷或持久化事实不合法时拒绝。
    pub fn ensure_replayable(&self, command: &CommandReceipt, supplier_id: &str) -> Result<()> {
        if self.supplier_id != supplier_id
            || self.base.id != command.id()
            || self.application_id != command.scope_id().unwrap_or_default()
            || self.application_version == 0
            || self.base.is_deleted()
            || (self.result.is_some() != (self.status == CooperationStatus::Effective))
            || command.match_structured(&self.command) != StructuredReceiptMatch::SamePayload
        {
            return Err(Error::ConflictError("幂等键已用于不同合作条款命令".into()));
        }
        Ok(())
    }
}
