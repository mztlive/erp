# Storage Crate

`storage` 负责 S3 对象存储，不包含 HTTP 或 Multipart 协议类型。

## 能力

- `S3Storage` 通过 AWS SDK 保存、读取和删除 S3 对象。
- 支持自定义 endpoint、session token、对象键前缀和 path-style URL，可连接 MinIO 等 S3-compatible 服务。
- 拒绝绝对路径、父目录、根目录、Windows Prefix 和空文件路径。

Multipart 解析、文件大小、扩展名、声明 MIME 与文件头真实类型校验位于
`apps/web-api/src/core/upload.rs`。

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
    public_base_url: "https://cdn.example.com".to_string(),
    force_path_style: false,
})?;
storage
    .save_with_content_type("images/example.png", image_bytes, Some("image/png"))
    .await?;
let public_url = storage.public_url("images/example.png")?;
# Ok::<(), storage::Error>(())
```

`read` 会将 S3 `NoSuchKey` 转换为 `storage::Error::NotFound`。`delete` 先通过
`HeadObject` 确认对象存在，对象不存在时返回同样的 `NotFound`。
