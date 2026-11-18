# storage：S3 对象存储

## 职责

本 crate 通过 AWS SDK 操作 S3 或兼容服务中的对象字节，统一客户端配置、对象键、错误映射、公开 URL 和分片直传接口。不解析 HTTP 表单，不校验文件内容，不登记文件资产或附件，不决定文件的业务读取权限。

### 使用场景

- 保存合同 PDF、商品图片、导入文件等对象，按同一相对路径读取或删除。
- 为可公开分发的对象生成 URL，或为大文件直传创建分片会话并签发限时 PUT 地址。

### 协作示例

上传合同 PDF 时，[web-api](../../../apps/erp-server/README.md) 的[上传层](../../../apps/erp-server/src/http/upload.rs)负责解析 Multipart 表单，并按场景校验大小、扩展名、声明 MIME 和真实文件头；本 crate 只保存字节。[erp-support](../../domains/erp-support/README.md) 维护文件资产元数据和通用单据附件关系：`FileAsset` 记录对象键、文件名、MIME、大小、内容指纹和治理状态，`DocumentAttachment` 记录哪个单据以何种用途引用资产。[erp-contract](../../domains/erp-contract/README.md)维护合同归档与修订。

跨域批量登记由 [erp-file-assets](../../orchestration/erp-file-assets/README.md)将已写入对象存储的文件整理成待登记资产，沿原业务事务写入资产和登记审计；业务用例建立相应的附件关系。S3 I/O 必须在数据库事务外执行，事务回滚不会删除已经写入的对象。标记资产销毁也不会自动删除对象；何时清理字节由业务流程决定。

## 目录结构

| 入口 | 用途 |
| --- | --- |
| [Cargo.toml](Cargo.toml) | 声明 AWS S3 SDK、URL、错误和日志依赖，不依赖 ERP 业务 crate。 |
| [src/lib.rs](src/lib.rs) | 导出 `S3Storage`、`S3StorageConfig`、`UploadedPart`、`Error` 和 `Result`；实现模块为私有。 |
| [src/s3.rs](src/s3.rs) | 客户端装配、配置校验、对象操作、分片上传和公开 URL。 |
| [src/path.rs](src/path.rs) | 相对对象路径与键前缀的校验。 |
| [src/cos.rs](src/cos.rs) | 为腾讯云 COS 标准域名上的图片 GET 请求追加原图参数。 |
| [src/content_cache.rs](src/content_cache.rs) | 进程内不可变对象字节缓存，限制期限、容量和在途请求回填。 |
| [src/error.rs](src/error.rs) | 存储错误分类及 `Result` 类型别名。 |

## 架构设计

### 配置与对象键

[应用启动入口](../../../apps/erp-server/src/bootstrap.rs)从运行配置构造并复用共享的 `S3Storage`，不得为每次请求重建客户端。`S3Storage::new` 在构造 SDK 客户端前进行本地校验；配置不合法返回 `Error::InvalidConfig`，通过校验不代表端点可达或凭证有效。应用可限时调用 `warm_connection`，以 `HeadBucket` 预建连接而不读写对象；预热失败不阻止启动。

| `S3StorageConfig` 字段 | 合同 |
| --- | --- |
| `bucket`、`region`、`access_key_id`、`secret_access_key` | 必填；不得为空、全为空白或含首尾空白。`region` 用于签名，兼容服务按其要求配置。凭证由应用注入，不得硬编码或写入日志。 |
| `session_token` | 可选，用于临时凭证；如提供，同样不得为空、全为空白或含首尾空白。 |
| `endpoint`、`force_path_style` | API 端点可省略；指定时须为带主机的 HTTP(S) 绝对 URL。默认不强制 path-style；MinIO 等兼容服务可按部署要求启用，使 bucket 位于请求路径而非子域名。 |
| `key_prefix` | 可选；配置后原样加在相对路径之前。拒绝空值、全空白、首尾空白、首尾斜杠、反斜杠以及空段、`.`、`..` 段。 |
| `public_base_url` | 必填；使用带主机、无查询参数和片段的 HTTP(S) 绝对 URL，可保留 CDN 或 bucket 基址中的路径。 |

对象操作、分片操作和 `public_url` 共用同一键规则。调用方传入未拼接前缀、未做 URL 编码的相对路径。路径忽略 `.` 分量，将有效分量用 `/` 连接；空路径、没有有效分量的路径、绝对路径、根目录、父目录、Windows 路径前缀、非 UTF-8 分量及含反斜杠的分量返回 `Error::PathError`。

### 能力

| 接口 | 行为 |
| --- | --- |
| `save_with_content_type` / `save_owned_with_content_type` | 通过 `PutObject` 保存字节，可设置调用方已校验的 MIME。借用切片的入口会复制整个缓冲区；已持有 `Vec<u8>` 时优先转移所有权，调用前完成内容校验、大小记录和指纹计算。 |
| `read` / `read_stream` | 通过 `GetObject` 读取字节。`read` 完整读入内存且绕过缓存；大文件或逐块处理使用 `read_stream`，由调用方消费字节流。GET 返回 `NoSuchKey` 时映射为 `Error::NotFound`。 |
| `read_immutable` | 按对象键和已登记内容指纹复用完整字节，仅用于新键写入且不再覆盖的受控对象。缓存不是内容校验或授权。 |
| `delete` | 先用 `HeadObject` 检查存在性，检查返回 `NotFound` 时映射为 `Error::NotFound`；检查成功后才调用 `DeleteObject`。 |
| `public_url` | 本地生成不带签名的公开 URL：保留公开基址路径，将含前缀的对象键逐段编码后追加，不另加 bucket。它不检查对象是否存在，也不授予访问权限。业务私有附件不得用此接口替代受控下载。 |

除明确映射为 `NotFound` 的情况外，SDK 操作失败及 `read` 收集正文失败归为 `Error::S3`。存储操作日志只允许记录操作类型、耗时、成功状态或缓存命中，不得记录对象内容、对象键、签名或凭证。

### 分片直传与预签名 URL

1. `create_multipart_upload` 对相对路径创建上传会话，可设置最终对象的 MIME，返回后续操作所需的上传 ID；响应缺少 ID 时返回 `Error::S3`。
2. `presign_upload_part` 按对象键、上传 ID、分片序号和有效期生成限时 PUT 地址，签发时不上传字节。序号须在 `1..=10000`，有效期由 SDK 校验。浏览器直接 PUT 到签名地址，不附带应用鉴权头；对象存储桶的跨域配置须向浏览器暴露响应 `ETag`。签名地址不是公开下载 URL，不能用 `public_url` 构造或替换。
3. `complete_multipart_upload` 接收上传 ID 和按序号升序提交的 `UploadedPart`（分片序号与 PUT 响应 `ETag`），去除 `ETag` 引号后交给 S3 合并；不排序、不去重。空白上传 ID、空分片清单、越界序号或去引号后为空白的 `ETag` 会在请求前被拒绝。
4. 放弃上传时用 `abort_multipart_upload` 按相对路径和上传 ID 取消会话，清理未合并分片；它不删除已经合并的对象。

HTTP 表单 Multipart 和 S3 分片直传是两套协议。直传接口不校验业务权限、文件大小或内容，也不登记资产。例如，[商品导入直传流程](../../orchestration/erp-data-import/src/product_import/direct_upload.rs)在签名前检查业务资格和请求与对象键的绑定，合并后读取并核验文件，再登记资产和导入任务。

### 原图读取与不可变字节缓存

所有远端 `GetObject` 共用 COS 原图处理：对腾讯云 COS 标准域名（如 `cos.ap-guangzhou.myqcloud.com` 或 `bucket.cos.ap-guangzhou.myqcloud.com`）上扩展名为 JPG、JPEG、PNG、GIF、WebP 的对象，在签名前追加 `ci-process=originImage`，扩展名匹配不区分大小写。其他服务和非图片仍用标准 GET。原图参数见 [COS 获取原图接口](https://cloud.tencent.com/document/product/436/119346)。[受控附件下载](../../../apps/erp-server/src/http/handler/file_asset/read.rs)仍须核对登记的大小、内容指纹和适用的真实类型；不得以压缩结果覆盖登记指纹或跳过校验。

`read_immutable` 只缓存成功 GET 的完整正文，无指纹或超过 512 KiB 的对象不入缓存。条目绝对期限为 30 秒，命中不续期；缓存总量上限 8 MiB、最多 256 项。登记指纹变化、到期或同一共享客户端的 PUT、分片合并及删除都会使旧条目失效；变更推进缓存代次，旧代次的在途 GET 不得回填。缓存不保证同键读写线性化，外部或其他进程的同键覆盖、删除可能在缓存期限内不可见；存在这类变更的对象不得使用此入口。可变对象继续用 `read` 或 `read_stream`。

授权结果、审计结果和完整 HTTP 响应不得缓存。即使命中字节缓存，调用方也必须每次核验当前业务对象的整单读取资格、文件治理状态并记录敏感读取审计；交付前重验权限、文件状态和版本，HTTP 响应仍须带 `Cache-Control: private, no-store`。

## 代码入口与修改要求

通过[目录结构](#目录结构)定位实现。新增对象操作须复用统一对象键规则；公开地址使用 `public_url`，不手工拼接。修改存储行为时，使用库内模拟 HTTP 客户端覆盖请求和错误映射，不连接真实 S3。

## 验证要求

仅修改文档时，按[后端开发规则](../../../AGENTS.md#验证)只对改动文件运行 `git diff --check`。存储代码变更时，在 `backend/` 执行库单元测试及[统一质量门禁](../../README.md#质量门禁)要求的检查，不运行集成测试或真实外部服务测试：

```bash
env -u ERP_TEST_MONGO_URI cargo test -p storage --lib --locked
```
