# 显式权限目录验收记录

## 交付基准

- 迁移前基线提交：`d551faa0d7d1d55e2871fb00d9b776931fe51aed`。
- 合并基准：`59ea90c2cde152fbba982feb415fbf141d744093`；保留该版本的供应商售后实现及退款结果接口新路径。
- 工作分支：`refactor/explicit-permission-catalog`。
- 独立 worktree：`/Users/huangjiajiang/Development/erp-permission-catalog`。
- 验证工具链：`rustc 1.101.0-nightly (41089d07a 2026-10-27)`。
- 实施合同：[权限目录迁移合同](permission-catalog-migration-contract.md)。
- 维护合同：[权限目录维护合同](../backend/docs/permissions-gen.md)。

## 行为不变验收

| 验收项 | 结果 | 证据 |
| --- | --- | --- |
| 657 处既有声明的权限身份 | 一致；410 个不同 HTTP 权限代码各定义一次 | `rbac_contract_cases.rs` 执行真实权限转换；静态目录检查 |
| 678 条 HTTP 路由的路径、方法、处理函数、目录信息及权限层顺序 | 与合并基准逐项一致 | `testdata/permission-routes.json` 与真实构建生成器输出对比 |
| 允许、拒绝、未登录、授权服务故障 | 2,628 个请求场景通过；状态、完整响应、调用主体、权限及下游执行次数一致 | `rbac_contract.rs` |
| 合同共享销售的两层入口权限 | 外层先检查；拒绝和异常停止下游；6 个场景通过 | `layered_permissions_keep_outer_first_denial_and_stop_execution` |
| 供应商连接命令的入口与目录权限 | 保留入口 `detail`、目录 `command` | `PermissionBinding`、路由基线及绑定测试 |
| 前端权限目录 | 与合并基准逐字节一致 | SHA-256：`d038dcc49ece38c81faf0b40f7ced8eda4de83be592c22929f29ee3cc5b8e010` |
| 后端授权文件白名单 | 411 个代码及顺序一致 | `testdata/policy-permissions.json` 对比编译进应用的实际白名单 |
| 生产处理器与路由函数 | 与合并基准的 192 个文件、1,025 个生产函数一致 | 统一权限参数表达、排除属性与格式差异后的语法树对比 |
| 角色授权与业务编排 | 权限代码未改变，业务函数实现未改变 | 权限合同及语法树对比；`erp-processes` 无文件改动 |

源码对比属于静态补充证据；请求行为由真实权限层单元测试验证。

## 架构验收

1. 运行时和构建生成器共同编译 `apps/web-api/permission_catalog/` 中的普通 Rust 常量。
2. 管理端路由显式引用目录常量；处理函数不再声明权限属性。
3. 旧权限属性宏 crate、workspace 成员、直接依赖及生成函数调用已移除。
4. 未知引用、缺少入口绑定、重复模块或符号、非法权限代码、分组说明冲突必须阻止生成。
5. 编译常量引用测试核对索引符号与实际常量的全部字段，防止目录索引与运行时常量配错。
6. 接口复用权限时保留各自分组和说明；独立领域政策权限仍引用拥有领域的定义。
7. 领域边界检查的最终模式继续执行全量依赖、源码与旧目录清零检查，不再读取已删除的阶段计划。缺少旧计划、旧依赖回流与旧目录回流均有纯回归测试。

## 质量门禁结果

| 检查 | 结果 |
| --- | --- |
| 迁移前工作区库测试 | 7,123 通过，0 失败，63 忽略；包含迁移前新增的鉴权基线测试 |
| 最终工作区库测试 | 7,160 通过，0 失败，63 忽略 |
| 最终 `web-api` 库测试 | 310 通过，0 失败 |
| `cargo fmt --all -- --check` | 通过 |
| `cargo check --workspace --locked` | 通过 |
| `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | 通过；未豁免 lint |
| BPM、领域边界及审计边界检查 | 通过 |
| 权限生成物漂移检查 | 通过；前端权限及审计动作目录均无漂移 |
| 迁移阶段 Rust 体积对比 | 原迁移基线 100 项、迁移结果 99 项；按文件和方法核对，无新增违规 |
| 新增 Rust 文件体积检查 | 47 个文件，0 项违规；包含普通活动源码枚举外的构建目录文件 |
| `git diff --check` | 通过 |

以下四个文件仅将测试模块移至文件末尾，修复 `items_after_test_module`。测试模块原文和生产代码内容保持不变，全仓严格 Clippy 必须通过：

- `crates/erp-sales/src/dto/sales_selection.rs`
- `crates/erp-sales/src/repository/sales_order/scope.rs`
- `crates/erp-read-models/src/customer_quality/dto.rs`
- `crates/erp-read-models/src/workbench/search/filters.rs`

合并门禁要求 `scripts/audit-entrypoints.json` 精确登记合并基准新增的四个供应商售后审计入口：退款意图保存、退款事实完成、退款观察结果保存和结算分页保存。每项均核对实际审计调用及事务边界，不增加豁免，不修改 `erp-processes` 业务代码。

## 交付与适用范围

- 验证范围为静态合同、构建产物及库单元测试；不声明真实数据库、浏览器或部署验收。
- 相对合并基准未修改 `backend/crates/erp-processes/`；未重启或部署应用。
- 交付要求：全部提交门禁通过后提交到 `refactor/explicit-permission-catalog`，以快进方式合入本地 `main`；不包含远端推送。
- 原始日志、基线快照、源码对比工具和完整补丁保存在 `/Users/huangjiajiang/Development/erp-permission-evidence/`；合并基准的复验日志及交付补丁位于其 `merge/` 子目录。
