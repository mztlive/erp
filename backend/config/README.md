# Config Crate

`config` 负责解析应用与 MongoDB 配置，并通过 `SafeConfig` 提供只读快照和变更订阅。
配置来源二选一：

- 本地开发与隔离 E2E 默认读取 `--config-path` 指定的本地 TOML。
- TKE 测试、生产使用 `--enable-nacos` 从 Nacos 读取完整 TOML；API 每 10 秒刷新，CLI 单次读取。
- 两种来源不得混用。Nacos 必须显式传入地址、Namespace UUID、Group、Data ID，并设置非空环境变量 `NACOS_CLIENT_USERNAME`、`NACOS_CLIENT_PASSWORD`；不得在命令参数中传密码。

接入以 [Nacos 环境配置接入与权限管理规范](https://outline.fushangyunfu.com/doc/nacos-vxFwmIgO8r)为准，ERP 使用 `DEFAULT_GROUP / erp`。测试 Namespace 为 `ccf7ec38-1d60-407e-bf2c-7c4654c481d0`，生产为 `8888c735-1d29-4765-ab7f-70100a888479`。地址使用 `host:port`，不得使用控制台 HTTPS URL 或 `http://` 前缀。

```bash
# 凭据由 Secret 或受控终端环境注入；本机调试先建立 8848、9848 端口转发。
cargo run -p web-api -- --enable-nacos \
  --nacos-addr 127.0.0.1:8848 \
  --nacos-namespace ccf7ec38-1d60-407e-bf2c-7c4654c481d0 \
  --nacos-group DEFAULT_GROUP --nacos-data-id erp
```

兼容 `--enable-nacos true`。连接和读取均在 30 秒后超时失败；首次读取失败不得降级到文件或 SDK 磁盘缓存。运行中读取、解析或应用校验失败保留有效快照。SDK 环境优先级已关闭，`NACOS_CLIENT_SERVER_ADDRESS`、`NACOS_CLIENT_NAMESPACE`、`NACOS_CLIENT_ENDPOINT` 和缓存环境变量不能覆盖启动参数；只有上述两个凭据变量由应用显式读取。

锁定的 SDK 0.8.0 在 debug 日志包含登录密码，API 和 CLI 必须全局过滤 `nacos_sdk` 日志。应用记录脱敏的连接标识、错误类别及协议错误码，不记录配置原文；TOML 解析错误只保留字节位置。新增进程复用本 crate 时必须执行同一日志过滤约束。

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

可选的 `[bootstrap].initial_admin_password` 只在数据库里还没有账号 `admin` 时，由 Web API
启动流程创建超级管理员并绑定 `role-root`。账号已存在时忽略该字段，不修改密码、状态或角色。
未配置或空字符串表示不创建。密码必须是 6 到 32 个字符，且不能使用模板里的示例占位值。
调试输出会把该字段打成 `<redacted>`。首次登录成功后应从配置中删除这段并重启。Nacos 热更新
不会补建或改写已有超级管理员，改动要等进程重启后才参与判断。

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

商品导入浏览器直传要求浏览器能直接访问 `endpoint`：对象存储桶须配置跨域
（允许 Web 前端来源的 `PUT`，并暴露 `ETag` 响应头，前端靠它确认分片）。
若存储内网地址浏览器不可达，直传会失败，前端会自动提供「改用普通上传」兜底。

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

可选 `[dangaoshushu]` 配置由 `DangaoshushuConfig` 校验，缺省或 `enabled=false` 时关闭。
登记须填写 HTTPS origin、渠道号和密钥；ERP 身份与环境只从后台连接读取。地址准备另须
填写固定 `user_id`。计量单位、标准地区与人民币含税结算价口径必须明确映射，不从供应商
目录猜测。完整模板见 [config.toml.example](../config.toml.example)，连接绑定、运行范围、
推送与回滚执行 [蛋糕叔叔接入合同](../../docs/dangaoshushu-connector-contract.md)。
供应商技术参数使用启动快照，修改文件或 Nacos 后必须重启并在后台重新绑定引用，不能依赖热更新
完成启停或凭据轮换。配置调试输出整体脱敏；真实密钥只保存在已忽略配置或受控 Nacos。

蛋糕叔叔配置不得包含 `connection_id`、`supplier_id`、`environment`。这些信息由后台“供应商 API”连接记录持有；新建连接后在页面选择地址和渠道凭据，无需回填内部 ID。配置 `enabled` 只登记技术参数，业务启停由后台连接控制。
