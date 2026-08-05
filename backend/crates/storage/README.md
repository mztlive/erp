# Storage Crate

`storage` 只负责把文件限制在配置的本地基础目录内，不包含 HTTP 或 Multipart 协议类型。

## 能力

- 保存、读取、删除和检查本地文件。
- 自动创建基础目录和文件父目录。
- 保存时先写入并同步同目录随机临时文件，再原子重命名；失败或任务取消时尽力清理临时文件。
- 拒绝绝对路径、父目录、根目录、Windows Prefix 和空文件路径。

Multipart 解析、文件大小、扩展名、声明 MIME 与文件头真实类型校验位于
`apps/web-api/src/core/upload.rs`。

```rust
let storage = storage::LocalStorage::new("./uploads").await?;
storage.save("images/example.png", image_bytes).await?;
let content = storage.read("images/example.png").await?;
# Ok::<(), storage::Error>(())
```
