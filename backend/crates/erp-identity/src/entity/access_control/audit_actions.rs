//! audit_events 的显式事件目录。新增写入事件必须先登记稳定代码与中文标签。

/// 当前审计事件写入入口的完整目录；不包含其他集合的操作日志或权限代码。
pub const AUDIT_ACTIONS: &[(&str, &str)] = &[
    ("permission.create", "权限定义 · 新建"),
    ("permission.update", "权限定义 · 修改"),
    ("permission.delete", "权限定义 · 删除"),
    ("data_scope.create", "数据范围 · 新建"),
    ("data_scope.delete", "数据范围 · 删除"),
    ("user_role.assign", "用户角色 · 授权"),
    ("user_role.revoke", "用户角色 · 撤权"),
    ("person_query_qualification.change", "人员查询资格 · 维护"),
];
