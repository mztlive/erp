# dev-plan 通用约定

> 状态：生效
>
> 本文件登记跨专项的文件所有权、冻结清单、分支命名、amendment 流程、测试独占目录和质量门禁。专项文件只能补充专属内容，不得重新定义本文件规则。

## 1. owns 前缀规则

1. 一个子阶段一个分支、一个 PR，只能修改所属 `owns` 前缀内的文件；
2. `owns` 前缀由专项阶段文档逐阶段登记，冲突时以 `_meta.json` 为准；
3. 两个**并行**阶段不得 own 同一文件。存在依赖顺序（后者 `dependsOn` 前者）的阶段之间，允许用 `_meta.json` 的 `ownsWithin` 把同一文件按符号范围分段登记，每段注明负责的枚举、字段、值对象或函数，并禁止越界修改。既不在 `owns` 也不在 `ownsWithin` 的文件视为无主，必须先修订本目录再实施；
4. 旧文件的**删除**也是所有权：必须由 `_meta.json` 的 `deletes` 明确登记责任阶段，未登记的旧文件不得由任意阶段顺手删除；
5. 阶段实施时必须使用全仓符号搜索识别调用方，不得只改 `owns` 表中的单个入口。
6. `_meta.json` 的 `perDocumentTypeStages[].p3` 与 `.p4` 是正式阶段对象，和 `stages[]` 采用同一所有权、依赖、分支和门禁规则。`dependencyGroups` 只允许表达「全部成员完成」门禁，不得代替具体阶段对象。

## 2. 冻结文件

下列文件是跨层共享入口，属于 P0 独占。P1—P6 不得修改。P0-A 必须在冻结这些文件的同时，为后续阶段将要填充的每个目标模块创建**失败关闭的最小占位文件**并完成模块声明（`_meta.json` 的 `creates`），使 P1—P5 不需要为了注册模块而回头修改冻结文件：

```text
backend/Cargo.toml
backend/AGENTS.md
backend/apps/web-api/src/main.rs
backend/apps/web-api/src/app_state.rs
backend/apps/web-api/src/core/handler/mod.rs
backend/apps/web-api/src/core/routes/mod.rs
backend/apps/web-api/src/core/routes/admin.rs
backend/apps/web-api/build.rs
backend/{entities,database,services}/src/lib.rs
backend/entities/src/ids.rs
backend/database/src/repository/mod.rs
backend/database/src/repository/extensions/mod.rs
backend/database/src/indexes/mod.rs
backend/services/src/{lib,errors}.rs
backend/services/src/approval/mod.rs
erp-client/lib/workspace-registry.ts
erp-client/lib/permissions.generated.ts
```

生成文件必须由登记的生成命令产生，不得手工编辑。

冻结不等于永不修改：`_meta.json` 已把每个冻结文件精确分配给 P0-A、P0-B 或 P0-C。冻结文件出现在任何非 P0 阶段的 diff 中即为阻断。

## 3. P0 amendment 流程

阶段实施中发现需要修改冻结文件时：

1. 立即停止当前 PR，不得夹带修改；
2. 新建单主题分支 `chore/erp-p0-amend-<主题>`，只做该主题的地基修改；
3. amendment 必须可独立合并并通过第 6 节全部门禁；
4. 合并后全部在途分支 rebase；
5. 一个 PR 无法以单一地基主题描述时必须拆成多个 amendment 逐个合并。

禁止以「交给最终阶段接线」为理由把共享修改推迟到 P6。

`P0-D` 是已登记的硬切换清理 amendment，只允许在 `ALL_DOCUMENT_TYPE_ROLLOUTS` 全部完成后执行。它负责删除为准备阶段持续编译而保留、但已无生产调用方的旧符号和旧文件；不得新增兼容读取、旧运行时入口或 fallback。

## 4. 编译与失败容忍

1. 每个准备合并的 DOC、P0—P6 PR 都必须在最新目标分支上独立通过第 6 节适用的全量门禁；编译失败、测试失败或生成物漂移的阶段分支不得合并；
2. P1—P5 允许因其他层尚未完成而暂时无法形成业务闭环，但不允许因此破坏现有 workspace。目标代码必须采用可编译的新增模块、未接线端口或失败关闭入口；激活新路径与删除旧路径必须放入已登记的切换阶段；
3. 准备阶段保留尚未接线的旧代码不构成兼容承诺，但新旧路径不得同时处理同一 `DocumentType`。切换后未接入类型必须返回稳定的失败关闭错误，不得回退旧运行时；
4. 不得以兼容枚举、旧字段 fallback、双写、前端推断或 `Noop` 领域动作换取编译通过；
5. 占位实现必须失败关闭，不得返回伪造成功或默认责任人。

## 5. 集成测试独占

1. `backend/database/tests/**` 与 `backend/apps/web-api/tests/**` 由 `P6-PILOT` 和 `P6-FINAL` 共同独占；
2. P2、P3 不得在上述目录新增或修改文件，只允许在模块内编写不依赖外部服务的单元测试；
3. Web API 集成测试必须是 Cargo 自动发现的顶层 target，不得放入未注册子目录或依赖共享 `mod.rs`；
4. 真实 MongoDB 测试必须使用 `#[ignore]` 与 `require_mongo!()`，只从 `ERP_TEST_MONGO_URI` 读取连接，使用独立随机库名并在结束后精确 drop，不依赖开发者现有数据，不打印完整 URI 或凭证；
5. 命名固定为 `<专项>_<关注点>.rs`，例如 `approval_workflow_repository.rs`、`approval_workflow_api.rs`。同一专项的新旧测试入口不得并存。

## 6. 质量门禁

任何修改后端文件的 PR 都必须在 `backend` 执行：

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

`P6-FINAL` 额外执行，且必须在 MongoDB replica set 环境提供 `ERP_TEST_MONGO_URI`：

```bash
cargo test --workspace -- --include-ignored
```

任何修改前端文件的 PR 都必须在 `erp-client` 执行：

```bash
npm run format:check
npm run lint
npm run test
npm run test:node
npm exec -- tsc --noEmit
npm run build
```

未提供数据库导致的 `ignored` 状态不得被表述为集成测试通过。

只修改 Markdown/JSON/YAML 文档的 DOC PR 至少必须执行 `git diff --check`、JSON 解析、相对链接检查和本专项第 10 阶段的文档语义扫描。DOC-D 还必须执行 `npm run openapi:lint`。

## 7. 代码约定

1. 所有新增或修改的非生成 Rust 方法与函数必须具备多行 `///`；
2. 生产代码不得使用未经允许的 `unwrap`、`expect` 或 `panic!`；
3. 前端新增函数、hook 和组件必须有 JSDoc；业务页面使用 Client Component，请求通过 TanStack Query，表单使用 `useAppForm + Zod`；
4. 用户可见文案必须通过 `docs/ui-glossary.md`，不得渲染内部枚举或 ID。

## 8. 分支命名

```text
chore/erp-p0-amend-<主题>        P0 地基与 amendment
feat/erp-<专项>-p<N>-<子阶段>    P1—P5 实施
test/erp-<专项>-p6-pilot         P6-PILOT
test/erp-<专项>-p6-final         P6-FINAL
docs/erp-<专项>-contract         DOC-A 合同与本目录
docs/erp-<专项>-data-model       DOC-B 数据模型
docs/erp-<专项>-workspaces       DOC-C 页面与术语
docs/erp-<专项>-protocol         DOC-D 线协议与运维
```

本专项 `P0-D` 的固定分支为 `chore/erp-p0-amend-approval-workflow-hard-cutover-cleanup`。
