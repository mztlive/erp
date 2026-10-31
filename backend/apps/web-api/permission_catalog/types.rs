//! 权限目录的静态值类型，不依赖 HTTP、数据库或权限执行器。

/// 权限展示分组。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PermissionGroupDef {
    pub name: &'static str,
    pub description: &'static str,
}

/// 接口引用的权限及其展示信息；多个接口可以共享同一代码。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PermissionDef {
    pub code: &'static str,
    pub group: PermissionGroupDef,
    pub description: &'static str,
}

impl PermissionDef {
    /// 复用权限代码，为另一个接口保留独立的展示说明。
    /// # 参数
    /// `group` 与 `description` 为引用场景的原有展示信息。
    /// # 返回
    /// 权限代码不变的定义。
    /// # 错误
    /// 无。
    pub const fn described(self, group: PermissionGroupDef, description: &'static str) -> Self {
        Self { group, description, ..self }
    }
}

/// 显式模块索引；符号名仅用于构建阶段解析路由引用。
#[derive(Debug, Clone, Copy)]
pub struct PermissionModule {
    pub name: &'static str,
    pub entries: &'static [(&'static str, PermissionDef)],
}

/// 路由入口所需权限与目录展示项；二者默认相同。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PermissionBinding {
    pub required: PermissionDef,
    pub catalog: PermissionDef,
}

impl PermissionDef {
    /// 为入口校验与动作目录分离的既有接口显式指定展示项。
    /// # 参数
    /// `catalog` 为目录展示的业务动作，当前定义仍决定入口鉴权。
    /// # 返回
    /// 同时保留入口权限和目录权限的绑定。
    /// # 错误
    /// 无。
    pub const fn documented_as(self, catalog: Self) -> PermissionBinding {
        PermissionBinding { required: self, catalog }
    }
}

impl From<PermissionDef> for PermissionBinding {
    fn from(definition: PermissionDef) -> Self {
        Self { required: definition, catalog: definition }
    }
}
