//! 仅测试支持 feature 提供的内存设施；不得用于应用装配。
use std::sync::Arc;

use crate::entity::access_control::ResolvedScope;
use crate::entity::authorization::ObjectFacts;
use crate::service::authorization::fixed;
use crate::{AccessMapping, AuthorizationService, Permission, Result};

/// 用固定身份、范围和对象事实执行真实授权服务。
/// # 参数
/// permissions 为同一启用角色授予的具体权限；scope、objects 为固定内存事实。
/// mappings 为本次查询使用的实际 Mapping；未登记的查询保持拒绝。
/// # 返回
/// 不连接数据库、不替换授权规则的测试服务；仅声明的绑定可供测试使用。
/// # 错误
/// 查询 Mapping 的模型、资源或固定规则能力不匹配时拒绝构造。
pub fn fixed_authorization(
    permissions: &[Permission],
    scope: ResolvedScope,
    objects: Vec<ObjectFacts>,
    mappings: Vec<(Permission, Arc<dyn AccessMapping>)>,
) -> Result<Arc<AuthorizationService>> {
    fixed::authorization(permissions, scope, objects, mappings)
}

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
