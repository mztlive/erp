# 自动化执行报告：采购责任分配与一单多采

- 执行日期：2026-08-24
- 对应 PRD：`docs/prd/changes/2026-08-24-procurement-responsibility-and-multi-purchase-orders.md`
- PRD 状态：`approved`
- 执行范围：Rust 后端、Next.js 前端、真实 Playwright 流程、Chrome 桌面与移动端行为验证、产品基线与自动化矩阵收口
- 总体结论：**核心用户流程已实现，并通过后端门禁、目标前端测试、生产构建、真实 E2E 以及桌面/移动浏览器验证；目标域真实 MongoDB 集成测试、性能专项和若干 PRD 验收项仍未完成，因此不把增量 PRD 标记为 `merged`。**

## 1. 已验证的业务结果

1. 采购确认可处于销售审批的非末级节点；该节点通过后不会提前生成采购建单任务。
2. 最终审批通过并使销售单生效后，系统为具体采购负责人生成 `PROCUREMENT_ORDER_CREATION` 开放任务。
3. 采购责任按规则解析到当前有效、可登录且拥有 `purchase_order:create` 的具体账号；销售页面只读展示负责人。
4. 创建依据绑定销售单、销售当前修订、开放任务、冻结稳定销售行范围、供应商及拆分维度。
5. 同一销售数量 10 可先创建数量 4 的采购草稿，再创建数量 6 的采购草稿；两张采购单共同覆盖销售需求。
6. 覆盖归零后采购任务自动完成；作废数量 4 的草稿后释放剩余量，并创建新的 successor 开放任务，历史终态任务不重开。
7. 非 owner、其他销售单任务和伪造任务 ID 采用隐藏式 404；同一最新依据的并发创建结果为一成功、一 HTTP 409，最终覆盖不超量。
8. 销售详情、采购创建依据和工作台任务在部分覆盖时均显示剩余 6，全部覆盖时显示销售总量 10、已覆盖 10、剩余 0。
9. 桌面和 `390×844` 移动视口均实际完成采购列表、详情、作废、来源销售单、采购进度、返回列表和工作台导航。

## 2. 后端质量门禁

### 2.1 Workspace 门禁

执行：

```bash
cd backend && cargo fmt --all -- --check \
  && cargo check --workspace \
  && cargo clippy --workspace --all-targets --all-features \
  && cargo test --workspace
```

结果：**通过，退出码 0。**

- Rust 格式检查通过。
- Workspace 编译检查通过。
- Clippy 通过；保留 5 个本轮范围外的既有 warning：2 个 `too_many_arguments`、1 个 `useless_conversion`、2 个 `clone_on_copy`。
- Workspace 测试通过。
- MongoDB ignored tests 未执行；本目标域没有落地使用真实 MongoDB `TestDb` 的 service/API integration tests。

### 2.2 后端构建

执行：

```bash
cd backend && cargo build -p web-api
```

结果：**通过。**

## 3. 前端质量门禁

### 3.1 类型检查

执行：

```bash
cd erp-client && npx tsc --noEmit
```

结果：**通过。**

### 3.2 本功能目标测试

执行 20 个直接相关测试文件，覆盖责任规则、销售责任预览和服务区域、销售详情采购进度、采购创建与缓存失效、工作台任务入口以及 SKU 组合框。

```bash
cd erp-client && npx vitest run \
  features/procurement-responsibilities/api/rules.test.ts \
  features/procurement-responsibilities/components/procurement-responsibility-rules-page.test.tsx \
  features/sales-orders/api/procurement-responsibility.test.ts \
  features/sales-orders/api/sales-order-service-region.test.ts \
  features/sales-orders/components/sales-order-create-form.test.tsx \
  features/sales-orders/components/sales-order-detail-related-lanes.test.tsx \
  features/sales-orders/components/sales-order-detail-tabs.test.tsx \
  features/sales-orders/hooks/use-sales-line-procurement-responsibilities.test.tsx \
  features/sales-orders/lib/sales-order-detail-mappers.test.ts \
  features/purchase-orders/api/purchase-order-commands.test.ts \
  features/purchase-orders/api/purchase-order-queries-api.test.ts \
  features/purchase-orders/pages/purchase-orders-list-create-dialog.test.tsx \
  features/purchase-orders/hooks/queries.test.tsx \
  features/purchase-orders/hooks/use-purchase-order-detail-edit-actions.test.ts \
  features/purchase-orders/hooks/use-purchase-order-detail-queries.test.tsx \
  features/purchase-orders/hooks/use-purchase-orders-list-controller.test.tsx \
  features/purchase-orders/hooks/use-purchase-order-detail-permissions.test.ts \
  features/workspace/components/workspace-task-detail.test.tsx \
  features/workspace/lib/document-facts.test.ts \
  tests/components/business/entity-comboboxes.test.tsx
```

结果：

```text
Test Files  20 passed (20)
Tests       128 passed (128)
```

### 3.3 修改文件格式与 lint

执行范围：85 个已修改或新增的 `erp-client` 文件。

```bash
TMP_FILES="$(mktemp)"
{ git diff --name-only -- erp-client; git ls-files --others --exclude-standard erp-client; } \
  | sed 's#^erp-client/##' | sort -u \
  | while IFS= read -r file; do
      [ -f "erp-client/$file" ] && printf '%s\n' "$file"
    done > "$TMP_FILES"
cd erp-client \
  && xargs npx oxfmt --check < "$TMP_FILES" \
  && xargs npx oxlint < "$TMP_FILES"
```

结果：

```text
All matched files use the correct format.
Found 0 warnings and 0 errors.
```

### 3.4 生产构建

执行：

```bash
cd erp-client && npm run build
```

结果：**通过，45 个页面生成成功。**

非阻塞警告：Next.js 检测到多个 lockfile，并把 `/Users/huangjiajiang/package-lock.json` 推断为 workspace root。该警告不影响本次构建结果。

### 3.5 全量 Vitest 尝试

本轮曾使用修改测试文件列表执行 Vitest；由于命令中的 `--` 参数行为，实际扩展为全量测试。结果：

```text
Test Files  404 passed, 2 failed, 2 worker OOM
Tests       3450 passed, 4 failed
```

失败均位于本功能范围外：

| 文件 | 失败数 | 现象 |
| --- | ---: | --- |
| `features/supplier-payables/pages/hooks/use-payment-reversal-flow.test.ts` | 1 | `reversalSubmitOpen` 期望 `true`，实际 `false` |
| `features/master-data/hooks/use-sellable-list-columns.test.tsx` | 3 | 文本精确匹配、`meta.align` 与 SVG 图标断言不一致 |

另有 2 个 worker 因内存不足退出。直接相关的 20 个测试文件随后单独执行并全部通过。本轮没有修改上述无关失败，也不把全量前端测试声明为通过。

## 4. 真实 Playwright E2E

执行：

```bash
cd e2e && E2E_ALLOW_REMOTE_RESET=1 \
  bash scripts/run-flow.sh \
  tests/flow-procurement-responsibility-multi-po.spec.ts
```

环境动作：

- 通过安全门重置远程开发 MongoDB 业务数据。
- 重启真实 Rust 后端。
- 发布 12 个审批定义。
- 使用真实前端、后端、MongoDB、认证账号和供应商供给数据；未 mock 应用接口。

结果：

```text
1 passed (2.4m)
```

核心断言：

- 非末级采购审批后销售单未生效且采购任务数为 0。
- 最终审批后采购任务派发到采购账号。
- 管理员跨 owner 查询创建依据返回空；伪造任务创建返回 HTTP 404。
- 第一次创建数量 4 后依据 ID 更新，三处剩余量均为 6。
- 第二次创建数量 6 后存在两张采购草稿，数量为 `[4, 6]`，任务完成且剩余为 0。
- 作废数量 4 的草稿后剩余恢复为 4，原任务仍为终态，新 successor 任务开放。
- 对 successor 最新依据并发发送两次创建命令，恰好一条成功、一条 HTTP 409；最终覆盖仍为 10。
- 移动视口可实际打开继续建单入口和工作台任务，不存在关键操作不可达。

## 5. Chrome 桌面与移动端验证

### 5.1 桌面

使用管理员、销售和采购真实账号执行以下行为：

- 在采购责任规则页编辑规则、停用后恢复并保存，真实 `PUT` 返回 200。
- 在销售详情查看测试销售单、两张采购单和 `10/10/0` 采购进度。
- 使用采购登录名 `caigou` 查看工作台，确认已覆盖后的“待我处理”为 0。
- 在采购列表查看两张有效草稿和一张已作废采购单。
- 打开有效采购详情，确认来源销售单、数量 4 和金额信息。
- 从采购详情进入来源销售单，再通过“打开采购”返回按销售单筛选的采购列表。

### 5.2 移动端

视口：`390×844`，触摸模式。

实际操作并验证：

- 采购列表与筛选结果。
- 采购详情和作废状态。
- 来源销售单导航。
- 销售采购进度与“采购已覆盖”。
- 返回采购列表。
- 工作台空状态。

最终移动工作台控制台无错误；profile、work-items 和 stats 请求均返回 200。

### 5.3 浏览器环境观察

- 采购账号的实际登录名为 `caigou`；API/E2E 逻辑别名仍为 `procurement`。
- 采购角色读取销售详情时，若请求客户详情、验收、应收、收款或发票等可选子资源，会出现既有 403 网络噪声；主页面按现有逻辑降级并保持可用。本轮未扩大采购角色权限。
- 开发期间旧页面曾持有旧 TanStack Query 内存数据结构，硬刷新后读取新 schema 正常。项目处于开发期且需求明确不保留旧客户端状态兼容，因此未增加兼容分支。

## 6. 产品基线收口

已更新：

- `docs/prd/03-master-data-and-permissions.md`
  - 补充同层异常重复命中时保守失败。
  - 明确任务管理受授予 `work_item:manage` 的角色管理数据范围约束。
  - 明确采购任务候选账号只需当前有效、可登录且具备 `purchase_order:create`，不要求已有销售参与关系。
- `docs/prd/05-sales-orders.md`
  - 保留已实现的责任预览、提交门禁、最终生效派发和采购进度口径。
- `docs/prd/06-workspace.md`
  - 补充管理数据范围和采购任务转交资格。
- `docs/prd/08-purchase-orders.md`
  - 当前产品审批状态只记录 `IN_APPROVAL`，不再把旧 `PENDING_FINANCE_REVIEW` 写入 As-Is 状态表。
  - 创建页空状态按当前实现记录为合并提示，而不是四类独立提示。
- `automation-matrix.md`
  - 所有条目已按真实资产更新为 `covered`、`partial`、`blocked`、`manual_only` 或 `planned`。

## 7. 未完成项与风险

| 项目 | 状态 | 影响 |
| --- | --- | --- |
| 目标域真实 MongoDB service/API integration tests | blocked | 尚未独立证明多集合事务回滚、DataScope 转交、重复生效和数据库级并发副作用 |
| 销售最终生效责任解析失败的管理员业务异常记录 | 未实现 | 生效事务会失败回滚，但没有 PRD 要求的专用管理员可见异常记录 |
| 并发采购创建被拒绝的失败审计 | 未发现持久化证据 | 成功创建审计和幂等收据已实现；HTTP 409 拒绝本身尚未形成可追踪失败审计 |
| 四类独立创建空状态 | 未实现 | 当前用合并文案覆盖“未生效、无供给、已覆盖”等原因，服务端授权与结果仍正确 |
| 双采购负责人分组与管理员转交 E2E | blocked | 单负责人主流程已通过；A/B 独立范围和真实转交仍缺自动化 |
| 未登录/规则写权限 API 合同测试 | blocked | 依赖通用认证授权实现，尚无本目标域逐接口证据 |
| 性能专项 4 条 | planned | 没有 200 行/1,000 规则、1,000×20 依据、20 并发和 500 次读取的性能结论 |
| 泛型 `work_item::close` 授权提交栅栏 | 既有问题 | W29 关闭路径未采用本次关键写操作相同的授权提交栅栏；不属于本功能新增路径 |
| 全量前端 Vitest | 未通过 | 2 个无关测试文件失败并有 2 个 worker OOM；本功能 focused tests 全部通过 |

## 8. 发布与 PRD 状态结论

- **核心功能代码、真实主流程和桌面/移动端行为可用。**
- **产品基线只记录当前代码能够支持的 As-Is 行为。**
- 增量 PRD 的目标仍包含管理员业务异常记录、并发拒绝审计和四类独立空状态；自动化门禁还要求真实 MongoDB 集成和性能专项。因此 PRD 继续保持 `approved`，本报告不声明所有验收标准已完成，也不将其改为 `merged`。
