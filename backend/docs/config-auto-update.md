# 配置自动更新（Nacos）流程说明

本文说明当前系统在使用 Nacos 配置中心时，配置自动更新的触发方式、数据流向，以及 Web API
对数据库配置变化和 JWT 密钥更新的处理策略。

## 适用范围

- 仅在启动参数启用 `--enable-nacos` 时生效
- 本地 `config.toml` 不会自动热更新

## 核心机制概览

- `NacosConfigWatcher` 定时调用 `NacosConfigClient::fetch` 拉取配置
- 拉取成功后写入内存配置并广播变更事件
- 调用方通过 `SafeConfig::snapshot` 获取当前内存配置快照
- Web API 订阅配置变化：数据库连接变化要求重启，JWT 密钥变化继续热更新

## 文本流程图

```text
[Nacos 配置中心]
        |
        | (每 10 秒拉取)
        v
[NacosConfigWatcher::start]
        |
        | NacosConfigClient::fetch()
        | reload_from_nacos()
        v
[SafeConfig]
  - watch::Sender 原子替换配置并广播变更
  - snapshot() 返回当前快照
        |
        v
[Web API 配置订阅器]
        |
        | 比对 database.uri / db_name
        | 比对 app.secret
        v
  +------------------------------------------+
  | 若 DB 变化:                              |
  | 1. 保持启动时的 Database 与 RBAC         |
  | 2. 记录 restart_required 结构化告警      |
  | 3. 重启应用后才使用新的数据库配置       |
  +------------------------------------------+
        |
        | 若 app.secret 变化
        v
  +------------------------------------------+
  | invalidate_jwt_engine()                  |
  | 下一次请求使用新密钥构建 JWT 引擎       |
  +------------------------------------------+
```

## 详细步骤

1. `NacosConfigWatcher` 每 10 秒拉取一次配置。
2. `NacosConfigClient::fetch` 成功后调用 `SafeConfig::reload_from_nacos`。
3. `SafeConfig` 更新内存配置并通过 `watch::Sender` 广播变更。
4. Web API 订阅配置变更并进行差异检查。
5. 如果 `database.uri` 或 `database.db_name` 变化，继续使用启动时的数据库与 RBAC，并记录需要重启。
6. 如果 `app.secret` 变化，使 JWT 引擎缓存失效；下一次请求使用新的配置快照重建引擎。

## 数据库启动配置策略

- **触发条件**：`database.uri` 或 `database.db_name` 发生变化
- **运行时行为**：
  1. 保持启动时数据库连接与运行时状态不变
  2. 业务 Repository 与 Casbin RBAC 继续使用启动时的同一数据库
  3. 输出结构化告警，其中 `restart_required = true`，并分别标记 URI 或数据库名是否变化
- **生效方式**：重启应用后，新的数据库配置才会用于构建 Database 与 RBAC
- **配置回退**：配置恢复为启动值时记录 `restart_required = false`

## JWT 热更新策略

- **触发条件**：`app.secret` 发生变化
- **执行步骤**：调用 `AppState::invalidate_jwt_engine` 使 JWT 引擎缓存失效
- **效果**：下一次请求会使用新密钥构建 JWT 引擎

## 不会自动更新的内容

- 数据库连接 `database.uri` 和 `database.db_name`
- 监听端口 `app.port`
- 日志配置
- 已经启动的业务组件初始化参数

这些内容需要重启进程才会生效。
