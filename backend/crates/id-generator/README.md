# ID Generator

`id-generator` 只提供跨应用共享的 UUID v4 ID 生成能力。

## 使用方式

```rust
use id_generator::next_id;

let id = next_id();
```

生成器返回不带连字符的 32 位十六进制字符串，不需要跨实例协调，也不持有全局锁。
