# XXL-JOB 接入执行步骤

本地容器与开发配置执行 [本地 XXL-JOB 操作说明](../deploy/local/xxl-job/README.md)。

## 1. 前置条件

1. 独立 Worker 已完成发布与真实任务验收，API 固定使用独立 HTTP 进程，Worker 固定单副本 Recreate。
2. 核实实际调度中心为 XXL-JOB 2.4.1 或已完成同协议兼容验证的版本。本地基础设施 Chart 标注为 2.4.1，不代替线上版本核实。
3. 登记目标环境 admin URL、同一调度中心 access token、唯一 app_name；确认调度中心可访问 ERP 命名空间的内部 Service，Worker 可访问 admin URL。
4. 停止 Worker 后，使用相应业务库执行 `backend/scripts/migrations/20261030-xxl-scheduled-runs.mongosh.js`，核对唯一索引、回报索引及 TTL。应用启动仍执行同名幂等索引登记。

## 2. 创建执行器与任务

在调度中心建立执行器，AppName 与 Nacos 配置一致，注册方式选择自动注册。测试、生产不得使用同一 AppName。

下表任务可在 Worker 发布前或发布后创建，创建后保持停用。任务 ID 由调度中心生成，不需要写入 ERP 配置：

| 配置 | 履约补查 | 目录计划 |
| --- | --- | --- |
| JobHandler | supplier_tracking_scan | supplier_catalog_schedule |
| 运行模式 | BEAN | BEAN |
| 调度类型 | CRON | CRON |
| 初始 CRON | `0/5 * * * * ?` | `0/5 * * * * ?` |
| 路由策略 | 第一个 | 第一个 |
| 阻塞处理 | 丢弃后续调度（DISCARD_LATER） | 丢弃后续调度（DISCARD_LATER） |
| 调度过期策略 | 忽略（DO_NOTHING） | 忽略（DO_NOTHING） |
| 超时时间 | 0 | 0 |
| 失败重试次数 | 0 | 0 |
| 任务参数 | 留空 | 留空 |

目录 CRON 表示扫描触发频率；每个供应商连接是否到期仍由原同步政策控制。首期不支持分片、串行积压、覆盖执行、脚本、强制超时或强制终止。不得将任务参数填写为 `{}`。

## 3. 配置并发布

将以下片段加入对应环境 Nacos 的 ERP TOML。URL 和 token 必须替换为真实登记值；示例名称不代表当前集群实际服务名。

```toml
[xxl_job]
admin_url = "http://xxl-job.infra.svc.cluster.local:8080/xxl-job-admin"
executor_url = "http://erp-worker.test.svc.cluster.local:10001"
app_name = "erp-test"
access_token = "replace-with-admin-access-token"
```

1. `[xxl_job]` 仅包含上述四个必填字段；不得添加 enabled、tracking_job_id 或 catalog_job_id。控制台任务通过 JobHandler 选择固定能力，新增任务不需要重启 Worker。
2. 生产使用生产 Service 地址和独立 app_name。token 至少 16 字节，必须与调度中心一致。
3. 递增相应环境的 `api.configRevision` 并通过 Jenkins 发布，`WORKER_ENABLED=true`。不要只修改 Nacos 而不重启；运行进程不会热切换调度归属。
4. 核对 Worker rollout、内部 Service Endpoints、注册列表地址和进程日志。API 与 Worker 必须分别启动。
5. 确认旧 Worker 已退出、新 Worker 已接受固定 XXL-JOB 调度后，再启用调度任务。

## 4. 验收与结果解释

- `/run` 成功表示请求已持久化；调度中心的执行成功表示本轮扫描完成。ERP 任务和逐单业务结果仍以 ERP 记录为准。
- 对同一日志身份重复触发只能产生一个回执和一次业务执行；同一 jobId 繁忙时新日志请求必须被拒绝；不同 jobId 使用同一 Handler 时不得互相拦截。
- 中断回报网络后恢复，必须只补发回报；中断业务执行后重启，原 running 回执必须报告中断，禁止重跑原调度。
- 调度中心暂停期间，目录已有任务、导入导出、媒体归档、自动发单及回调记录处理必须继续运行。
- 检查 `scheduled_runs` 的 accepted/running 积压、completed 且 callback_done=false 的待回报记录，以及注册失败日志。仅健康探针成功不足以完成验收。
- 接入测试必须分别记录协议、数据库并发、业务结果和重启恢复证据；本地单元替身不能证明真实 Mongo 原子性和 XXL 控制台联通。

## 5. 回退与凭据轮换

先在调度中心停用任务，等待当前轮次结束和结果回报；存在遗留 running 或待回报记录时先核实，禁止清集合。回退目标必须仍要求 XXL-JOB；保留同一 app_name 和调度中心地址，更新配置 revision 后发布。禁止通过删除配置恢复本地周期调度。

不得将 app_name 或 admin_url 改指另一调度中心来绕过积压。切换调度中心前必须完成旧回报清理和业务核实。token 轮换必须同步调度中心和 ERP 配置并重启 Worker，真实 token 不进入 Git、任务参数或日志。

协议依据：[XXL-JOB 2.4.1 执行器路由](https://github.com/xuxueli/xxl-job/blob/2.4.1/xxl-job-core/src/main/java/com/xxl/job/core/server/EmbedServer.java)、[管理端回报接口](https://github.com/xuxueli/xxl-job/blob/2.4.1/xxl-job-core/src/main/java/com/xxl/job/core/biz/client/AdminBizClient.java)。
