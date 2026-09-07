# 阶段 17 Process 消费者实施核销

## 1. 输入与交付范围

实施树固定 `/private/tmp/erp-domain-crate-17-cutover`；输入提交 `a537414eb8f78c43ebc383a3457a45c437dececc`，对应源码提交 `72a0c79a2261d33699b869329376e534edfb1ef4`。主分片覆盖合同列明的 202 个非共享 Process 消费者文件；另按 root 明确补充授权完成导入确认复合响应修复，精确 11 个新旧文件见第 7 节。未修改共享 errors/lib/Cargo/test_indexes 或 E 的 adapters。

`cutover17-process-result.json` 固定 202 个输入 blob/修改后 SHA、2614 个原声明及逐文件符号替换。输入文件全部与接受提交一致；不得用本报告代替 root 的最终冻结提交和统一门禁。

## 2. 已实施的边界切换

| 原消费 | 目标实现 | 执行约束 |
| --- | --- | --- |
| services Error / Result，含 grouped use、别名、inline test | crate::Error / crate::Result | 保持别名、错误分支、收据恢复判定及调用位置 |
| services::workflow_compose | crate::adapters::workflow | 消费 E 单份实际 adapter，保留 factory 与 RBAC 读取时点 |
| services::identity_compose | crate::adapters::identity | 消费单份 shared_rbac_service |
| ignored 测试的 database::ensure_indexes | crate::test_indexes::ensure_indexes | root 保留原全局 19 domain 索引顺序；G 不缩减索引、不运行该 ignored 测试 |

202 个文件中的旧 services/database/entities 限定引用均已清零；未增加旧错误 façade 或反向领域 From。

## 3. 必要的读模型错误转换

旧 Read Model 与 Process 共用 services::Error；切换为独立错误类型后，在 22 个文件的 42 个函数中，53 个原尾返回或 early return 于原 `.await` 后增加 `.map_err(crate::Error::from)`。这些转换不是纯 namespace 替换。root 提供穷尽的 `From<erp_read_models::Error>`；G 不扩大原七个错误降级函数。

| 业务目录 | 显式转换点 |
| --- | --- |
| `finance_posting` | 9 |
| `fulfillment_execution` | 2 |
| `order_to_cash` | 8 |
| `procure_to_pay` | 2 |
| `reverse_flow` | 28 |
| `sales_change` | 4 |

`cutover17-process-rm-bridges.json` 逐项保存文件、函数、原行号、原表达式及新表达式。所有转换保持原读取、权限、事务、收据 replay 和首错位置；原 `.await?` 消费点继续使用 root 的同一 From。check4 报出的客户验收两处原 replay early return 已同样桥接，保持首次 replay 及事务错误恢复时点；修复后按 202 文件扫描支持换行的 read/reads/read_model receiver，未发现未转换的原 `.await` 返回。

原 `commit_customer_receipt` 的 `Some(receipt_id) => return ...` 分支因表达式变长被 rustfmt 包成单一 block；返回表达式、匹配条件和控制流不变。该格式变化在函数 token 核对中只对此一个精确分支登记。

root 独占的 `sales_change/mod.rs::sales_change_order_detail` 同类尾返回已单独交 root，不计入 G 的 53 点。

## 4. 私有步骤名称

按 root 明确授权，将 `order_to_cash/formalization_posting.rs` 的私有 `PostingStep::Document` 改为 `RegisterDocument`，共 4 个 token：声明、真实执行数组、provider match arm、测试 `expected()` helper。原十步执行顺序、同 Executor 和失败停止保持；未放宽 boundary scanner。机器登记在 `cutover17-process-posting-rename.json`。

## 5. 原行为与静态验收

- 7 个特殊错误/事务函数全部仅 namespace token 变化：approval 两个降级 mapper、workflow mapper、inventory 两个 mapper、采购责任首错映射、run_audited。具体原分支和前后函数 SHA 已记录；Rbac/Coded 等原降级范围未扩大。
- 375 个原测试函数的函数体、断言与名称在 namespace 归一后全部 token 相等，1 个原 ignore 保留；主 202 文件未新增测试，补充导入修复新增 3 个测试在新测试叶单列。私有测试 `expected()` helper 的唯一变体 token 改名另行登记。
- 294 项原 include_str/include/path 引用全部保留且目标存在；未改历史 tests 目录。
- 从接受输入重新生成独立预期文件：固定 namespace 替换、定向 rustfmt、53 个精确转换、4 个私有改名 token、再次定向 rustfmt。再验证 3 个 Import 叶与补充修复输入快照相等，并应用明确许可的响应类型与授权路由修复；202/202 实际文件逐字节相等。其余文件不作值、字符串、分支或断言归一。
- 42 个包含转换的函数另核对完整原函数与新函数 token，除登记的错误转换及一处 rustfmt block 外无其他差异。
- own 202 文件定向 rustfmt --check 与 git diff --check 已通过；G 未运行 Cargo、Clippy、DB、外部 I/O 或历史 tests。统一编译、运行与边界门禁由 root 执行。

## 6. 复现与冻结要求

1. 执行 `/private/tmp/cutover17-process-reconstruct.py`，验证接受输入加逐项许可变换等于实际源码。
2. 执行 `/private/tmp/cutover17-process-result.py`，重核每个输入 blob、实际 SHA、函数 token、原测试与 include。
3. root 完成统一门禁并冻结 source 后，将本 JSON 的 after 文件 SHA 与最终提交 actual blob 重绑；期间任何 G 文件修复必须重新执行以上两项。

机器结果：`/private/tmp/cutover17-process-result.json`。
当前结果 SHA-256：`08a1c14aad927aaa9d14fc5eb87e0b408c723294b6d0c134389c553c504da471`。

## 7. 明确许可的继承行为修复

`cutover17-import-view-repair.{md,json}` 独立绑定阶段 00、16 和当前树：3 个导入复合响应移到 Process；恢复唯一 Workflow 类型/状态；授权投影消费原 Workbench 路由，raw/read-only 保留原 W18。涉及主清单 3 个叶及额外授权 DTO/导出/HTTP/route wrapper 文件。此项是实际业务投影恢复，不归一成命名空间变化；主 JSON 直接引用补充报告 SHA。新增 3 个真实投影测试独立计数，原 375 个主分片测试仍全部保留。

## 冻结源码与统一门禁观察

- 最终 source：`39c55021d8bb1d030eade4afb3e135e4b5a8a18b`。本报告 202 个 after 文件均从实际 Git blob 读取，原证据 SHA、当前工作树与冻结 blob 三方逐字节一致。
- root 执行全 fmt/check/strict Clippy/lib 并报告退出成功；G 读取并哈希封存日志，没有重跑门禁。库测试日志逐 package 汇总为 **3655 passed / 0 failed / 68 ignored**，3 个新增 Import 投影测试均有实际 `... ok` 行。
- 日志：`/private/tmp/erp-cutover17-lib-tests-sealed.log`、`erp-cutover17-clippy-sealed.log`、`erp-cutover17-check-sealed.log`；精确 SHA 与观察边界见 JSON 的 `source_freeze`。
- 本轮只更新 /tmp 证据；没有修改仓库源码、执行 Cargo/数据库或全仓扫描。
- 本 JSON SHA-256：`08a1c14aad927aaa9d14fc5eb87e0b408c723294b6d0c134389c553c504da471`。
