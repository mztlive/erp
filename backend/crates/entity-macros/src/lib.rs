use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Ident};

/// 派生 Entity 宏实现。
///
/// # 参数
/// * `input` - 输入数据
///
/// # 返回
/// 返回 `TokenStream` 实例。
#[proc_macro_derive(Entity)]
pub fn derive_entity(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    quote! {
        impl entity_core::HasBaseModel for #name {
            /// 返回实体持久化元数据。
            ///
            /// # 返回
            /// 返回引用，生命周期与持有者一致。
            fn base(&self) -> &entity_core::BaseModel {
                &self.base
            }

            /// 返回实体持久化元数据的可变引用。
            ///
            /// # 返回
            /// 返回可变引用，生命周期与持有者一致。
            fn base_mut(&mut self) -> &mut entity_core::BaseModel {
                &mut self.base
            }
        }
    }
    .into()
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
#[proc_macro]
pub fn id_type(input: TokenStream) -> TokenStream {
    let name = parse_macro_input!(input as Ident);

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
                Self(value)
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
    .into()
}
