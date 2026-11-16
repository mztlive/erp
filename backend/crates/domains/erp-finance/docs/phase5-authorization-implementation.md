## FSY-7 第五阶段授权检查：第一轮实现交接

FIN-AUTH-01、FIN-AUTH-02 已完成修复，交独立 Review。原始逐入口、权限、Facts/Mapping 和装配检查表保留在同目录 `phase5-authorization-analysis.md`；该文档是第一轮分析历史，本轮处置以本交接为准。此前尚无 Review 问题清单。

### 版本与交付

| 项目 | 记录 |
| --- | --- |
| 日期、环节、轮次 | 2026-11-16，后端开发，第一轮 |
| 任务起始提交 | `ce7c3c32187a295418fbf25c03f24f358ec42ef7` |
| 分析基准 / 报告提交 | `82706febcc048fe250772eb8d3a047ad017ec3d6` / `ad6fb89cabbd79aa84541c8d24b6cbc7980814a7` |
| 基准核对 | 分析基准与任务起始提交之间，finance crate、identity 现有 testing.rs 和 persistence-core 测试执行器无差异 |
| 分支 | `agent/agent/fsy-7-92d3b99e8db3`；保持任务起始提交为祖先 |
| 共享测试夹具提交 | `6e25bc4c532e9bc478e363531c46f1a2df539f69` |
| 财务修复与回归提交 | `d1663465af227946618ac8c1e7626270534d1456` |
| 完整交付提交 | 包含本报告的最终 SHA 记录在 FSY-7 实现交接评论；审查应读取该 SHA |

### 问题处置与入口证据

| 问题 | 实现、入口与测试 | 结果 |
| --- | --- | --- |
| FIN-AUTH-01 · P2 | `src/repository/supplier_deposit_query.rs:53` 的 `account_page` 在调用 `account_rows` 前执行真实 `account_pipeline`，其中调用 `ensure_current`；成功读取后再次调用原凭证的 `ensure_current`。该模块第 423、449 行的内联单测执行生产管道构造，验证跨执行器、下一读取阶段、事务重启、写操作使用的阶段推进、原始会话暴露、原执行器释放、期限冲突。 | 已修复，提交 `d1663465af227946618ac8c1e7626270534d1456`；消费前拒绝有库级行为证据，消费后检查有静态接线证据。 |
| FIN-AUTH-02 · P2 | 预存款 `validate_account_model`（第 98 行）仅接受 `supplier_deposit / supplier_deposit_accounts / id / Object`。付款申请 `validate_request_model`（`src/repository/payable/payment_request.rs:192`）仅接受 `purchase_payment_request / purchase_payment_requests / id / Object`，并核对实际 Repository 的集合。两处均先调用 `QueryModel::validate`，使用现有集合常量。 | 已修复，提交 `d1663465af227946618ac8c1e7626270534d1456`；合法完整对象通过，错误资源、集合、主键和合法 Share/Existence 在生产前置守卫拒绝；非法描述由实际模型校验拒绝。 |

付款申请 `scope_candidates`（第 116 行）经 `authorized_request_stages` 取得原授权前缀，保留成功读取后的原凭证检查。预存款仍采用授权前缀 → 业务交集 → `created_at/id` 稳定倒序 → skip → size+1；分页上限为 100。付款申请原筛选、排序和 10,001 个候选上限不变。成本的合法存在性会员模型保留，相关已有回归实际通过。

两处拒绝均在实际仓储 I/O 之前的生产函数执行；测试没有构造数据库或发出请求。上层展示补齐后的统一 `AuthorizationService::validate` 保持原调用链。预存款读取后的有效期变化、Mongo 聚合执行和事务回滚未动态验证，不能由这些前置函数测试推定通过。

### 共享测试支持与协作边界

父事项已有 FSY-7 的跨任务协作记录，明确提出 finance / fulfillment 的合法凭证测试需求。实现前核对 FSY-8 最新交接，以及 FSY-9、FSY-39 的事项状态与评论：后两者均为 todo、评论为空；Git 历史未见其他已交付的同类凭证夹具。

`erp-identity/src/testing/query.rs:22` 增加 `testing::compiled_access`，只随已有 `test-support` feature 导出。构造权仍在 identity，检查合法模型和设施提供的执行边界，恒签发 `$match: { $expr: false }` 管道。它不会签发生产数据读取权限，不开放任意管道入参，不改 Builder 或生产授权规则；用途仅为真实消费守卫、管道顺序和错误类别回归。有效期可设为固定过去时点，无需 sleep 或依赖计时竞态。

现有 `QualifiedPermissions::test_grants` 的 crate 内部测试构造在同一 feature 下可用，公开可见性未扩大。finance 仅通过 dev-dependencies 启用 identity / persistence-core 的 `test-support`；正常依赖和生产 feature 不变。可供 FSY-9 / FSY-39 复用的单独提交为 `6e25bc4c532e9bc478e363531c46f1a2df539f69`；FSY-8 的 require/功能资格测试仍需其自身适用设施，不能用这个恒拒绝管道冒充真实 query 授权。

FSY-36 的预存款 Facts/Mapping、FSY-38 的资金投影、server 共享装配及 FSY-3 的错误恢复合同均无修改。本轮没有数据库结构、权限编码、DTO 或金额语义变更，无数据迁移。HTTP/worker/Application 工厂切换和 server 全量编译继续属于第六阶段。

### 已执行验证

命令除另注外均在 `backend/` 执行；原始日志随事项最终评论交付。

| 命令 | 实际结果 |
| --- | --- |
| `env -u ERP_TEST_MONGO_URI cargo test -p erp-finance --lib --locked` | 退出码 0；530 passed，0 failed，13 ignored；本轮新增 9 个内联回归。 |
| `env -u ERP_TEST_MONGO_URI cargo test -p erp-identity --lib --features test-support --locked` | 退出码 0；584 passed，0 failed，0 ignored；包含新增夹具的 3 个内联回归和原授权合同测试。 |
| `cargo fmt --all`，`cargo fmt --all -- --check` | 均退出码 0。 |
| `./scripts/check-domain-boundaries.sh --cutover` | 退出码 0；依赖/源码边界和审计边界检查均通过。 |
| `./scripts/check-rust-size.sh` | 退出码 1；全仓 3,905 个活动源码文件中，59 个已有方法超限，超限文件及解析失败均为 0。59 项所在文件与任务起始提交无差异。 |
| 复用 `rust_size_gate.scan_workspace` 扫描本轮 5 个 Rust 文件；逐个核对全仓超限文件相对任务起始提交的差异 | 退出码 0；5 个文件均通过原 1,200 行文件 / 50 有效行方法限制；全仓 59 项所在文件均未修改。未放宽或更新门禁基线。 |
| `git diff --check`，提交前 `git diff --cached --check` | 退出码 0。 |

全仓体积门禁的已有失败不属于本轮两个授权缺陷；没有将该门禁描述为通过，也没有为过门禁修改同族任务的业务代码。门禁产生的 Python 字节码及原始日志不纳入业务提交。

### 验收矩阵与验证限度

| 编号 | 本轮结果与证据限度 |
| --- | --- |
| AC-01 | 生产 `account_pipeline` 同阶段接受，失效边界和到期凭证拒绝；到期保留原 `ConflictError(DATA_SCOPE_CHANGED)`。测试不进入 I/O。 |
| AC-02 | `account_page` 成功聚合后复验原凭证；调用方最终 validate 未改。此项接线静态核对，未模拟数据库读取过程中的时钟变化。 |
| AC-03 | 预存款资源、集合、主键、Object 模型及描述合法性齐全；模型拒绝矩阵与生产前置函数测试通过。 |
| AC-04 | 付款申请资源、集合、主键、Object 模型齐全；保留前后边界检查，生产前置函数的当前 / 失效 / 到期回归通过。 |
| AC-05 | 单测执行真实生产守卫和管道函数；夹具只在 test-support 内构造恒拒绝凭证。未使用源码字符串断言，未新增通用仓储接口。 |
| AC-06 | 账户四种 supplier/payer 组合、授权前缀和排序分页顺序、0/101 大小、skip 溢出、最大页 size+1 均有行为断言。付款申请原筛选组合及成本会员测试通过；真实数据库结果集不在本轮验证范围。 |
| AC-07 | `unwired_organization_reads_fail_closed` 通过；组织事实、显式空集合、上层筛选接线保留，后者沿用分析报告的静态证据。 |
| AC-08 | finance 指定 lib 命令通过，13 个历史 Mongo 测试保持 ignored。未新增/修改/执行集成测试、examples、真实 MongoDB/S3/外部服务测试；未跑 cargo check、all-targets 或 server/workspace 编译。 |
| AC-09 | 实现与测试形成独立提交，最终评论给出交付 SHA / 分支并交 in_review。独立 Review 尚未执行，因此不置 done。 |

identity 的 584 个通过是共享库实际执行结果，未计作 finance 的服务或 Mongo 行为覆盖。生产完整装配、完整公开财务调用链的允许/拒绝、外部依赖变化、并发、真实事务隔离和回滚仍未动态验证；本轮库测试只对上述明确行为提供证据。
