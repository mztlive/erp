# 阶段 16 供应商履约实施交付合同

## 1. 输入与归属

- 唯一实施树：`/private/tmp/erp-domain-crate-16-supply`。
- 原始源码：`ed8015e28d36e5b9323b87e1a1fd5f63c122eb70`。31 个合同源文件已与该提交逐字节核验，完整输入账本为 `/private/tmp/supply16-g-input.json`。
- 原 23 个 source-map 文件、7 个 owned 文件及阶段 11 详情叶全部闭合；旧 30 个实现文件已删除，详情叶原地迁接。不保留旧 Service 或兼容 façade。
- A 的 17 个实体/持久化文件交付见 `/private/tmp/supply16-fulfillment-persistence-result.json`；G 的 32 个新建/改接文件、逐文件 SHA、原测试和新测试见 `/private/tmp/supply16-fulfillment-result.json`。
- 所有全局 Cargo、lib、HTTP、AppState、错误及共享索引注册由 root 独占。D 独占统一失败分类与双向转换；C 独占唯一任务授权 ReadPort 与生产 adapter。

## 2. 必须保留的实际出口

| 消费方 | 唯一出口与调用方式 |
| --- | --- |
| 普通列表 | `erp_supply::service::supplier_fulfillment::SupplierFulfillmentService::new(db).supplier_fulfillment_order_list(&params)` |
| 跨域八个写入口 | `erp_processes::supply_execution::SupplierFulfillmentProcess::new(db, gateway)`；原 `submit_place/submit_cancel/submit_refund/record_reject/record_refund_result/investigate_order/investigate_order_task/complete_order_task` 名称和业务入参保持 |
| 详情 | `erp_read_models::supplier_center::SupplierFulfillmentDetailReadService::new(db, domain_service)`；原详情末参接 `&dyn WorkItemAuthorizationReadPort` |
| 本域 DTO | `erp_supply::dto::supplier_fulfillment`；结算复用原 `normalize_sort/PageParams/SortDir/PageView` |
| 三类强命令响应 | `erp_processes::supply_execution::dto::{SupplierOrderInvestigationWorkItemView,SupplierOrderInvestigationResultView,SupplierOrderTaskCompletionResultView}` |
| 详情 DTO | `erp_read_models::supplier_center::fulfillment_dto::SupplierFulfillmentOrderDetailView` |
| 网关 Port | `erp_supply::ports::supplier_gateway::{SupplierGateway,DispatchOutcome,InvestigationOutcome}`；方法没有 Executor |
| 实际网关实现 | `erp_processes::adapters::supplier_fulfillment_gateway::{UnavailableSupplierGateway,SimulatedSupplierGateway}`；默认仍失败关闭 |
| 失败事实 | `erp_supply::entity::failure::SupplierFailureClass`；Process 使用 D 的 `adapters::supplier_failure::integration_class` 进入唯一 integration 重试政策 |
| 数据入口 | `erp_supply::repository::SupplierFulfillmentExt`，叶 `repository::supplier_fulfillment::SupplierFulfillmentExt` 同时可达 |

## 3. 逐文件责任

| 原叶 | 领域实际实现 | Process / Read Model 实际实现 |
| --- | --- | --- |
| `query.rs` | 两个原列表/订单加载方法 | 详情继续调用本域订单加载 |
| `mapping.rs` | 原状态转换、动作行、退款头/分配视图映射 | 命令与详情共用唯一映射 |
| `place.rs` | 连接与供给校验、订单/明细/PLACE 构造、普通派发本域分支、创建与结果 CAS 写入 | 意图事务、外呼、inbox/error/audit；`dispatch_writes.rs` 执行同 Executor 三组写入，`work_item.rs` 唯一构造实际 W26 |
| `cancel.rs` / `refund.rs` | 动作头/行、归属检查、Pending 状态、本域写入 | 原动作回放、事务及外呼；退款仍转公共售后根 |
| `reject.rs` | Rejected 推进、历史构造、最新 PLACE 查询与 Failed 更新、三项本域写入 | 原回调回放、审计与唯一事务 |
| `refund_result.rs` | 原两次财务聚合、上限、退款状态、事实/分配构造与写入 | 原回放、inbox/audit；`refund_writes.rs` 按原顺序复用同 Executor |
| `investigate.rs` | 五字段 `InvestigationSubject`、intent/prepared/evidence、所有纯调查/重放规则、证据和订单动作写入 | intent/prepare/durable/final 分段，外呼、真实授权、责任更新、audit receipt；`execution.rs` 只复用已存在分支语义 |
| `complete.rs` | 完成证据解析、关联优先的当前终态复验、terminal action 构造 | WorkItem 授权与完成、跨域事务、一次回执恢复 |
| `receipt.rs` | 原正版本解析、完整 JSON 指纹及稳定 ID/key | 两类原文本收据、解析与错误顺序 |
| `gateway.rs` | trait 与 tagged outcomes，唯一失败事实 | 两个原生产实现与六个原测试 |
| `dto.rs` | 四个明确迁出结构以外全部原声明、实现和六测试 | 三个响应与一个详情只各有一份 |
| 原详情叶 | 消费本域 DTO、证据只读 getter、仓储 | C 的授权事实替代完整可写服务；保原授权调用、active 查询、raw 重读、blocker 顺序 |

`InvestigationEvidenceRecord` 原字段权限保留；Process 所需构造与 operation ID 只读方法均为窄出口。原 `investigation_intent_record` 继续私有。`ensure_task_actor_eligible` 唯一落 RM `fulfillment_access`，保原无操作 body；实际 workflow 授权仍在原位置执行。

## 4. 事务、幂等与首错约束

1. 普通下单、售后、退款回调命中原身份即返回原事实；不新增 payload、路径订单绑定、连接启用或业务余额检查。普通事务错误不新增回执恢复。
2. 普通调用必须先成功提交意图，再进入无 Executor 的网关方法。普通结果写入顺序为订单/动作 CAS → 集成消息/错误 → W26 正式责任；退款为 inbox → 订单 CAS → 退款事实/分配 → audit。各组复用调用方同一 Executor，任何错误立即停止。
3. W26 ID 仍在结果事务前生成。工厂仍使用真实 `WorkItem::new`，绑定订单、当前版本、`role-procurement/company/当前人`、`SystemRule/High/None` 及原原因和摘要。先取首个同类型活动任务，只有 subject 不同才刷新与审计。
4. 普通成功缺单号仍接单；重放成功缺单号保持未知。普通 Failed 消费 integration 派生的 retry bool，重放 Failed 不消费该政策，原 `record_attempt(None)` 与未知结果保持。
5. 调查 intent 先落地；冻结结果命中跳过外呼；新结果单独持久化后才能进入最终 CAS。只有最终事务错误触发一次 fresh receipt 回读，准备阶段错误原样退出，回读自身错误仍优先透出。
6. 完成命令保持任务校验与真实授权先于订单/证据读取；业务证据核实后才完成 WorkItem。收据重复字段优先于指纹，指纹优先于缺字段，未知字段仍可接受；调查收据任务版本的原 Validation 分类不并入其他版本的 Internal。
7. 详情仅替换授权接口和四个平铺字段；原 `|| false`、当前名称缺失错误、原 no-op、raw WorkItem 重读及各 blocker 不变。

## 5. 测试与证据验收

- 原测试 79 个、ignore 0：A 的实体 49、仓储 11、索引 6；G 的 DTO 6、网关 6、durable intent 1。A 218 个生产函数和 66 测试体已核销；G 13 原测试体全部在唯一新归属保留。
- G 新增 17 个测试定义：意图提交/失败停止 2、派发结果同 Executor/各步失败停止 2、退款写入同 Executor/各步失败停止 2、普通与重放差异 3、真实 W26 工厂字段/原校验 2、文本收据与首错 3、durable 复用/准备错误/final 单次恢复 3。测试驱动实际生产 helper 或 runner，不引入旁路事务模型。
- `/private/tmp/supply16-g-semantic-review.py` 检查 95 项完整函数、DTO、证据声明及限定的 Subject/Inbox ID/ReadPort 适配，当前全部 token 等价。公开可见性、明确 provider 路径和 rustfmt 之外，不归一状态、错误文案、serde、金额、索引或执行次序。
- 拆分方法的旧主要符号与新消费者、八类序列合同写入 JSON；这些记录不得表述成 scanner 自动证明了任意拆分。C 的独立语义审核与 root 的原始合同扫描继续作为共同验收。
- 7 个集合、16 个索引、原 ensure 顺序、Decimal128、3 笔创建与 2 笔退款复合写入的证据归 A；索引入口由 root 统一注册。
- G 已执行定向 rustfmt 和 `git diff --check`。观察 root `/private/tmp/erp-supply16-check-4.log` 的 workspace check 完成；G 不运行 Cargo、MongoDB、外部供应商或历史集成测试，不提前声称 Clippy/单元测试/边界门禁通过。
- 最终 source commit 已绑定 `72a0c79a2261d33699b869329376e534edfb1ef4`，32 个 after 文件逐个 Git blob SHA 均核验一致，95 项对照仍全等。root 已报告全部库测试 exit 0：3594 passed / 0 failed / 68 ignored、36 packages；strict Clippy、领域边界与权限门禁通过。G 没有代替 root 执行这些命令。`/private/tmp/supply16-g-result-complete.py` 会重核冻结 blob 后生成最终 JSON。
