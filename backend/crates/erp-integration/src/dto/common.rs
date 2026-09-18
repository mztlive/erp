/// 排序方向。
pub use application_core::SortDir;

/// 归一化后的分页查询 DTO（Service → Repository 共用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageParams {
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数（已 clamp 到 1–100）。
    pub page_size: u32,
    /// 排序字段（已过白名单校验，`&'static str` 保证来源只可能是白名单）。
    pub sort_by: &'static str,
    /// 排序方向。
    pub sort_dir: SortDir,
}

/// 契约目标形状的分页响应（api-contract §3）：`items` + `total` + `page` + `page_size`。
pub use application_core::PageView;
/// 校验文本去除首尾空白后非空（validator 的 `length(min=1)` 对纯空白字符串不生效）。
pub(super) use application_core::non_blank;
/// 拒绝把 `"me"` 当作人员 ID。
///
/// # 参数
/// * `ids` - 已解析的稳定人员 ID
/// * `field` - 面向用户的字段名
///
/// # 返回
/// 不含 `me` 时成功。
///
/// # 错误
/// 任一 ID 为 `me` 时返回校验错误。
pub(crate) fn reject_me_ids(ids: &[String], field: &str) -> crate::Result<()> {
    if ids.iter().any(|id| id.eq_ignore_ascii_case("me")) {
        return Err(crate::Error::ValidationError(format!("{field}不得使用 me 作为人员 ID")));
    }
    Ok(())
}

/// 校验排序参数（白名单 + 方向），返回归一化排序字段与方向。///
/// # 参数
/// * `sort_by` - 可选排序字段；空白视为未提供
/// * `sort_dir` - 可选排序方向；空白视为未提供
/// * `allowed_fields` - 白名单
///
/// # 返回
/// 返回 `(排序字段, 方向)`；未提供时默认 `("created_at", Desc)`。
///
/// # 错误
/// 字段不在白名单或方向不是 `asc`/`desc` 时返回 `ValidationError`。
pub(crate) use application_core::normalize_sort;

impl PageParams {
    /// 归一化分页与排序参数（白名单校验 + 默认值）。
    ///
    /// # 参数
    /// * `page` - 页码（`None` 取第一页）
    /// * `page_size` - 单页条数（`None` 取默认并 clamp 到 1–100）
    /// * `sort_by` - 排序字段；空白视为未提供
    /// * `sort_dir` - 排序方向；空白视为未提供
    /// * `allowed_sort` - 排序白名单
    ///
    /// # 返回
    /// 返回归一化后的分页参数。
    ///
    /// # 错误
    /// 字段不在白名单或方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(
        page: Option<u64>,
        page_size: Option<u32>,
        sort_by: &Option<String>,
        sort_dir: &Option<String>,
        allowed_sort: &'static [&'static str],
    ) -> crate::Result<Self> {
        use application_core::{page_or_default, page_size_or_default};

        let (sort_by, sort_dir) = normalize_sort(sort_by, sort_dir, allowed_sort)?;
        Ok(Self {
            page: page_or_default(page),
            page_size: page_size_or_default(page_size),
            sort_by,
            sort_dir,
        })
    }
}

/// 校验范围版本长度与下级约束（错误任务与差异列表共用）。
///
/// # 参数
/// * `scope_version` - 跨页范围版本
/// * `org_unit_ids` - 请求组织筛选
/// * `include_descendants` - 是否包含有效下级
///
/// # 返回
/// 校验通过返回 `Ok(())`。
///
/// # 错误
/// 范围版本为空/超长，或包含下级却无组织筛选时返回 `ValidationError`。
pub(crate) fn check_scoped_version(
    scope_version: &Option<String>,
    org_unit_ids: &Option<application_core::QueryIds>,
    include_descendants: Option<bool>,
) -> crate::Result<()> {
    if scope_version.as_ref().is_some_and(|version| version.is_empty() || version.len() > 256) {
        return Err(crate::Error::ValidationError("范围版本非法".into()));
    }
    if include_descendants == Some(true) && org_unit_ids.is_none() {
        return Err(crate::Error::ValidationError("包含下级时必须提供组织筛选".into()));
    }
    Ok(())
}

/// 归一化处理人筛选并拒绝 `me` 占位（错误任务与差异列表共用）。
///
/// # 参数
/// * `handler_user_ids` - 当前处理人筛选
/// * `operator_user_ids` - 历史处理人筛选
///
/// # 返回
/// 返回 `(当前处理人, 历史处理人)`；未提供时对应为空列表。
///
/// # 错误
/// 任一 ID 为 `me` 时返回校验错误。
pub(crate) fn normalize_handler_filters(
    handler_user_ids: &Option<application_core::QueryIds>,
    operator_user_ids: &Option<application_core::QueryIds>,
) -> crate::Result<(Vec<String>, Vec<String>)> {
    let handler = handler_user_ids.as_ref().map(application_core::QueryIds::as_slice).unwrap_or(&[]).to_vec();
    let operator =
        operator_user_ids.as_ref().map(application_core::QueryIds::as_slice).unwrap_or(&[]).to_vec();
    reject_me_ids(&handler, "当前处理人")?;
    reject_me_ids(&operator, "历史处理人")?;
    Ok((handler, operator))
}

/// 归一化组织筛选（错误任务与差异列表共用）。
///
/// # 参数
/// * `org_unit_ids` - 请求组织筛选
///
/// # 返回
/// 返回组织 ID 列表；未提供时为空（表示不按组织收窄）。
pub(crate) fn normalize_org_filter(org_unit_ids: &Option<application_core::QueryIds>) -> Vec<String> {
    org_unit_ids.as_ref().map(application_core::QueryIds::as_slice).unwrap_or(&[]).to_vec()
}

#[cfg(test)]
mod tests {
    use super::{SortDir, normalize_sort};

    #[test]
    fn sort_whitelist_rejects_unknown_fields_and_directions() {
        assert!(normalize_sort(&Some("name".to_string()), &None, &["created_at"]).is_err());
        assert!(normalize_sort(&None, &Some("up".to_string()), &["created_at"]).is_err());

        let (field, direction) = normalize_sort(
            &Some(" received_at ".to_string()),
            &Some(" asc ".to_string()),
            &["created_at", "received_at"],
        )
        .unwrap();
        assert_eq!(field, "received_at");
        assert_eq!(direction, SortDir::Asc);

        let (field, direction) = normalize_sort(&None, &None, &["created_at"]).unwrap();
        assert_eq!(field, "created_at");
        assert_eq!(direction, SortDir::Desc);
    }
}
