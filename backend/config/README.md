# Config Crate

`config` 负责解析应用与 MongoDB 配置，并通过 `SafeConfig` 提供只读快照和变更订阅。
配置来源二选一：

- 默认读取 `--config-path` 指定的本地 TOML。
- 启用 `--enable-nacos` 后从 Nacos 拉取 TOML，并每 10 秒刷新一次。

最小配置如下：

```toml
[app]
port = 10001
# 故意使用不可启动的占位值；部署前必须替换为至少 32 个随机字节。
secret = "replace-with-at-least-32-random-bytes"
upload_path = "./uploads"
upload_min_free_bytes = 536870912
file_base_url = "http://localhost:10001/uploads"

[database]
uri = "mongodb://localhost:27017"
db_name = "rs_project_template"
```

`app.secret` 少于 32 字节或仍为公开示例占位值时，加载会立即失败。不要把真实密钥提交到
仓库；本地 `config.toml` 已被忽略。`upload_path` 必须指向专用上传目录，不能使用空路径、
当前工作目录或文件系统根目录。

S3 配置为可选启动参数。需要构建 `storage::S3Storage` 时必须完整填写以下
配置；未配置 `[s3]` 时现有本地存储配置继续有效：

```toml
[s3]
bucket = "erp-assets"
region = "cn-south-1"
# 直连 AWS S3 时可省略 endpoint。
endpoint = "https://s3.example.com"
access_key_id = "replace-with-access-key-id"
secret_access_key = "replace-with-secret-access-key"
# 仅临时凭证需要 session_token。
session_token = "replace-with-session-token"
key_prefix = "erp/uploads"
# MinIO 等需要 path-style URL 的服务设为 true。
force_path_style = false
```

`bucket`、`region`、`access_key_id` 和 `secret_access_key` 为必填字段。`endpoint` 必须是
`http://` 或 `https://` 绝对地址。`key_prefix` 必须是不含空分段、`.` 或 `..` 的
相对对象键前缀。真实凭证只能写入已忽略的 `config.toml` 或受控 Nacos 配置，
不得提交到仓库。

`[s3]` 只提供经校验的启动参数，不自动改变现有 Web API 的本地上传路由。需要将
上传路由切换到 S3 时，运行时必须在启动期构建并注入单个 `S3Storage` 实例。

```rust,no_run
use config::SafeConfig;

# async fn example() -> config::Result<()> {
let config = SafeConfig::from_args().await?;
let snapshot = config.snapshot();
println!("listen port: {}", snapshot.app.port);
# Ok(())
# }
```

运行时可热更新 JWT 密钥、`file_base_url` 和 `upload_min_free_bytes`。数据库连接、RBAC 与 `upload_path`
共享启动时固定的运行资源；Nacos 中的数据库或上传目录发生变化时，Web API 会记录
`restart_required = true` 并继续使用启动值，重启后才会生效。

`upload_path` 必须是根目录以下的明确专用目录，不得为空、`.`、文件系统根或包含 `..`。
Web API 启动时会创建并 canonicalize 该目录；若最终解析到根目录、当前工作目录或其祖先
则拒绝启动。
