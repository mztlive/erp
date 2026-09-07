//! 连接本域构造和写入；供应商存在性由外层先验证。
use super::SupplierApiService;
use crate::repository::SupplierApiExt;
use crate::{dto::supplier_api::*, entity::supplier_api::*, Result};
use id_generator::next_id;
use persistence_core::Executor;
impl SupplierApiService {
    /// 在原供应商存在性检查之后生成 ID 并构造身份连接。
    pub fn prepare_connection(
        req: CreateSupplierApiConnectionRequest,
        prepared: PreparedSupplierConnectionCreate,
        actor_id: &str,
    ) -> Result<SupplierApiConnection> {
        let id = erp_core::ids::SupplierApiConnectionId::new(next_id());
        let connection = SupplierApiConnection::new(
            id,
            SupplierApiConnectionData {
                supplier_id: req.supplier_id,
                connection_code: req.connection_code,
                environment: req.environment,
                endpoint_reference: String::new(),
                credential_reference: None,
                rate_limit_policy: req
                    .rate_limit_policy
                    .map(RateLimitPolicyRequest::into_policy)
                    .transpose()?,
                status: prepared.status(),
            },
            actor_id,
        )?;
        Ok(connection)
    }
    /// 在调用方事务中创建连接及能力集合，不构造审计或开启事务。
    pub async fn persist_created_connection(
        &self,
        connection: &SupplierApiConnection,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db
            .supplier_api()
            .create_connection_with_capabilities(connection, &[], executor)
            .await?;
        Ok(())
    }
    /// 在调用方事务中推进连接 CAS。
    pub async fn persist_connection(
        &self,
        connection: &mut SupplierApiConnection,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db
            .supplier_api_connections()
            .update(connection, executor)
            .await?;
        Ok(())
    }
    /// 在调用方事务中推进健康运行记录 CAS。
    pub async fn persist_health_run(
        &self,
        run: &mut SupplierHealthCheckRun,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db
            .supplier_api_health_check_runs()
            .update(run, executor)
            .await?;
        Ok(())
    }
}
