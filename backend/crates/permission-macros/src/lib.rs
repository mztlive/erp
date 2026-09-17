use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::{format_ident, quote};
use syn::punctuated::Punctuated;
use syn::{Error, Expr, ItemFn, Lit, LitStr, MetaNameValue, Result, Token, parse_macro_input};

/// Permission macro arguments.
///
/// This structure stores the required permission key components.
struct PermissionArgs {
    resource: LitStr,
    action: LitStr,
}

/// 权限标注宏（编译期生成权限键函数）。
///
/// # 参数
/// * `attr` - 宏属性参数（支持 `group`、`group_desc`、`desc`、`resource`、`action`）
/// * `item` - 函数项
///
/// # 返回
/// 返回 `TokenStream` 实例。
///
/// # 错误
/// 属性缺 `resource`/`action` 或其非字符串字面量时编译失败。
#[proc_macro_attribute]
pub fn permission(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args_parser = Punctuated::<MetaNameValue, Token![,]>::parse_terminated;
    let args = parse_macro_input!(attr with args_parser);
    let item_fn = parse_macro_input!(item as ItemFn);

    let permission_args = match parse_permission_args(args) {
        Ok(args) => args,
        Err(err) => {
            let compile_err = err.to_compile_error();
            return quote! {
                #item_fn
                #compile_err
            }
            .into();
        },
    };
    if let Err(err) = validate_permission_key(&permission_args.resource, &permission_args.action) {
        let compile_err = err.to_compile_error();
        return quote! {
            #item_fn
            #compile_err
        }
        .into();
    }

    let handler_name = item_fn.sig.ident.to_string();
    let permission_fn_ident = format_ident!("{}_permission_key", item_fn.sig.ident);
    let resource = &permission_args.resource;
    let action = &permission_args.action;

    let expanded = quote! {
        #item_fn

        /// 返回处理器对应的权限键。
        ///
        /// # 返回
        /// 返回该处理器绑定的权限键。
        pub fn #permission_fn_ident() -> ::erp_identity::Permission {
            ::erp_identity::Permission::parse(concat!(#resource, ":", #action))
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
///
/// # 错误
/// 缺 `resource`/`action` 或其非字符串字面量时返回错误。
fn parse_permission_args(args: Punctuated<MetaNameValue, Token![,]>) -> Result<PermissionArgs> {
    let mut resource = None;
    let mut action = None;

    for arg in args {
        if arg.path.is_ident("resource") {
            let lit = require_string_value(&arg.value)?;
            resource = Some(lit);
        } else if arg.path.is_ident("action") {
            let lit = require_string_value(&arg.value)?;
            action = Some(lit);
        } else if arg.path.is_ident("group") || arg.path.is_ident("group_desc") || arg.path.is_ident("desc") {
            // 仅透传给权限收集，键生成不消费它们；保留全部接受行为。
        } else {
            // 未知键保持静默忽略，接受集合不变。
        }
    }

    let resource = resource.ok_or_else(|| Error::new(Span::call_site(), "missing resource"))?;
    let action = action.ok_or_else(|| Error::new(Span::call_site(), "missing action"))?;

    Ok(PermissionArgs { resource, action })
}

/// 提取字符串字面量，保留原文位置。
///
/// # 参数
/// * `value` - 属性值表达式
///
/// # 返回
/// 返回原文字符串字面量。
///
/// # 错误
/// 非字符串字面量时在表达式位置报错。
fn require_string_value(value: &Expr) -> Result<LitStr> {
    match value {
        Expr::Lit(expr) => match &expr.lit {
            Lit::Str(lit) => Ok(lit.clone()),
            _ => Err(Error::new_spanned(value, "resource/action 必须为字符串字面量")),
        },
        _ => Err(Error::new_spanned(value, "resource/action 必须为字符串字面量")),
    }
}

/// 在宏展开期对权限键做与运行时一致的轻量校验。
///
/// 完整归属仍在 `erp-identity` 的 `Permission::parse`，运行时 `expect` 原样保留为兜底。
///
/// # 参数
/// * `resource` - 资源字面量
/// * `action` - 动作字面量
///
/// # 返回
/// 合法时返回空。
///
/// # 错误
/// 格式非法时在对应字面量位置报错。
fn validate_permission_key(resource: &LitStr, action: &LitStr) -> Result<()> {
    validate_permission_part(&resource.value(), true)
        .map_err(|message| Error::new_spanned(resource, message))?;
    validate_permission_part(&action.value(), false)
        .map_err(|message| Error::new_spanned(action, message))?;
    Ok(())
}

/// 校验权限资源或动作单段，与运行时规则同口径。
///
/// # 参数
/// * `value` - 待校验原文
/// * `allow_slash` - 资源允许斜杠分段，动作不允许
///
/// # 返回
/// 合法时返回空。
///
/// # 错误
/// 非法时返回面向宏展开期的说明文本。
fn validate_permission_part(value: &str, allow_slash: bool) -> std::result::Result<(), String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Err("权限资源或动作不能为空".to_string());
    }
    if normalized.len() > 128 {
        return Err("权限资源或动作长度不能超过128个字符".to_string());
    }
    if normalized == "*" {
        return Ok(());
    }
    if !allow_slash && normalized.contains('/') {
        return Err("权限动作不能包含斜杠".to_string());
    }
    let valid = normalized.split('/').all(|segment| {
        !segment.is_empty()
            && segment
                .chars()
                .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '_' | '-'))
    });
    if !valid {
        return Err("权限资源或动作包含非法字符".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use syn::parse::Parser;
    use syn::punctuated::Punctuated;
    use syn::{MetaNameValue, Token};

    use super::{parse_permission_args, validate_permission_key};

    fn parse_args(input: &str) -> Punctuated<MetaNameValue, Token![,]> {
        Punctuated::<MetaNameValue, Token![,]>::parse_terminated.parse_str(input).expect("测试属性必须可解析")
    }

    #[test]
    fn parses_valid_resource_and_action() {
        let args = parse_permission_args(parse_args(r#"resource = "order", action = "read""#))
            .expect("合法组合必须解析");

        assert_eq!(args.resource.value(), "order");
        assert_eq!(args.action.value(), "read");
    }

    #[test]
    fn ignores_metadata_and_unknown_keys() {
        let args = parse_permission_args(parse_args(
            r#"group = "g", group_desc = "d", desc = "x", extra = "y", resource = "a", action = "b""#,
        ))
        .expect("元数据与未知键不得拒绝");

        assert_eq!(args.resource.value(), "a");
        assert_eq!(args.action.value(), "b");
    }

    #[test]
    fn rejects_missing_and_non_string_values() {
        assert!(parse_permission_args(parse_args(r#"resource = "a""#)).is_err());
        assert!(parse_permission_args(parse_args(r#"resource = 123, action = "b""#)).is_err());
        assert!(parse_permission_args(parse_args(r#"resource = "a", action = 123"#)).is_err());
    }

    #[test]
    fn rejects_illegal_permission_key_at_expand_time() {
        let args = parse_permission_args(parse_args(r#"resource = "a", action = "B/C""#))
            .expect("解析保留原文大小写");
        // 动作含斜杠应在展开期校验拒绝，运行时 expect 仍为兜底。
        assert!(validate_permission_key(&args.resource, &args.action).is_err());
    }
}
