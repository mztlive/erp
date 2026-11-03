//! 运行时复用构建阶段的显式权限定义。
use erp_identity::Permission;

#[path = "../permission_catalog/mod.rs"]
mod definitions;
pub use definitions::*;

impl PermissionDef {
    /// 将静态目录代码转换为领域权限值。
    /// # 参数
    /// 无。
    /// # 返回
    /// 与既有权限代码完全相同的领域权限。
    /// # 错误
    /// 不返回错误；目录在构建阶段检查格式。
    /// # Panics
    /// 静态目录代码被领域解析器拒绝时报告编程错误。
    pub fn permission(self) -> Permission {
        Permission::parse(self.code).expect("显式权限目录必须使用合法的权限代码")
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::permissions;

    include!(concat!(env!("OUT_DIR"), "/permission_references.rs"));

    #[test]
    fn catalog_index_names_resolve_to_the_exact_compiled_constants() {
        let actual = MODULES
            .iter()
            .flat_map(|module| {
                module
                    .entries
                    .iter()
                    .map(move |(name, definition)| (format!("{}::{name}", module.name), *definition))
            })
            .collect::<HashMap<_, _>>();
        assert_eq!(actual.len(), COMPILED_DEFINITIONS.len());
        for (symbol, definition) in COMPILED_DEFINITIONS {
            assert_eq!(actual.get(*symbol), Some(definition), "{symbol}");
        }
    }

    #[test]
    fn command_catalog_entry_does_not_replace_existing_detail_guard() {
        let binding = supplier_api::CONNECTION_DETAIL.documented_as(supplier_api::CONNECTION_COMMAND);
        assert_eq!(binding.required.permission().to_string(), "supplier_api_connection:detail");
        assert_eq!(binding.catalog.permission().to_string(), "supplier_api_connection:command");
        let ordinary = PermissionBinding::from(purchase_order::SUBMIT);
        assert_eq!(ordinary.required, ordinary.catalog);
    }

    #[test]
    fn every_registered_definition_uses_the_domain_permission_parser() {
        for module in MODULES {
            for (_, definition) in module.entries {
                assert_eq!(definition.permission().to_string(), definition.code);
            }
        }
    }

    #[test]
    fn alternate_presentations_preserve_authorization_identity() {
        let alternate = purchase_order::SUBMIT.described(groups::CUSTOMER, "另一个接口说明");
        assert_eq!(alternate.permission(), purchase_order::SUBMIT.permission());
        assert_eq!(alternate.description, "另一个接口说明");
        assert_eq!(alternate.group, groups::CUSTOMER);
    }
}
