# storage：S3 对象存储

将文件字节保存到 S3 或兼容服务，提供读取、删除、公开 URL 和大文件分片上传能力。

上传合同 PDF、商品图片和导入文件都需要统一的对象键、客户端配置和存储错误处理。本 crate 封装这些文件存储操作。

## 使用场景

- 保存、读取、删除文件，或为对象生成公开 URL。
- 调整 S3 兼容连接、对象路径校验和分片上传操作。

## 协作示例

上传合同 PDF 时，本 crate 保存文件字节；[erp-support](../erp-support/README.md) 记录文件资产及附件关系，[erp-contract](../erp-contract/README.md) 维护合同归档。[web-api](../../apps/web-api/README.md) 负责 HTTP 上传协议和文件校验。

## 能力

- `S3Storage` 通过 AWS SDK 保存、读取和删除 S3 对象。
- 支持自定义 endpoint、session token、对象键前缀和 path-style URL，可连接 MinIO 等 S3-compatible 服务。
- 提供分片上传的创建、分片预签名、完成和取消操作，供大文件直传流程使用。
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

## 代码入口与修改要求

| 入口 | 用途 |
| --- | --- |
| [src/s3.rs](src/s3.rs) | `S3Storage`、配置及 S3 操作 |
| [src/path.rs](src/path.rs) | 对象相对路径与键规则 |
| [src/error.rs](src/error.rs) | 稳定存储错误 |
| [src/lib.rs](src/lib.rs) | 公共导出 |

1. 对象键必须通过统一路径校验，公开 URL 必须使用 `public_url` 生成，保持前缀和 URL 编码一致。
2. 凭证由应用配置注入，不得硬编码真实密钥或输出凭证日志。
3. 文件资产元数据与附件关系由 `erp-support` 维护；S3 I/O 必须位于数据库事务之外。
4. 修改存储行为时使用库内模拟 HTTP 客户端测试覆盖请求与错误映射，不连接真实 S3。

## 验证要求

以下命令在 `backend/` 目录执行：

```bash
cargo check -p storage --locked
env -u ERP_TEST_MONGO_URI cargo test -p storage --lib --locked
```

代码变更还须执行[统一质量门禁](../README.md#质量门禁)。测试范围限定为库单元测试，不执行集成测试或真实外部服务测试。
