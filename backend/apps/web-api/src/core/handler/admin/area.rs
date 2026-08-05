use crate::core::{errors::Result, response::ApiResponse};

#[permission_macros::permission(
    group = "共享数据",
    group_desc = "共享基础数据查询",
    desc = "查询省市区树",
    resource = "shared",
    action = "area_tree_list"
)]
/// 查询省市区树。
///
/// # 返回值
/// 返回省市区树。
pub async fn area_tree() -> Result<Vec<services::area::AreaNode>> {
    let tree = services::area::area_tree().await?;
    Ok(ApiResponse::ok_with_data(tree))
}
