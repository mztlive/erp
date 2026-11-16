//! 仅测试支持 feature 提供的拒绝式设施与仓储凭证夹具；不得用于应用装配。
mod query;

use std::sync::Arc;

pub use query::compiled_access;

use crate::AuthorizationService;
pub use crate::service::authorization::fixture::authorization;

/// 构造始终拒绝取证的测试授权服务。
/// # 参数
/// 无。
/// # 返回
/// 用于验证输入拒绝和身份故障先于业务读取；全部查询具有不可执行的静态模型描述。
/// # 错误
/// 有效查询进入身份设施后返回测试设施不可用错误；映射转换始终拒绝，不连接数据库。
pub fn unavailable_authorization() -> Arc<AuthorizationService> {
    AuthorizationService::unavailable_for_test()
}

/// 构造缺少查询绑定的测试授权服务。
/// # 参数
/// 无。
/// # 返回
/// 没有Facts和Mapping的测试实例，用于验证查询绑定缺失的拒绝顺序。
/// # 错误
/// 有效查询在身份取证前返回缺少Mapping错误；其他取证返回测试设施不可用错误，不连接数据库。
/// 此入口只在test-support feature提供，禁止用于生产装配。
pub fn unbound_authorization() -> Arc<AuthorizationService> {
    AuthorizationService::unbound_for_test()
}
