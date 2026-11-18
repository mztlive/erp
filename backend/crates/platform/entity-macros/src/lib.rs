use proc_macro::TokenStream;
use quote::__private::TokenStream as TokenStream2;
use quote::quote;
use syn::{Data, DeriveInput, Error, Fields, Ident, Result, parse_macro_input};

/// 为嵌入 `base: entity_core::BaseModel` 的业务实体派生 `HasBaseModel`，让仓储统一访问元数据。
/// 只能用于含 `base` 字段的具名结构体；无法解析或形态不符时在编译阶段报错。
/// 调用方仍需声明 `entity-core` 依赖并自行配置 `serde(flatten)` 等序列化行为。
#[proc_macro_derive(Entity)]
pub fn derive_entity(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand_derive_entity(&input) {
        Ok(expanded) => expanded.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

// 派生入口解析后调用；先确认输入是含 base 字段的具名结构体，再保留原泛型和约束生成
// HasBaseModel 实现。形态不符返回 syn 错误，字段类型由展开后的 Rust 类型检查确认。
fn expand_derive_entity(input: &DeriveInput) -> Result<TokenStream2> {
    let name = &input.ident;
    let Data::Struct(data) = &input.data else {
        return Err(Error::new_spanned(name, "Entity 派生仅支持结构体"));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(Error::new_spanned(name, "Entity 派生要求具名结构体并含有 base 字段"));
    };
    if !fields.named.iter().any(|field| field.ident.as_ref().is_some_and(|id| id == "base")) {
        return Err(Error::new_spanned(name, "Entity 派生要求含有 base 字段"));
    }

    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    Ok(quote! {
        impl #impl_generics ::entity_core::HasBaseModel for #name #ty_generics #where_clause {
            /// 返回实体持久化元数据。
            ///
            /// # 参数
            /// 无。
            ///
            /// # 返回
            /// 返回引用，生命周期与持有者一致。
            ///
            /// # 错误
            /// 不返回错误。
            fn base(&self) -> &::entity_core::BaseModel {
                &self.base
            }

            /// 返回实体持久化元数据的可变引用。
            ///
            /// # 参数
            /// 无。
            ///
            /// # 返回
            /// 返回可变引用，生命周期与持有者一致。
            ///
            /// # 错误
            /// 不返回错误。
            fn base_mut(&mut self) -> &mut ::entity_core::BaseModel {
                &mut self.base
            }
        }
    })
}

/// 用单个类型名（如 `SalesOrderId`）声明独立的字符串 ID 类型，防止不同实体的主键在编译时混用。
/// 展开结果提供 `new`、`Deref<Target = str>`、`AsRef<str>`、`From<String>`、`Display` 及透明的
/// 字符串序列化；消费方需启用 serde 派生。调用方负责生成并传入 ID，宏不校验值的格式。
/// 输入不是单个标识符时在编译阶段报错。
#[proc_macro]
pub fn id_type(input: TokenStream) -> TokenStream {
    let name = parse_macro_input!(input as Ident);
    expand_id_type(&name).into()
}

// id_type! 入口解析出类型名后调用，生成透明字符串包装及常用访问与转换实现，
// 使不同 ID 在 Rust 类型层面隔离而不改变持久化形态。
fn expand_id_type(name: &Ident) -> TokenStream2 {
    quote! {
        /// 主键 ID（透明字符串值对象；由调用方生成并传入，不承载业务含义）。
        #[derive(Debug, Clone, PartialEq, Eq, Hash, ::serde::Serialize, ::serde::Deserialize)]
        #[serde(transparent, crate = "::serde")]
        pub struct #name(::std::string::String);

        impl #name {
            /// 由已生成的主键值构造 ID。
            ///
            /// # 参数
            /// * `value` - 调用方生成的主键值。
            ///
            /// # 返回
            /// 返回新的 ID。ID 是透明值对象，不校验格式。
            ///
            /// # 错误
            /// 不返回错误。
            pub fn new(value: impl ::std::convert::Into<::std::string::String>) -> Self {
                Self(value.into())
            }
        }

        impl ::std::ops::Deref for #name {
            type Target = str;

            fn deref(&self) -> &str {
                &self.0
            }
        }

        impl ::std::convert::AsRef<str> for #name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl ::std::convert::From<::std::string::String> for #name {
            fn from(value: ::std::string::String) -> Self {
                Self::new(value)
            }
        }

        impl ::std::fmt::Display for #name {
            fn fmt(&self, formatter: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                formatter.write_str(&self.0)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use syn::{DeriveInput, Ident};

    use super::{expand_derive_entity, expand_id_type};

    fn parse_derive(input: &str) -> DeriveInput {
        syn::parse_str(input).expect("测试输入必须可解析")
    }

    fn expand_to_string(input: &str) -> String {
        expand_derive_entity(&parse_derive(input)).expect("合法输入必须展开").to_string()
    }

    #[test]
    fn derive_expands_for_named_struct_with_base() {
        let expanded = expand_to_string("struct Foo { base: ::entity_core::BaseModel }");

        assert!(expanded.contains("HasBaseModel"));
        assert!(expanded.contains("entity_core"));
    }

    #[test]
    fn derive_rejects_enum_and_missing_base() {
        let input = parse_derive("enum Foo { A, B }");
        assert!(expand_derive_entity(&input).is_err());

        let input = parse_derive("struct Foo { other: u8 }");
        assert!(expand_derive_entity(&input).is_err());
    }

    #[test]
    fn derive_keeps_generics_in_impl() {
        let expanded = expand_to_string("struct Foo<T> { base: ::entity_core::BaseModel, value: T }");
        assert!(expanded.contains("Foo"));
    }

    #[test]
    fn id_type_uses_transparent_string_and_single_construction() {
        let name: Ident = syn::parse_str("ExampleId").expect("标识符必须可解析");
        let expanded = expand_id_type(&name).to_string();

        assert!(expanded.contains("ExampleId"));
        assert!(expanded.contains("serde (transparent"));
        assert!(expanded.contains("Self :: new"));
    }
}
