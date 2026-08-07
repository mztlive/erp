# Web API 开发指南

`web-api` 是 Axum HTTP 适配层。Handler 只负责提取协议输入、向 Service 传递已认证的
审计身份、调用用例并包装 `ApiResponse`；管理资源审计语义与事务属于 `services`，
领域规则和持久化查询分别属于 `entities` 与 `database`。

## 当前结构

```text
src/
├── app_state.rs
├── main.rs
└── core/
    ├── auth/             # JWT 编解码
    ├── extractor/        # 请求扩展类型
    ├── handler/
    │   ├── admin/        # 管理端协议适配
    │   ├── auth/         # 登录与当前账号
    │   └── upload.rs     # 已认证后台图片上传
    ├── middleware/
    │   ├── authentication.rs
    │   └── rbac.rs
    ├── routes/           # public/account/admin 路由组合
    ├── tracing/          # Web API 日志与请求 tracing
    ├── errors.rs
    ├── rate_limit.rs     # 按 key、全局窗口和并发的进程内准入计数
    ├── response.rs
    └── upload.rs         # Multipart 图片解析与真实类型校验
```

`AppState` 持有启动时建立的 MongoDB `Database`、`S3Storage`、`SafeConfig`、共享
`RbacService` 和按需构建的 JWT 引擎。Service 在 Handler 中按请求构造，并复用
`state.db()` 与 `state.rbac()`。

## 路由与鉴权

- 公开：`/health`、`/login`。
- 当前账号：`/account/*` 使用 `authenticate`。
- 管理端：`/admin/*` 先认证，再通过 `with_permission` 调用 Casbin RBAC。
- 上传：`POST /upload` 保持原路径，只允许后台 JWT。
  它在读取 Multipart 前执行每主体、全局窗口和并发限制，并设置独立 body limit。

登录入口使用 4 KiB 请求体上限和进程内限制：每个 TCP peer IP 20 次/60 秒、每个“来源 +
规范化账号”组合 5 次/60 秒、全局应急熔断 600 次/60 秒、并发 4。账号部分复用
`LoginAccount` 的 trim 语义，无效超长输入会被截断；超限统一
返回 `429` 和 `Retry-After`。默认只使用 TCP 对端地址，不信任客户端可伪造的转发头；
反向代理部署必须通过可信网络拓扑保留真实来源地址，或在可信代理边界显式完成来源地址
传递。

管理端 Handler 使用 `#[permission_macros::permission(...)]` 声明
`resource:action`；`build.rs` 解析这些声明生成前端权限定义。新增管理端接口时必须同时
挂到 `routes/admin.rs` 的认证与 RBAC 链路。

## DTO 与 Handler 边界

- 请求体和响应默认直接复用 `services::<domain>` 对外 re-export 的 DTO。
- 分页响应使用 `services::Page<T>`，保持 `items`、`total` 合同。
- Handler 不导入 Repository filter、`PageResult` 或 MongoDB `Document`。
- 仅路径参数拆分、认证上下文注入或协议字段名不同时创建最小 HTTP 包装，并提供显式转换。
- 参数规范化和业务校验放在 Service DTO 或实体中，不在多个 Handler 复制。

典型 Handler 直接调用 Service：

```rust
pub async fn list_roles(State(state): State<AppState>) -> Result<Vec<RoleItem>> {
    let roles = state.rbac().role_list().await?;
    Ok(ApiResponse::ok_with_data(roles))
}
```

## 响应与错误

`core::errors::Result<T>` 的成功值是 `ApiResponse<T>`。JSON 字段保持：

```json
{
    "status": 200,
    "errorMessage": "OK",
    "data": null,
    "success": true
}
```

HTTP status 与响应体中的 `status` 一致：

- 参数/validator 错误：`400`
- 未认证：`401`
- 无权限：`403`
- 不存在：`404`
- 乐观锁或唯一性冲突：`409`
- 领域/业务规则不满足：`422`
- 登录或上传限流：`429`，并携带 `Retry-After`
- Repository、配置或内部错误：`500`

内部错误只向客户端返回稳定的“系统内部错误”，底层错误细节留在结构化日志中。
事务提交结果无法确认时也返回稳定的 `500`，但提示调用方先查询当前状态；管理端默认不自动
重试 mutation，避免把“可能已提交”误判为确定失败后重放写请求。全局请求超时为 180 秒，
用于覆盖正常的 MongoDB 提交确认窗口，但业务仍应为需要安全重试的写操作设计幂等键。

## 图片上传

HTTP 上传逻辑分为三层：

- `core/upload.rs`：读取 Multipart，限制单文件 5 MiB，校验
  JPEG/PNG/WebP/GIF 扩展名、声明 MIME 与文件头真实类型三者一致，并把认证主体
  交给通用 `rate_limit` 计数。
- `handler/upload.rs`：生成安全随机对象键，调用注入的 `storage::S3Storage` 上传对象，
  显式写入已验证的 `Content-Type`；`POST /upload` 保持 `{ url }` 响应。
- `handler/file_asset/mod.rs`：文件资产上传同样写入 S3，并在返回的 `FileAssetView.public_url`
  中提供公开 URL；仅登记元数据的接口不上传对象。
- `storage::S3Storage`：把 `key_prefix` 加到对象键前；公开 URL 按
  `public_base_url/key_prefix/object_key` 生成，并对路径分段执行 URL 编码。

`storage` 不依赖 Axum 或 Multipart，只负责安全相对对象键与 S3 I/O。新增格式时需要同时
补充扩展名、MIME、文件头识别和失败路径测试。

S3 客户端在启动时固定。Nacos 修改数据库或任一 S3 字段后，进程记录
`restart_required = true` 并继续使用当前客户端，重启后生效。进程内上传限流只抑制
单实例突发滥用；多实例部署必须在网关或共享限流服务补充集群级配额。对象生命周期、
公开访问策略和 CDN 缓存策略由部署环境负责。

## 新增接口

1. 先在 `entities` 定义或复用不变式，在 `services` 增加 DTO 与编排。
2. 在对应 `handler/{admin,auth}` 模块或明确的共享 Handler 文件中添加薄 Handler。
3. 在 `routes/{admin,account,public}.rs` 注册路由。
4. 管理端接口添加 permission 宏并使用 `with_permission`。
5. 覆盖正常、校验失败、权限/冲突等关键路径。
6. 运行：

   ```bash
   cargo fmt --all -- --check
   cargo check --workspace
   cargo clippy --workspace --all-targets --all-features -- -D warnings
   cargo test --workspace
   ```

生成权限定义后，确认生成文件未漂移。
