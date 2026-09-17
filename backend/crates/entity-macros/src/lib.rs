use proc_macro::TokenStream;
use quote::__private::TokenStream as TokenStream2;
use quote::quote;
use syn::{Data, DeriveInput, Error, Fields, Ident, Result, parse_macro_input};

/// 派生 Entity 宏实现。
///
/// # 参数
/// * `input` - 输入数据
///
/// # 返回
/// 返回 `TokenStream` 实例。
///
/// # 错误
/// 输入非具名结构体或缺 `base` 字段时编译失败。
#[proc_macro_derive(Entity)]
pub fn derive_entity(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand_derive_entity(&input) {
        Ok(expanded) => expanded.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

/// 生成 Entity 派生的 `HasBaseModel` 实现。
///
/// # 参数
/// * `input` - 已解析的派生输入
///
/// # 返回
/// 返回生成代码。
///
/// # 错误
/// 输入非具名结构体或缺 `base` 字段时返回错误。
fn expand_derive_entity(input: &DeriveInput) -> Result<TokenStream2> {
    let Data::Struct(data) = &input.data else {
        return Err(Error::new_spanned(&input.ident, "Entity 派生仅支持结构体"));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(Error::new_spanned(&input.ident, "Entity 派生要求具名结构体并含有 base 字段"));
    };
    if !fields.named.iter().any(|field| field.ident.as_ref().is_some_and(|id| id == "base")) {
        return Err(Error::new_spanned(&input.ident, "Entity 派生要求含有 base 字段"));
    }
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    Ok(quote! {
        impl #impl_generics ::entity_core::HasBaseModel for #name #ty_generics #where_clause {
            /// 返回实体持久化元数据。
            ///
            /// # 返回
            /// 返回引用，生命周期与持有者一致。
            fn base(&self) -> &::entity_core::BaseModel {
                &self.base
            }

            /// 返回实体持久化元数据的可变引用。
            ///
            /// # 返回
            /// 返回可变引用，生命周期与持有者一致。
            fn base_mut(&mut self) -> &mut ::entity_core::BaseModel {
                &mut self.base
            }
        }
    })
}

/// 生成透明主键 ID newtype。
///
/// 输入为单个标识符（如 `SalesOrderId`）。展开结果提供 `new`、`Deref<Target = str>`、
/// `AsRef<str>`、`From<String>`、`Display` 以及透明字符串的 `Serialize`/`Deserialize`。
/// ID 值由调用方生成并传入；本宏不生成主键，也不校验格式。
///
/// # 参数
/// * 输入 token - 要生成的 ID 类型名
///
/// # 返回
/// 返回类型定义与 impl 的 `TokenStream`。
///
/// # 错误
/// 输入非单个标识符时编译失败。
#[proc_macro]
pub fn id_type(input: TokenStream) -> TokenStream {
    let name = parse_macro_input!(input as Ident);
    expand_id_type(&name).into()
}

/// 生成透明字符串 ID newtype 定义。
///
/// # 参数
/// * `name` - 要生成的 ID 类型名
///
/// # 返回
/// 返回类型定义与 impl。
///
/// # 错误
/// 无；本函数不做输入校验，非法输入由宏入口拒绝。
fn expand_id_type(name: &Ident) -> TokenStream2 {
    quote! {
        /// 主键 ID（透明字符串值对象；由调用方生成并传入，不承载业务含义）。
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct #name(::std::string::String);

        impl #name {
            /// 由已生成的主键值构造 ID。
            ///
            /// # 参数
            /// * `value` - 调用方生成的主键值。
            ///
            /// # 返回
            /// 返回新的 ID。ID 是透明值对象，不校验格式。
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

        impl ::serde::Serialize for #name {
            fn serialize<S: ::serde::ser::Serializer>(
                &self,
                serializer: S,
            ) -> ::std::result::Result<S::Ok, S::Error> {
                serializer.serialize_str(&self.0)
            }
        }

        impl<'de> ::serde::Deserialize<'de> for #name {
            fn deserialize<D: ::serde::de::Deserializer<'de>>(
                deserializer: D,
            ) -> ::std::result::Result<Self, D::Error> {
                Ok(Self(::std::string::String::deserialize(deserializer)?))
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

    #[test]
    fn derive_expands_for_named_struct_with_base() {
        let input = parse_derive("struct Foo { base: ::entity_core::BaseModel }");
        let expanded = expand_derive_entity(&input).expect("合法输入必须展开").to_string();

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
        let input = parse_derive("struct Foo<T> { base: ::entity_core::BaseModel, value: T }");
        let expanded = expand_derive_entity(&input).expect("泛型输入必须展开").to_string();

        assert!(expanded.contains("Foo"));
    }

    #[test]
    fn id_type_uses_single_construction() {
        let name: Ident = syn::parse_str("ExampleId").expect("标识符必须可解析");
        let expanded = expand_id_type(&name).to_string();

        assert!(expanded.contains("ExampleId"));
        assert!(expanded.contains("Self :: new"));
    }
}
