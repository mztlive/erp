use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

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
