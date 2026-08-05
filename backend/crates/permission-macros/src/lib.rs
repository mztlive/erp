use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, punctuated::Punctuated, Expr, ItemFn, Lit, LitStr, MetaNameValue, Token};

/// Permission macro arguments.
///
/// This structure stores the required permission key components.
#[derive(Debug)]
struct PermissionArgs {
    resource: String,
    action: String,
}

/// 权限标注宏（编译期生成权限键函数）。
///
/// # 参数
/// * `attr` - 宏属性参数（支持 `group`、`group_desc`、`desc`、`resource`、`action`）
/// * `item` - 函数项
///
/// # 返回
/// 返回 `TokenStream` 实例。
#[proc_macro_attribute]
pub fn permission(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args_parser = Punctuated::<MetaNameValue, Token![,]>::parse_terminated;
    let args = parse_macro_input!(attr with args_parser);
    let item_fn = parse_macro_input!(item as ItemFn);

    let permission_args = match parse_permission_args(args) {
        Ok(args) => args,
        Err(err) => return err.to_compile_error().into(),
    };

    let handler_name = item_fn.sig.ident.to_string();
    let permission_fn_ident = format_ident!("{}_permission_key", item_fn.sig.ident);
    let resource = LitStr::new(&permission_args.resource, proc_macro2::Span::call_site());
    let action = LitStr::new(&permission_args.action, proc_macro2::Span::call_site());

    let expanded = quote! {
        #item_fn

        /// 返回处理器对应的权限键。
        ///
        /// # 返回
        /// 返回该处理器绑定的权限键。
        pub fn #permission_fn_ident() -> entities::Permission {
            entities::Permission::parse(concat!(#resource, ":", #action))
                .expect(concat!("invalid permission key for handler ", #handler_name))
        }
    };

    expanded.into()
}

/// 解析 permission 宏参数。
///
/// # 参数
/// * `args` - 参数列表
///
/// # 返回
/// 返回解析后的参数或错误。
fn parse_permission_args(args: Punctuated<MetaNameValue, Token![,]>) -> syn::Result<PermissionArgs> {
    let mut resource = None;
    let mut action = None;

    for arg in args {
        let name = arg.path.get_ident().map(|ident| ident.to_string());
        let value = match arg.value {
            Expr::Lit(expr) => match expr.lit {
                Lit::Str(lit) => lit.value(),
                _ => continue,
            },
            _ => continue,
        };

        match name.as_deref() {
            Some("resource") => resource = Some(value),
            Some("action") => action = Some(value),
            _ => {}
        }
    }

    let resource =
        resource.ok_or_else(|| syn::Error::new(proc_macro2::Span::call_site(), "missing resource"))?;
    let action = action.ok_or_else(|| syn::Error::new(proc_macro2::Span::call_site(), "missing action"))?;

    Ok(PermissionArgs { resource, action })
}
