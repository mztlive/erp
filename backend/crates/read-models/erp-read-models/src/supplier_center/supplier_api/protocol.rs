//! 供应商协议读取和技术票据操作的固定连接目标检查。
use application_core::AuditActor;
use erp_identity::AccessTarget;
use erp_permissions::{Permission, internal};
use erp_supply::entity::supplier_api::SupplierApiConnection;
use persistence_core::{Executor, Transactional};

use super::SupplierApiReadService;
use crate::Result;

#[derive(Clone, Copy)]
enum ProtocolAccess {
    Read,
    ReferenceTickets,
}

impl ProtocolAccess {
    // 检查供应商协议读取或技术票据操作前，用服务端操作类型选择登记权限，不能由调用方传入权限常量。
    fn permission(self) -> Permission {
        match self {
            Self::Read => internal::CONNECTION_PROTOCOL_READ,
            Self::ReferenceTickets => internal::CONNECTION_REFERENCE_TICKETS,
        }
    }
}

impl SupplierApiReadService {
    /// 处理供应商协议时，授权并读取协议目录操作所需的真实连接。
    ///
    /// # 参数
    /// id 为连接身份，actor 为当前已认证账号。
    ///
    /// # 返回
    /// 在同一快照中通过 protocol_read、连接详情及供应商资格检查的连接。
    ///
    /// # 错误
    /// 缺少资格、连接或来源不存在、事实不完整或数据库读取失败时拒绝。
    pub async fn protocol_connection(&self, id: &str, actor: &AuditActor) -> Result<SupplierApiConnection> {
        self.connection_for_protocol(id, actor, ProtocolAccess::Read).await
    }

    /// 处理供应商协议时，授权并读取技术绑定票据操作所需的真实连接。
    ///
    /// # 参数
    /// id 为连接身份，actor 为当前已认证账号。
    ///
    /// # 返回
    /// 通过凭证引用管理、连接详情及供应商资格检查的连接。
    ///
    /// # 错误
    /// 缺少资格、连接或来源不存在、事实不完整或数据库读取失败时拒绝。
    pub async fn reference_ticket_connection(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<SupplierApiConnection> {
        self.connection_for_protocol(id, actor, ProtocolAccess::ReferenceTickets).await
    }

    // 处理供应商协议时，连接事实和协议用连接数据在一个快照内读取，外部供应商调用由调用方执行。
    async fn connection_for_protocol(
        &self,
        id: &str,
        actor: &AuditActor,
        operation: ProtocolAccess,
    ) -> Result<SupplierApiConnection> {
        let service = self.clone();
        let id = id.to_owned();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.protocol_snapshot(&id, &actor, operation, executor).await })
            })
            .await
    }

    // 处理供应商协议时，入口资格与真实连接、完整来源由 identity 的单一固定入口取证。
    async fn protocol_snapshot(
        &self,
        id: &str,
        actor: &AuditActor,
        operation: ProtocolAccess,
        executor: &mut dyn Executor,
    ) -> Result<SupplierApiConnection> {
        self.authorization
            .require(actor, &operation.permission(), AccessTarget::record(id)?, executor)
            .await?;
        Ok(self.domain().load_connection(id, executor).await?)
    }
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_identity::entity::access_control::ResolvedScope;
    use erp_identity::testing::{fixed_authorization, unavailable_authorization};
    use erp_permissions::{supplier, supplier_api};
    use mongodb::Client;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::Error;

    async fn service(permissions: &[Permission]) -> SupplierApiReadService {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("protocol_authorization_unit");
        let authorization = fixed_authorization(
            permissions,
            ResolvedScope { role_clauses: vec![], user_limit: None },
            vec![],
            vec![],
        )
        .unwrap();
        SupplierApiReadService::new(db, authorization)
    }

    fn actor() -> AuditActor {
        AuditActor::new("user".into(), "login".into(), AccountKind::Admin)
    }

    #[tokio::test]
    async fn connection_detail_alone_does_not_grant_protocol_read_or_credential_tickets() {
        let service = service(&[supplier_api::CONNECTION_DETAIL, supplier::DETAIL]).await;
        for operation in [ProtocolAccess::Read, ProtocolAccess::ReferenceTickets] {
            let error = service
                .protocol_snapshot("connection", &actor(), operation, &mut SnapshotExecutor::default())
                .await
                .unwrap_err();
            assert!(matches!(error, Error::Forbidden(_)));
        }
    }

    #[tokio::test]
    async fn protocol_identity_failure_stops_before_connection_storage() {
        let mut service = service(&[]).await;
        service.authorization = unavailable_authorization();
        let error = service
            .protocol_snapshot("connection", &actor(), ProtocolAccess::Read, &mut SnapshotExecutor::default())
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Internal(reason) if reason == "测试授权设施不可用"));
    }
}
