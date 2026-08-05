# Auth 值对象

`entities/src/auth` 只包含不依赖 I/O 的认证值对象。数据库查询、异步调度和成功登录后的
凭证升级由 Service 负责。

## `LoginAccount`

`LoginAccount::new` 去除账号首尾空白并校验字符数，不改变大小写。`Secret::new`、
`change_account` 和消费者创建接口都接收这个已规范化类型，避免各调用方重复处理账号。

```rust
use entities::{LoginAccount, Secret};

let account = LoginAccount::new(" example_user ")?;
let secret = Secret::new(account, "example-only-password")?;
assert_eq!(secret.account(), "example_user");
# Ok::<(), entities::Error>(())
```

## `Secret`

- `account` 与 `password_hash` 字段保持私有，只暴露最小读取/变更方法。
- Serde 持久化字段仍为 `account` 和 `password`，兼容现有 MongoDB 文档。
- 新建和修改密码使用带随机盐的 Argon2id PHC 字符串。
- `Debug` 始终把密码字段输出为 `[REDACTED]`，不暴露哈希。
- 明文密码按原值匹配，不自动去除具有业务意义的首尾空白。

不要在日志、错误、文档示例或 API DTO 中暴露哈希，也不要依赖序列化绕过私有字段。

## 密码校验与旧数据迁移

`Secret::verify_password_or_dummy` 是同步、CPU 密集型能力，返回：

- `PasswordVerification::Current`：当前 Argon2id 密码匹配。
- `PasswordVerification::Legacy`：合法的旧 MD5 摘要匹配，需要升级。
- `PasswordVerification::Mismatch`：密码不匹配、凭证不存在或哈希格式损坏。

凭证不存在、未知格式和损坏的 Argon2 哈希都会执行 Argon2 dummy work 后失败；旧 MD5
路径也补充 dummy work，以缩小账号状态和哈希格式带来的时延差异。未知或损坏格式不会
降级为成功。

旧 MD5 只用于兼容验证。匹配成功后，Service 使用同一明文生成新的 Argon2id 哈希并先写回
数据库；持久化失败就不返回登录成功，因此不会继续签发基于未迁移凭证的身份。

## Async Service 边界

异步代码不能直接在线程池 worker 上执行 Argon2。当前共享边界位于
`services/src/auth/password.rs`：

1. 全局 `Semaphore` 把并发密码工作限制为 4。
2. 获取许可后把凭证和密码所有权移入 `tokio::task::spawn_blocking`。
3. 许可随阻塞任务生命周期持有，即使请求 Future 被取消也不会突破并发上限。
4. 后台账号和消费者认证都复用这条边界。

新增认证入口时应复用 Service 的有界校验能力，不要在 Handler 中直接调用同步密码哈希。
