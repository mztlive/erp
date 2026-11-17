//! 解析路由表达式中的方法、处理函数和显式权限引用。
use syn::visit::{self, Visit};
use syn::{Expr, ExprCall, ExprMethodCall, ExprPath, Lit};

use crate::build_permissions::RouteHandler;

/// 按源码中的注册顺序读取路由；无法识别的受保护表达式必须报错。
pub(crate) fn parse_routes(content: &str) -> Result<Vec<RouteHandler>, String> {
    let file = syn::parse_file(content).map_err(|error| error.to_string())?;
    let mut visitor = Routes { routes: Vec::new(), error: None };
    visitor.visit_file(&file);
    match visitor.error {
        Some(error) => Err(error),
        None => Ok(visitor.routes),
    }
}

struct Routes {
    routes: Vec<RouteHandler>,
    error: Option<String>,
}

impl<'ast> Visit<'ast> for Routes {
    fn visit_item_mod(&mut self, module: &'ast syn::ItemMod) {
        if module.attrs.iter().any(|attribute| {
            attribute.path().is_ident("cfg")
                && attribute.parse_args::<syn::Path>().is_ok_and(|path| path.is_ident("test"))
        }) {
            return;
        }
        visit::visit_item_mod(self, module);
    }

    fn visit_expr_method_call(&mut self, call: &'ast ExprMethodCall) {
        if call.method != "route" {
            visit::visit_expr_method_call(self, call);
            return;
        }
        self.visit_expr(&call.receiver);
        let result = route_arguments(call)
            .and_then(|(path, expression)| handlers(expression, &path, &[], &mut self.routes));
        if let Err(error) = result {
            self.error.get_or_insert(error);
        }
    }
}

fn route_arguments(call: &ExprMethodCall) -> Result<(String, &Expr), String> {
    let Some(Expr::Lit(path)) = call.args.first() else {
        return Err("路由路径必须是字符串字面量".into());
    };
    let Lit::Str(path) = &path.lit else {
        return Err("路由路径必须是字符串字面量".into());
    };
    let expression = call.args.iter().nth(1).ok_or("路由必须指定处理器")?;
    Ok((path.value(), expression))
}

fn handlers(
    expression: &Expr,
    path: &str,
    permission: &[String],
    out: &mut Vec<RouteHandler>,
) -> Result<(), String> {
    match expression {
        Expr::Call(call) => call_handlers(call, path, permission, out),
        Expr::MethodCall(call) => {
            handlers(&call.receiver, path, permission, out)?;
            if is_method(&call.method.to_string()) {
                push_handler(&call.method.to_string(), call.args.first(), path, permission, out)?;
            } else if call.method == "merge" {
                handlers(call.args.first().ok_or("merge 缺少路由")?, path, permission, out)?;
            }
            Ok(())
        },
        Expr::Paren(expression) => handlers(&expression.expr, path, permission, out),
        Expr::Path(_) if permission.is_empty() => Ok(()),
        _ => Err(format!("路由 {path} 使用了无法解析的处理器表达式")),
    }
}

fn call_handlers(
    call: &ExprCall,
    path: &str,
    permission: &[String],
    out: &mut Vec<RouteHandler>,
) -> Result<(), String> {
    let Expr::Path(function) = call.func.as_ref() else {
        return Err(format!("路由 {path} 的处理器必须是函数路径"));
    };
    let name = function.path.segments.last().ok_or("缺少函数名")?.ident.to_string();
    if name == "permission_route" {
        if call.args.len() != 2 {
            return Err(format!("路由 {path} 的权限目录标记必须包含路由与权限两个参数"));
        }
        let required = permission_symbol(call.args.iter().nth(1).ok_or("缺少权限定义")?)?;
        let route = call.args.first().ok_or("缺少目录标记对应的路由")?;
        let before = out.len();
        let mut guards = permission.to_vec();
        guards.push(required);
        handlers(route, path, &guards, out)?;
        if out.len() == before {
            return Err(format!("路由 {path} 的权限未绑定到 HTTP 方法"));
        }
        Ok(())
    } else if is_method(&name) {
        push_handler(&name, call.args.first(), path, permission, out)
    } else {
        // multipart_route 等包装器的首参数是 MethodRouter，其他参数是大小配置。
        handlers(call.args.first().ok_or("路由包装器缺少参数")?, path, permission, out)
    }
}

fn permission_symbol(expression: &Expr) -> Result<String, String> {
    if let Expr::Reference(reference) = expression {
        return permission_symbol(&reference.expr);
    }
    let Expr::Path(reference) = expression else {
        return Err("路由必须直接引用显式权限常量".into());
    };
    path_name(reference)
        .strip_prefix("permissions::")
        .map(str::to_owned)
        .ok_or_else(|| "权限引用必须使用 permissions:: 前缀".into())
}

fn push_handler(
    method: &str,
    expression: Option<&Expr>,
    path: &str,
    permission: &[String],
    out: &mut Vec<RouteHandler>,
) -> Result<(), String> {
    let Some(Expr::Path(handler)) = expression else {
        return Err(format!("路由 {path} 的 {method} 处理器必须是函数路径"));
    };
    out.push(RouteHandler {
        method: method.to_uppercase(),
        path: path.into(),
        handler: path_name(handler),
        permission: permission.last().cloned(),
        guards: permission.to_vec(),
    });
    Ok(())
}

fn is_method(name: &str) -> bool {
    matches!(name, "get" | "post" | "put" | "delete" | "patch" | "head" | "options" | "trace")
}

fn path_name(path: &ExprPath) -> String {
    path.path.segments.iter().map(|segment| segment.ident.to_string()).collect::<Vec<_>>().join("::")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_nested_metadata_and_independent_merged_method_permissions() {
        let routes = parse_routes(r#"fn routes() { Router::new()
            .route("/nested", permission_route(permission_route(put(h::update), &permissions::contract::UPDATE), &permissions::customer::ASSIGN))
            .route("/merged", wrapper(permission_route(get(h::read), &permissions::item::READ)).merge(wrapper(permission_route(post(h::command), &permissions::item::COMMAND)))); }"#).unwrap();
        assert_eq!(routes.len(), 3);
        assert_eq!(routes[0].guards, ["customer::ASSIGN", "contract::UPDATE"]);
        assert_eq!(routes[0].permission.as_deref(), Some("contract::UPDATE"));
        assert_eq!(routes[1].method, "GET");
        assert_eq!(routes[2].method, "POST");
        assert_eq!(routes[2].guards, ["item::COMMAND"]);
        assert_eq!(routes[2].permission.as_deref(), Some("item::COMMAND"));
    }

    #[test]
    fn reads_real_route_syntax_with_chains_wrappers_and_independent_permissions() {
        let source = r#"fn routes() { Router::new()
            .route("/first", permission_route(get(handler::first), &permissions::item::READ))
            .route("/second", permission_route(upload::multipart_route(post(handler::second), LIMIT), &permissions::item::WRITE).layer(limit))
            .route("/shared", permission_route(get(handler::first).post(handler::second), &permissions::item::READ)); }"#;
        let routes = parse_routes(source).unwrap();
        let actual = routes
            .iter()
            .map(|route| {
                (
                    route.method.as_str(),
                    route.path.as_str(),
                    route.handler.as_str(),
                    route.permission.as_deref(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            actual,
            vec![
                ("GET", "/first", "handler::first", Some("item::READ")),
                ("POST", "/second", "handler::second", Some("item::WRITE")),
                ("GET", "/shared", "handler::first", Some("item::READ")),
                ("POST", "/shared", "handler::second", Some("item::READ")),
            ]
        );
    }

    #[test]
    fn rejects_unresolvable_permissions_and_nonliteral_paths() {
        for source in [
            r#"fn routes() { Router::new().route("/a", permission_route(get(h), make_permission())); }"#,
            r#"fn routes() { Router::new().route(path, permission_route(get(h), &permissions::item::READ)); }"#,
            r#"fn routes() { Router::new().route("/a", permission_route(dynamic, &permissions::item::READ)); }"#,
            r#"fn routes() { Router::new().route("/a", permission_route(get(h))); }"#,
            r#"fn routes() { Router::new().route("/a", permission_route(get(h), &permissions::item::READ, extra)); }"#,
        ] {
            assert!(parse_routes(source).is_err());
        }
    }
}
