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

COS 标准域名上的 JPG、JPEG、PNG、GIF、WebP 对象读取必须在签名前追加
`ci-process=originImage`，取得上传原图，避免自动压缩改变文件大小和内容指纹。
其他 S3 服务及非图片对象保持标准 `GetObject`。文件下载仍须校验登记的大小、
类型和内容指纹；禁止以压缩结果覆盖已登记指纹或跳过校验。
原图参数遵循 [COS 获取原图接口](https://cloud.tencent.com/document/product/436/119346)。

上传已持有 `Vec<u8>` 时，使用 `save_owned_with_content_type` 转移请求体所有权；
调用前必须完成文件校验、字节数记录和内容指纹计算。旧的借用入口保持原合同。
应用启动可限时调用 `warm_connection` 预建连接；预热失败不得改变启动资格。

`read_immutable` 仅用于新键存储且内容不可修改的受控对象。调用方必须每次执行当前
业务对象整单读取资格、文件治理状态、敏感读取审计，以及响应前的权限和版本重验；
不得复用授权结果或完整 HTTP 响应。响应仍须携带 `private, no-store`。
字节缓存仅接收成功 GET，绝对期限 30 秒、单对象 512 KiB、总内容 8 MiB、最多 256 项。
指纹变化、TTL 到期或同客户端对象变更均失效；在途 GET 不得跨变更代次回填。
此入口不保证同键读写线性化，也不适用于绕过 ERP 覆盖或删除物理对象的使用方式；
外部同键变化和其他进程的变化最多可能在缓存期限内不可见。可变对象继续使用 `read`。

存储操作日志只记录操作类型、耗时、成功状态与缓存命中；禁止写入内容、对象键、签名和凭证。

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
env -u ERP_TEST_MONGO_URI cargo test -p storage --lib --locked
```

代码变更还须执行[统一质量门禁](../README.md#质量门禁)。测试范围限定为库单元测试，不执行集成测试或真实外部服务测试。
