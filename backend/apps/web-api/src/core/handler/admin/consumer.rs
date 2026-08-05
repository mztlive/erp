use axum::{
    extract::{Extension, Path, Query, State},
    Json,
};
use services::consumer::{
    ConsumerAccountService, ConsumerItem, ConsumerListParams, CreateConsumerParams, UpdateConsumerParams,
};
use services::{audit::AuditActor, Page};
use validator::Validate;

use crate::{
    app_state::AppState,
    core::{errors::Result, response::ApiResponse},
};

#[permission_macros::permission(
    group = "会员管理",
    group_desc = "消费者账号管理",
    desc = "查询消费者列表",
    resource = "consumer",
    action = "list"
)]
pub async fn list_consumers(
    State(state): State<AppState>,
    Query(params): Query<ConsumerListParams>,
) -> Result<Page<ConsumerItem>> {
    params.validate()?;

    let page = ConsumerAccountService::new(state.db())
        .consumer_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "会员管理",
    group_desc = "消费者账号管理",
    desc = "创建消费者",
    resource = "consumer",
    action = "create"
)]
pub async fn create_consumer(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(payload): Json<CreateConsumerParams>,
) -> Result<()> {
    payload.validate()?;

    ConsumerAccountService::new(state.db())
        .create(payload, actor)
        .await?;

    Ok(ApiResponse::<()>::ok())
}

#[permission_macros::permission(
    group = "会员管理",
    group_desc = "消费者账号管理",
    desc = "更新消费者信息",
    resource = "consumer",
    action = "update"
)]
pub async fn update_consumer(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(mut payload): Json<UpdateConsumerParams>,
) -> Result<()> {
    payload.id = id;
    payload.validate()?;

    payload.ensure_has_updates()?;

    ConsumerAccountService::new(state.db())
        .update(payload, actor)
        .await?;

    Ok(ApiResponse::<()>::ok())
}
