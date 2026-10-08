//! 连接本域构造和写入；供应商存在性由外层先验证。
use id_generator::next_id;
use persistence_core::Executor;

use super::SupplierApiService;
use crate::Result;
use crate::dto::supplier_api::*;
use crate::entity::supplier_api::*;
use crate::repository::SupplierApiExt;
impl SupplierApiService {
    /// 在原供应商存在性检查之后生成 ID 并构造身份连接。
    ///
    /// # 参数
    /// * `req` - 创建连接请求。
    /// * `prepared` - 已准备的创建状态。
    /// * `actor_id` - 创建人。
    ///
    /// # 返回
    /// 返回尚未持久化的连接实体。
    ///
    /// # 错误
    /// 限流策略转换或连接实体构造失败时返回对应错误。
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
    ///
    /// # 参数
    /// * `connection` - 待创建的连接。
    /// * `executor` - 调用方事务的执行器。
    ///
    /// # 返回
    /// 无返回值。连接与空能力集合已写入。
    ///
    /// # 错误
    /// 仓储写入失败时返回对应错误。
    pub async fn persist_created_connection(
        &self,
        connection: &SupplierApiConnection,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db.supplier_api().create_connection_with_capabilities(connection, &[], executor).await?;
        Ok(())
    }
    /// 在调用方事务中推进连接 CAS。
    ///
    /// # 参数
    /// * `connection` - 待写回的连接；成功后版本由仓储推进。
    /// * `executor` - 调用方事务的执行器。
    ///
    /// # 返回
    /// 无返回值。连接已按 CAS 写回。
    ///
    /// # 错误
    /// 仓储更新失败时返回对应错误。
    pub async fn persist_connection(
        &self,
        connection: &mut SupplierApiConnection,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db.supplier_api_connections().update(connection, executor).await?;
        Ok(())
    }
    /// 在调用方事务中推进健康运行记录 CAS。
    ///
    /// # 参数
    /// * `run` - 待写回的健康运行记录。
    /// * `executor` - 调用方事务的执行器。
    ///
    /// # 返回
    /// 无返回值。健康运行记录已按 CAS 写回。
    ///
    /// # 错误
    /// 仓储更新失败时返回对应错误。
    pub async fn persist_health_run(
        &self,
        run: &mut SupplierHealthCheckRun,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db.supplier_api_health_check_runs().update(run, executor).await?;
        Ok(())
    }
}
