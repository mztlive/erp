//! 仅测试支持 feature 提供的内存设施；不得用于应用装配。
use std::sync::Arc;

use crate::entity::access_control::ResolvedScope;
use crate::entity::authorization::ObjectFacts;
use crate::service::authorization::fixed;
use crate::{AccessMapping, AuthorizationService, Permission, Result};

/// 从固定的同角色权限、人员范围和对象事实构造真实授权服务，供测试在内存中验证生产规则。查询使用显式传入的映射；模型、资源或能力不符时构造失败，未绑定查询保持拒绝。
pub fn fixed_authorization(
    permissions: &[Permission],
    scope: ResolvedScope,
    objects: Vec<ObjectFacts>,
    mappings: Vec<(Permission, Arc<dyn AccessMapping>)>,
) -> Result<Arc<AuthorizationService>> {
    fixed::authorization(permissions, scope, objects, mappings)
}

/// 提供静态查询模型和始终取证失败的身份设施，供测试验证参数、身份错误先于业务读取。该服务无需数据库，映射转换也会拒绝执行。
pub fn unavailable_authorization() -> Arc<AuthorizationService> {
    AuthorizationService::unavailable_for_test()
}

/// 创建缺少事实端口和查询映射的服务，用于验证合法查询在身份取证前报告绑定缺失。其余取证会报告测试设施不可用；此入口仅供 `test-support` 使用。
pub fn unbound_authorization() -> Arc<AuthorizationService> {
    AuthorizationService::unbound_for_test()
}
