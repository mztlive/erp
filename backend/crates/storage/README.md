# Storage Crate

`storage` 负责本地文件与 S3 对象的存储操作，不包含 HTTP 或 Multipart 协议类型。

## 能力

- `LocalStorage` 保存、读取、删除和检查本地文件。
- `S3Storage` 通过 AWS SDK 保存、读取、删除和检查 S3 对象。
- S3 支持自定义 endpoint、session token、对象键前缀和 path-style URL，可连接 MinIO 等 S3-compatible 服务。
- `LocalStorage` 自动创建基础目录和文件父目录。
- `LocalStorage` 保存时先写入并同步同目录随机临时文件，再原子重命名；失败或任务取消时尽力清理临时文件。
- 两种实现都拒绝绝对路径、父目录、根目录、Windows Prefix 和空文件路径。

Multipart 解析、文件大小、扩展名、声明 MIME 与文件头真实类型校验位于
`apps/web-api/src/core/upload.rs`。

```rust
let storage = storage::LocalStorage::new("./uploads").await?;
storage.save("images/example.png", image_bytes).await?;
let content = storage.read("images/example.png").await?;
# Ok::<(), storage::Error>(())
```

S3 调用方必须从运行配置构造并复用单个客户端；不得在每次请求中重建客户端：

```rust,no_run
use storage::{S3Storage, S3StorageConfig};

let storage = S3Storage::new(S3StorageConfig {
    bucket: "erp-assets".to_string(),
    region: "cn-south-1".to_string(),
    endpoint: Some("https://s3.example.com".to_string()),
    access_key_id: "access-key".to_string(),
    secret_access_key: "secret-key".to_string(),
    session_token: None,
    key_prefix: Some("erp/uploads".to_string()),
    force_path_style: false,
})?;
storage.save("images/example.png", image_bytes).await?;
# Ok::<(), storage::Error>(())
```

`read` 会将 S3 `NoSuchKey` 转换为 `storage::Error::NotFound`。`delete` 先通过
`HeadObject` 确认对象存在，以保持与本地存储一致的不存在错误语义。
