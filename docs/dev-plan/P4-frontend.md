# P4 前端集成（erp-client）

| 项 | 值 |
| --- | --- |
| 分支 | `feat/erp-f<n>-<批次简称>` |
| 并行度 | 10（F1–F10；F0 串行前置，已在 P0 完成） |
| 依赖 | 对应后端域的 P3 已合并 |
| owns | `erp-client/features/<feature>/**` + 该批次页面路由目录 |

`erp-client` 已有 28 个 feature、约 30 个工作台页面在 mock 上完整跑通。
本阶段**不重做 UI**，只把 `features/<feature>/api.ts` 从 mock 实现换成真实 HTTP 实现。

---

## 1. 集成动作（每个 feature）

现状：`api.ts` 从 `@/mock/*` 取数并在内存里过滤、分页、计算，导出的函数签名与
返回类型已经是既成契约；`queries.ts` 用这些函数作 `queryFn`/`mutationFn`。

集成步骤：

1. 阅读该 feature 的 `types.ts` 与 `api.ts`，抽出**对外契约**（函数签名 + 返回类型）。
2. 与后端 P3 交付的接口清单逐字段比对，产出差异表。
   - 字段名不一致：以 `docs/erp-data-model.md` 与 `ui-workspaces/wNN.md` 为准判定谁改。
   - 后端缺字段：登记为后端缺口，不在前端计算补偿。
3. 用 `lib/api/client.ts` 重写 `api.ts` 的实现体，**保持导出签名不变**。
   - 服务端已完成的过滤、排序、分页、汇总，前端删除对应本地实现。
   - `filter-*.ts`、`build-*.ts` 中属于纯展示派生的保留；属于业务计算的删除（应由后端返回）。
4. 通过 `lib/api/feature-source.ts` 切到真实实现，联调。
5. 联调通过后**删除该 feature 的 mock 文件与开关分支**。
6. `queries.ts` 与页面组件原则上不改；确需改动在 PR 中单列。

---

## 2. 硬性要求

1. 所有请求经 TanStack Query，禁止组件内裸 `fetch`、禁止 `useEffect` + 手动请求
   （`erp-client/AGENTS.md` §2）。
2. 纯 SPA，禁止在 Server Component / layout 服务端逻辑中发业务请求（§1）。
3. 表单一律 TanStack Form + zod（§3）。
4. 错误统一 `ApiError`，提示集中在 `useErrorHandler`；API 层不 `throw new Error("string")`（§10）。
5. `queryKey` 分层稳定（`all` / `list` / `detail`）；写操作成功后 `invalidateQueries`。
6. 用户可见文案必须过 `docs/ui-glossary.md`。
7. 金额与数量：后端返回已舍入的字符串，前端**只格式化不运算**。
   现有 `lib/fixed-decimal.ts` 的运算函数在集成后只应剩展示用途；仍被业务计算调用的地方
   要么由后端接管，要么在 PR 中说明为什么必须留在前端。
8. 权限：用 P0 落地的 `lib/permissions.generated.ts` 控制按钮与菜单可见性，
   不再用 `lib/demo-roles.ts` 的演示角色。
9. 数据新鲜度：客户经营质量、盈亏、卡券指标页面必须展示数据更新时间
   （数据模型 §12），从接口返回的 `as_of` 字段读取，不用客户端时间。

---

## 3. 子阶段清单

| 阶段 ID | 页面 | feature | 依赖（P3 单元） | 分支 |
| --- | --- | --- | --- | --- |
| F1 | W14 | `master-data` | C-G2、C-G3 | `feat/erp-f1-master-data` |
| F2 | W03、W04 | `customers`、`contracts` | C-G2、C-G4 | `feat/erp-f2-customer-contract` |
| F3 | W05、W06 | `sales-orders` | C-G4、C-G6 | `feat/erp-f3-sales-orders` |
| F4 | W07、W08 | `procurement-confirmation`、`purchase-orders` | C-G4、C-G5 | `feat/erp-f4-procurement` |
| F5 | W09、W10 | `fulfillment-operations`、`inventory` | C-G6 | `feat/erp-f5-fulfillment` |
| F6 | W11、W12、W13 | `customer-receivables`、`supplier-payables`、`card-funds-review` | C-G7 | `feat/erp-f6-finance` |
| F7 | W01、W02、W15、W16、W28 | `workspace`、`unified-task-queue`、`customer-quality`、`actual-profit-loss`、`card-business-analytics` | C-G1、C-G7、C-G10、P5 投影 | `feat/erp-f7-workbench-analytics` |
| F8 | W17、W18、W19 | `mall-sync`、`import-opening`、`access-audit` | C-G1、C-G8 | `feat/erp-f8-sync-import-audit` |
| F9 | W20、W21、W22、W23 | `supplier-api-connections`、`supplier-catalog`、`product-publications`、`execution-projections` | C-G5、C-G9 | `feat/erp-f9-supply-publication` |
| F10 | W25、W26、W27、W29、W30 | `mall-consumption-orders`、`supplier-orders`、`supplier-settlements`、`integration-errors`、`history-backfill` | C-G10、C-G11、C-G12 | `feat/erp-f10-mall-supplier-ops` |

工作量最大的是 F3（`sales-orders` 18 个文件，含验收工作台、变更单、卡券审批、
采购拒绝处理）与 F5（`fulfillment-operations` 17 个文件）。这两个批次建议单人独占一个 worktree。

F7 依赖 P5 的查询投影（工作台汇总、客户经营质量、实际盈亏、卡券指标允许一分钟内
异步刷新，数据模型 §12），因此排在 P5 投影任务之后。

---

## 4. 验收标准

### 4.1 命令

```bash
cd erp-client
npm run lint
npx tsc --noEmit
grep -r "@/mock" features/<feature> app/<该批次路由>   # 期望：无输出
```

### 4.2 联调检查表（每个页面）

| 项 | 要求 |
| --- | --- |
| 列表 | 分页、排序、全部筛选项对真后端生效；空态与错误态正确 |
| 详情 | 字段齐全，与 `ui-workspaces/wNN.md` 的字段清单逐条对齐 |
| 写操作 | 提交成功后列表自动刷新；按钮在 pending 期间禁用 |
| 权限 | 无权限时按钮/菜单隐藏；后端 403 有明确提示 |
| 错误 | 400/403/409/500 各有可读提示；409 提示为"数据已变更，请刷新后重试" |
| 术语 | 用户可见文案与 `ui-glossary.md` 一致 |
| 新鲜度 | 分析类页面展示数据更新时间 |

### 4.3 PR 证据

conventions §7.3 模板 + 以下三项：

- 契约差异表：前端原 mock 形态 → 后端实际形态 → 处理方式（前端改 / 后端改 / 已登记缺口）
- 已删除的 mock 文件清单
- 每个页面的联调截图或操作记录

---

## 5. 常见偏差

| 偏差 | 处理 |
| --- | --- |
| 保留 mock 文件"以防万一" | 打回；集成完成即删除，历史在 git 里 |
| 在前端重算金额、汇总、状态 | 打回；后端返回已计算值 |
| 改 `queries.ts` 与页面组件以适配后端返回形状 | 打回；应在 `api.ts` 内做适配，或修正后端契约 |
| 组件内裸 `fetch` | 打回；`AGENTS.md` §2 |
| 后端字段缺失时前端造数据 | 打回；登记为后端缺口 |
