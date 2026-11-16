//! 客户资料的授权跨域详情投影。
mod display;
mod query;
mod sensitive;
mod views;
use std::sync::Arc;

use erp_customer::CustomerAccess;
use erp_identity::AuthorizationService;
use erp_party::SensitiveDataCodec;
use mongodb::Database;
/// 在同一授权快照组合客户、主体与账号名称的只读服务。
#[derive(Clone)]
pub struct CustomerProfileReadService {
    db: Database,
    sensitive_data: Arc<SensitiveDataCodec>,
    access: CustomerAccess,
    authorization: Arc<AuthorizationService>,
}
impl CustomerProfileReadService {
    /// 绑定客户详情读取所需的共享设施。
    /// # 参数
    /// db 为业务库；sensitive_data 为令牌编码器；access 与 authorization 来自同次授权装配。
    /// # 返回
    /// 不缓存授权结果的客户资料读模型。
    /// # 错误
    /// 无。
    pub fn new(
        db: Database,
        sensitive_data: Arc<SensitiveDataCodec>,
        access: CustomerAccess,
        authorization: Arc<AuthorizationService>,
    ) -> Self {
        Self { db, sensitive_data, access, authorization }
    }
}
