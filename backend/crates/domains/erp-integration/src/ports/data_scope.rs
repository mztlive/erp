//! 集成授权端口只传递 identity 生成的查询凭证和完整对象目标。
use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_identity::CompiledAccess;
use persistence_core::Executor;

use crate::{Error, Result};

/// 集成授权及组织事实端口；不公开可由业务解释的权限条款。
#[async_trait]
pub trait IntegrationDataScopePort: Send + Sync {
    /// 取得 identity 编译的本次读取凭证。
    /// # 参数
    /// actor、resource、action 为真实身份和服务端入口，executor 为原执行器。
    /// # 返回
    /// 完整授权聚合计划。
    /// # 错误
    /// 未装配、无权或版本变化拒绝。
    async fn resolve(
        &self,
        _actor: &AuditActor,
        _resource: &str,
        _action: &str,
        _executor: &mut dyn Executor,
    ) -> Result<CompiledAccess> {
        Err(unwired())
    }

    /// 在原执行器复验查询凭证。
    /// # 参数
    /// access 为本次查询凭证，executor 为原执行器。
    /// # 返回
    /// 身份及版本仍有效时成功。
    /// # 错误
    /// 未装配或授权发生变化时拒绝。
    async fn validate(&self, _access: &CompiledAccess, _executor: &mut dyn Executor) -> Result<()> {
        Err(unwired())
    }

    /// 授权指定的完整集成对象。
    /// # 参数
    /// actor、resource、action、id 为当前入口与实际记录身份，executor 为原执行器。
    /// # 返回
    /// identity 完成对象授权后成功。
    /// # 错误
    /// 未装配、对象缺失或无权限时拒绝。
    async fn require(
        &self,
        _actor: &AuditActor,
        _resource: &str,
        _action: &str,
        _id: &str,
        _executor: &mut dyn Executor,
    ) -> Result<()> {
        Err(unwired())
    }

    /// 展开业务筛选中明确指定的有效组织。
    /// # 参数
    /// org_unit_ids、include_descendants 为筛选，executor 为原执行器。
    /// # 返回
    /// 有效组织集合。
    /// # 错误
    /// 未装配、组织不存在或读取失败时拒绝。
    async fn expand_org_units(
        &self,
        _org_unit_ids: &[String],
        _include_descendants: bool,
        _executor: &mut dyn Executor,
    ) -> Result<BTreeSet<String>> {
        Err(unwired())
    }

    /// 读取时点内组织的主属成员。
    /// # 参数
    /// org_unit_ids、at 为实际业务筛选和时点，executor 为原执行器。
    /// # 返回
    /// 有效成员身份。
    /// # 错误
    /// 未装配或读取失败时拒绝。
    async fn org_member_ids(
        &self,
        _org_unit_ids: &BTreeSet<String>,
        _at: Instant,
        _executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        Err(unwired())
    }

    /// 读取处理人的唯一有效主属组织。
    /// # 参数
    /// user_id、at 为实际人员和时点，executor 为原执行器。
    /// # 返回
    /// 有效主属组织或无主属关系。
    /// # 错误
    /// 未装配、主属关系冲突或读取失败时拒绝。
    async fn own_org(
        &self,
        _user_id: &str,
        _at: Instant,
        _executor: &mut dyn Executor,
    ) -> Result<Option<String>> {
        Err(unwired())
    }
}

/// 未装配时所有访问明确失败。
pub struct FailClosedIntegrationDataScopePort;
impl FailClosedIntegrationDataScopePort {
    /// 创建失败关闭端口。
    /// # 参数
    /// 无。
    /// # 返回
    /// 共享端口。
    /// # 错误
    /// 不执行 I/O。
    pub fn shared() -> Arc<dyn IntegrationDataScopePort> {
        Arc::new(Self)
    }
}
#[async_trait]
impl IntegrationDataScopePort for FailClosedIntegrationDataScopePort {}
fn unwired() -> Error {
    Error::Internal("集成授权端口未接线".into())
}
