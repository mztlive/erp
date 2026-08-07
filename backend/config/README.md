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

[database]
uri = "mongodb://localhost:27017"
db_name = "rs_project_template"

[s3]
bucket = "erp-assets"
region = "cn-south-1"
endpoint = "https://s3.example.com"
access_key_id = "replace-with-access-key-id"
secret_access_key = "replace-with-secret-access-key"
key_prefix = "erp/uploads"
force_path_style = false
public_base_url = "https://assets.example.com"
```

`app.secret` 少于 32 字节或仍为公开示例占位值时，加载会立即失败。不要把真实密钥提交到
仓库；本地 `config.toml` 已被忽略。

`[s3]` 是 Web API 必需的启动配置。完整字段如下：

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
# 公开桶或 CDN 根地址，不包含 key_prefix。
public_base_url = "https://assets.example.com"
```

`bucket`、`region`、`access_key_id` 和 `secret_access_key` 为必填字段。`endpoint` 必须是
`http://` 或 `https://` 绝对地址。`key_prefix` 必须是不含空分段、`.` 或 `..` 的
相对对象键前缀。`public_base_url` 必须是无查询参数、无片段的 HTTP(S) 绝对地址。
Web API 上传对象后返回 `public_base_url/key_prefix/object_key`。真实凭证只能写入已忽略的
`config.toml` 或受控 Nacos 配置，不得提交到仓库。

```rust,no_run
use config::SafeConfig;

# async fn example() -> config::Result<()> {
let config = SafeConfig::from_args().await?;
let snapshot = config.snapshot();
println!("listen port: {}", snapshot.app.port);
# Ok(())
# }
```

运行时可热更新 JWT 密钥。数据库连接、RBAC 与 S3 客户端属于启动时固定资源；Nacos 中的
数据库或任一 S3 字段发生变化时，Web API 会记录 `restart_required = true` 并继续使用
启动值，重启后才会生效。
