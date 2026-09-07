# 阶段 16 Offering 实施与验收合同

## 1. 输入及文件核销

- 实施输入：`ed8015e28d36e5b9323b87e1a1fd5f63c122eb70`，唯一树 `/private/tmp/erp-domain-crate-16-supply`。
- 原 13 个模块叶与 4 个 owned 准备叶已实际迁移并删除；没有修改历史 `tests/**`、catalog 混合供给 pipeline、共享 Cargo/lib/mod/HTTP/AppState 或工作台实现。
- [输入重绑](/private/tmp/supply16-offering-input-rebind.json) 核对 17 个输入文件与预备合同快照，全部零漂移。
- [当前逐符号/测试/声明证据](/private/tmp/supply16-offering-implementation-evidence.json) 核销 189 个原生产函数，175 个函数体在规范 owner 路径后相同；其余 14 个函数通过真实领域/流程/读模型及 production Port 拆分，具体差异见 [函数体对照](/private/tmp/supply16-offering-body-diffs.txt)。
- 32 个当前新增/修改叶的 SHA-256 和原位置均已记录；[after 路径清单](/private/tmp/supply16-offering-after-paths.txt) 与 `/private/tmp/supply16-offering-after-snapshot` 用于后续格式化、门禁修复及最终提交封存前逐字核对。当前32个after文件已逐字核对固定源码提交 `72a0c79a2261d33699b869329376e534edfb1ef4` 的 commit blob、原 after-snapshot 与真实工作树，三者及 SHA-256 全部一致。源码封存不代表全部测试已经完成。

## 2. 已实现的公开合同

| 所有者 | 实际路径与出口 | 调用合同 |
| --- | --- | --- |
| supply 实体 | `erp_supply::entity::supplier_offering::*` | 原四实体、data、write_data、状态/影响枚举唯一实现。 |
| supply DTO | `erp_supply::dto::supplier_offering::*` | 原写请求/结果、条款和强类型任务决定；指纹与转换同源。 |
| supply 仓储 | `erp_supply::repository::{SupplierOfferingExt,owned::SupplierOffering*Repository}` | 四 owned 类型及原专用 inherent 方法；无外域实体/集合引用。 |
| supply 本域用例 | `erp_supply::service::supplier_offering::SupplierOfferingService::new(Database)` | prepare_create/prepare_revise/prepare_availability；persist_created/persist_revised/persist_availability；原命令恢复 API。 |
| supply 资格端口 | `erp_supply::ports::offering_qualification::QualificationPort` | 传 supplier_id、sku_id、BusinessDate 与 caller Executor；associated Error 保持原错误类别。 |
| process 命令 | `erp_processes::supply_governance::SupplierOfferingProcess::new(Database)` | 原 create/revise/update_availability/complete_supply_exception_task 参数顺序与结果。 |
| process 资格适配 | `supply_governance::offering::MongoOfferingQualification` | 默认生产 adapter；构造无事实读取，with_qualification 可替换生产 Port。 |
| read model | `erp_read_models::supplier_center::SupplierOfferingReadService::new(Database).list(&params)` | 真实 list 根；查询/展示类型只在 supplier_center::offering::dto 定义。 |
| read repository | `supplier_center::repository::offering::SupplierOfferingReadRepository` | 真正承接 resolve/search/load_bundle/load_page 与六组外域展示批读。 |
| supplier 政策 | `erp_supplier::entity::supplier::eligibility::{OfferingProductKind,required_offering_capability}` | 四种商品事实至三种能力的唯一政策。 |
| supplier 资格加载 | `erp_supplier::service::supplier::eligibility::{ensure_capability_qualified_with_executor,ensure_offering_capability_qualified}` | 复用原 supplier 纯资格规则；旧 ensure_capability_qualified 保持 NoTransaction wrapper。 |

共享 roots 与旧根清理由集成负责人登记；本片已提交精确入口。阶段 16 按集成负责人合同继续使用现有 `services::workflow_compose::work_item_service` 与 `services::identity_compose::shared_rbac_service` 两个真实工厂。阶段 17 再统一迁移 composition；本片没有预先指向尚未实现的组合入口。

## 3. 领域与流程的真实职责

领域 prepare 仍执行原 validate → fingerprint → command replay，再读取本域身份、source connection、当前最大修订、版本与条款，按原位置调用 QualificationPort。准备结果为明确的 Replay 或 Apply；库存/可供更新不增加资格读取。领域自行拥有实体准备、不可变修订、可供 apply、响应和命令构造。

领域持久化三条实际生产 runner 在 `service/supplier_offering/write.rs`；其调用 `repository/supplier_offering/write.rs` 的唯一 `MongoOfferingWrite`。既有 aggregate repository 与新 command runner 共用 create_triple/append_revision，未复制第二份 Mongo 写序。

| 命令 | 实际顺序与生成时点 |
| --- | --- |
| create | 原 prepare 构造 offering/revision/availability/result/command；事务内 offering insert → revision insert → availability insert → command insert → audit insert。 |
| revise | 原 prepare 构造新修订、资格、status/pointer、expected_version与audit；事务内 revision insert → offering CAS → result → command ID/实体 → command insert → audit insert。 |
| availability | 原 offering exists → availability exists → optional version → now/source time/data/apply；事务内 availability CAS → result → command ID/实体 → command insert → audit insert。 |
| exception completion | 原 task/type/object/reason/Open/version/subject → 真实workflow授权 → complete/两个audit构造 → task CAS → decision audit → receipt audit。只完成任务。 |

process `commit.rs` 的实际 Mongo providers 顺序执行领域持久化与 audit，三个命令均真正调用此 runner。exception 内 `MongoCompletionWrite` 真正执行原三个写入。所有写入使用 process transaction 传入的同一 Executor，prepare/replay/recovery 保持原事务外调用位置。没有网络外呼或新发送行为。

## 4. 资格与恢复的展开合同

资格六次实际数据库读取固定为 SKU → Product → capability/current pointer → SupplierAccount → capability revision → capability。process `qualify` 保持 SKU missing/disabled 在 Product 和供应商读取之前；`MongoQualificationFacts` 的第二次 capability 读取按 revision.code 执行，未缓存第一次读取。供应商 disabled 仍在最后三次读取之后由原 pure eligibility 判定。

create 在首版修订后无条件资格校验；revise 仅 next_status Active 校验，BusinessDate 取新 revision.valid_from。Paused/Stopped、availability 与 exception 不补资格。原人为关闭的 linked qualification 检查及注释仍保留；未恢复资质档案拦截。

事务恢复实际实现位于 `service/supplier_offering/recovery.rs`：成功不读 command；任意事务错误只重读一次原 raw key；有记录先 operation/fingerprint，再原 result_json 解码；无记录返原 transaction error；重读错误通过原 `?` 优先覆盖 transaction error。两个原恢复方法委派同一个 runner；没有新增提交重试。exception 自有 CommandReceipt 回放仍按原流程执行，未增加回放授权或当前版本检查。

## 5. 列表与数据合同

- availability/keyword/code 候选解析始终在分页前；keyword 保持 supplier code OR SKU IDs，code helper 保持 product_no/sku_no 的交集及 None/Some(empty) 差异。
- `SupplierOfferingFilter::to_doc` 保持原同名字段覆盖：精确 sku_id 之后的 sku_ids extend 覆盖该字段；没有改成新的 `$and`。新增纯测试冻结非空/空候选的原覆盖行为。
- 原单字段排序不增加 id 次排序；原分页 rows 后 count 的顺序保持。
- 当前修订装载沿 row.current_revision_id；SKU 名称与供应商名称沿 SKU/Party 当前 pointer；缺失时保持 None/空集合，未回退最新或历史修订。
- 外域六组展示事实只在 read repository，所有读取沿同一 executor 并按原顺序批量执行；本域没有完整 catalog/supplier/party 实体或外域集合常量。
- 原 28 个实体与 DTO public struct/enum 声明，包括字段 serde 属性，在 owner 路径规范化后全部 token 相同；金额/税率/数量类型、nullable 字段、BSON flatten、三种 fingerprint 原 golden 测试均保留。
- 索引构造函数本体与输入相同，四集合名称、十个索引键/名称/唯一性、无 soft-delete partial 保持。没有执行索引创建或数据变更。

## 6. 测试与当前验证层级

原 **52 个 inline test 全部恰好一份，3 个原 ignore 保留**。49 个原非 ignore 测试体在规范 owner 路径后相同；3 个 ignore 的 fixture 只改为实际新 RM receiver 和供给 indexes 入口，断言及原 ignore 条件保留。

原混合 BSON 兼容测试 `fingerprint_versioned_and_legacy_are_compatible` 完整迁至 `repository/supplier_offering/command_tests.rs`，由本域仓储叶 `#[cfg(test)] mod command_tests` 注册。原测试体、摘要常量和 fixture 数据/构造全部保留；其余 6 个纯 command 实体测试留在实体中。实体源码不再直接使用 BSON，持久化往返断言只在仓储测试层。

新增 **16 个**真实生产 Port/规则测试：

| 覆盖范围 | 新增测试数 | 生产实现与断言 |
| --- | ---: | --- |
| supplier 类型与资格读取 | 5 | 四种 kind 映射；原四次 supplier 读取；非 ZST Executor；逐步错误；缺指针短路；disabled 读后判定；第二 capability 不缓存。 |
| process catalog 资格 | 3 | 原 SKU/Product/supplier 组合顺序；每步错误；SKU missing/disabled、Product missing；全部 kind variant。 |
| supply 三命令写入 | 2 | create四项、revise三项、availability两项本域写序；非 ZST Executor；每步 storage failure 之后停止；原 result version 与 source time。 |
| process domain/audit | 1 | 实际 CommitPort 的 domain→audit，非 ZST Executor，原结果与两步失败传播。 |
| process task/audit | 1 | 实际 CompletionWritePort 的 task→decision→receipt，同 Executor及三步错误停止。 |
| supply command recovery | 3 | 成功无重读；任意错误恢复；raw key；无记录返原错；重读错误优先；异operation与损坏结果的原错误。 |
| supply list filter | 1 | 精确 SKU 被编号候选覆盖的原实际 BSON 行为。 |

这些测试调用实际生产 runner，其 Mongo provider 已展开并记录到符号证据；没有以独立模拟步骤列表代替生产调用。

本分片只执行定向 rustfmt、source/schema/test 映射和 owned git diff --check。集成负责人已报告 clippy5、domain、permissions 门禁 exit 0；lib tests 仍在运行，未登记最终测试结论。集成负责人统一运行 Cargo 与全量门禁；当前不得把源码入口存在写成测试通过。真实数据库运行未验证。

## 7. 最终封存要求

1. 集成负责人给出实际 gate 日志后，逐项修复本分片真实错误，不修改业务算法补测试期望。
2. 每次 source hash 改变先与 after-snapshot 比较；注释/排序/测试修改与生产体修改分别登记，不能直接替换 hash。
3. 最终提交前重新生成符号、52原测试、28声明及当前16新增测试核销；原17删除状态与32after文件hash必须全部匹配真实工作树。
4. 仅在收到固定验收提交并验证当前文件与该提交字节一致后填写 source_commit。编译、替身测试及schema比对不代表真实Mongo回滚/并发行为已验证。
