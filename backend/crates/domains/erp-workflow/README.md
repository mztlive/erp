# erp-workflow：审批集成与工作项

将通用流程引擎接入 ERP，管理业务审批、审批参与者、业务单据关联和人工待办任务。

流程引擎能计算节点变化，但 ERP 还需要审批政策、账号资格、单据绑定、数据库记录和通知。本 crate 负责这些 ERP 层面的流程与任务能力。

## 使用场景

- 修改审批定义、运行、ERP 审批政策或通知待发送记录。
- 修改工作项创建、关闭、转派，以及单据参与者和关联。

## 协作示例

采购单提交审批后，[bpm](../../platform/bpm/README.md) 计算流程下一步，本 crate 持久化审批并管理工作项；审批通过后的采购生效由 [erp-procure-to-pay](../../orchestration/erp-procure-to-pay/README.md) 调用采购领域。任务列表、详情和统计统一由 `WorkItemReadService` 提供；跨业务审批材料和履约队列由 [erp-read-models](../../read-models/erp-read-models/README.md) 提供。

## 负责的数据与能力

- ERP 审批政策、审批定义与运行服务、通知 Outbox 和 BPM 持久化适配。
- 采购付款申请使用 `purchase_payment_request` 独立审批身份；驳回终止实例并调用领域释放占用，不创建下一轮审批。供应商实际付款记录使用 `supplier_payment` 且不重复审批。
- 付款执行任务只能绑定已批准付款申请，按本申请未付额度更新和完成；旧 `payable_account` 直付任务不能获得执行权限。
- 业务单据登记、参与者与关系，以及工作项创建、关闭和转派。

## 依赖与协作边界

术语与统一分工见[总索引](../../README.md#代码中常见名称的含义)。

- 本领域的数据结构、业务规则、服务和数据库仓储在此维护。允许依赖 identity 的公开身份和授权能力；不得依赖其他业务领域或组合层。
- 流程图和纯状态迁移规则复用 bpm；账号资格、单据事实和领域动作通过 Port 取得。
- 任务查询、授权范围和分页归本 crate；直接调用 identity 的公开服务。跨域审批动作由 web-api 分派，业务材料读取归 erp-read-models。
- 需要其他领域的能力时，由组合层或应用将本域接口接到实际提供方；公开入口见 [src/lib.rs](src/lib.rs)。

## 代码入口

| 入口 | 用途 |
| --- | --- |
| [src/service/approval/mod.rs](src/service/approval/mod.rs) | 审批定义、运行与政策入口 |
| [src/service/approval/read.rs](src/service/approval/read.rs) | 冻结定义与有序启动回执读取；收据命中后仍须核验精确命令身份 |
| [src/service/approval/execution/start_input.rs](src/service/approval/execution/start_input.rs) | 节点绑定与启动输入装配；业务入口须先完成定义版本、命令身份及专属资格检查 |
| [src/service/approval/execution/cancel_prepare.rs](src/service/approval/execution/cancel_prepare.rs) | 普通单据撤回事实读取与输入构造；全程沿用调用方 Executor |
| [src/service/work_item/mod.rs](src/service/work_item/mod.rs) | 工作项命令入口 |
| [src/repository/bpm.rs](src/repository/bpm.rs) | BPM 持久化适配 |
| [src/entity/mod.rs](src/entity/mod.rs) | 本域实体、值对象和确定性规则 |
| [src/repository/mod.rs](src/repository/mod.rs) | 本域 MongoDB 仓储与集合访问器 |
| [src/indexes/mod.rs](src/indexes/mod.rs) | 公开索引注册入口 |

## 修改执行要求

1. 将无 I/O 的校验、状态迁移和不变式放入本域实体或值对象；Service 组织本域用例。
2. Repository 使用调用方传入的 `persistence_core::Executor`；跨集合原子写入由本域用例或 Process 控制事务。
3. 新增或调整集合查询时同步评估索引；组合根复用本域公开索引入口，保持既有逐集合注册顺序。
4. HTTP 请求和响应优先复用本域 DTO；扩展公开合同须同步检查 Process、ReadModel 和应用调用方。
5. 业务改动补充本域库单元测试，覆盖成功、失败、边界及相关幂等或版本冲突路径。

## 验证要求

以下命令在 `backend/` 目录执行：

```bash
env -u ERP_TEST_MONGO_URI cargo test -p erp-workflow --lib --locked
```

代码变更还须执行[统一质量门禁](../../README.md#质量门禁)。测试范围限定为库单元测试，不执行集成测试或真实外部服务测试。

## 任务查询与关联链路

- 列表、详情、统计和查询 DTO 使用 `service/work_item/read` 与 `dto/work_item`；调用方保留固定 Future 边界。
- 处理人、来源销售单和来源采购单筛选只接受稳定 ID。姓名仅按已授权页中的账号 ID 批量补齐，不参与任务关键词匹配。
- 来源筛选使用 `WorkItem.links` 的类型与 ID 配对引用；同字段取并集，不同字段取交集，不授予关联单据读取权限。
- 创建任务及改变业务版本时，写流程须在原事务写入完整上级链路。审批任务须继承对应提交快照，后续节点和驳回处理不得从当前草稿重建历史链路。
- 发布前按 [任务链路迁移合同](../../../docs/work-item-links-migration.md) 完成历史数据审计与回填。

## 任务统一授权

任务权限通过 `AuthorizationService` 执行。`WorkItemReadService::new` 装配固定的 `work_item:list`、`work_item:detail`、`work_item:manage` 登记，调用方不得改变政策或 Mapping。

| 入口 | 授权执行 |
| --- | --- |
| 列表、检索、统计、焦点任务 | `query` → WorkItem Mapping → 数据库授权过滤 → 计数与分页 → `validate` |
| 单条详情、业务模块检查任务可读性 | `require` 检查精确任务 ID |
| 转派、关闭、放弃返工、替代任务、幂等回放 | `WorkflowAuthorizationPort::require_task` 接入相同对象授权；写事务内再次检查 |
| 管理按钮 | 在当前展示批次内查询 `manage` 授权任务 ID，不以读取资格替代管理资格 |
| 履约聚合队列 | 从 `ActorAccess::read_access` 取得完整约束，先授权任务，再关联业务事实、计算指标和分页 |

映射合同：

- `Owned` 对应任务当前 `owner_user_id`；组织范围对应该负责人在授权时点的唯一有效主属组织，不使用 `owner_organization_id` 代替。
- 根对象必需事实为负责人和主属组织。已确认没有值时返回 `FactValue::Known(None)`，不能以未读取值代替。
- 任务列表不增加历史参与或记录读取授权分支。Mine、Managed、History 视图均先通过 list 范围，再追加视图筛选。
- 任务来源材料的读取资格、授权就绪状态和业务办理条件分别检查，不参与本人待办计数。
- 审批管理、绑定升级、财务负责人和任务执行的完整功能资格必须通过 `QualifiedPermissions::same_role`；不得把不同角色的逐项资格拼成完整操作资格。
- `TaskAccess` 只携带当前展示批次中，由 identity 管理对象结果及完整执行、审批功能资格生成的精确任务 ID；这些展示结果不能替代命令授权重验。动作投影不得保存角色权限集合或重新解释创建人、历史参与者与负责人读取规则。
- 任务用例只在统一对象授权之后核验业务状态、冻结责任行、来源存在性及主体版本。
- 查询 Mapping 只在调用方事务中编译和执行。事务内使用同一授权时点、组织版本和有效期边界；跨页版本包含读取及管理查询依赖。
- 组织关联复用 identity 提供的查询片段及成员索引，按任务负责人查找当前关系，不预加载全部负责人名单。重复有效主属关系会被查询排除；对象检查返回事实错误。

实现位置：[WorkItem Mapping](src/repository/work_item/authorization.rs)、[固定登记与统一调用](src/service/work_item/read/identity.rs)、[对象事实端口](src/service/work_item/authorization/facts.rs)。
