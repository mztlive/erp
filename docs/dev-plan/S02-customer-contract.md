# S02 客户中心与合同

## 1. 元信息

- 分支：`feat/erp-s02-customer-contract`
- 业务期：`p1`
- 依赖阶段：`S01`
- 本阶段是否要求单独编译：`must_compile=false`
  - 中间阶段仅实现 owns 内实体/仓储/服务/Handler；**禁止**并行改写共享汇合文件。合并进集成分支后由「最终集成汇合」保证 `cargo check`。
- 前端工作面：`W03` 客户中心、`W04` 合同 PDF 档案

## 2. 目标与业务范围

### 2.1 客户目录与客户中心（W03）

- `scope=mine|collaborating|team`；状态/搜索/排序对齐 W03 §6.1
- 对象中心：`customerId`/`partyId`、`lockVersion`、归属、联系人/地址/银行、合同摘要、修订时间线、`allowedActions`/`actionBlockers`
- 新建客户同事务：party + 首版 revision + customer_account + 恰好一个 OWNER
- 修订主体：`expectedLockVersion` + `baseRevisionId`；历史快照不被覆盖
- 联系人/地址/银行按有效期追加；掩码 + 短时 reveal + 审计
- 调整 OWNER/协作销售；停用后禁止新合同与新销售单

### 2.2 合同（W04）

- 唯一入口=上传已签署 PDF；禁止新建空草稿/正文编辑/提交生效
- 原子：`contract` + 不可变 `contract_revision` + PDF `file_asset`/`document_attachment`
- 状态仅 `EFFECTIVE|TERMINATED|EXPIRED`；`payment_term_snapshot`/`invoice_requirement_snapshot`
- `selectable-for-sales-order`；PDF 短时签名下载；导出注册 `background_job`；终止

### 2.3 数据表

`contract`、`contract_revision`、`customer_account`、`customer_assignment`、`party*`、`document_attachment`、`file_asset`、`background_job`。

依据：`erp-phase-1.md` §4.3/§5.1；`w03`/`w04`；`erp-data-model.md` §6.2/§6.4。

## 3. 明确不在范围

- CRM 商机/投标/拜访/续签；企业微信审批；可配置审批流
- 销售单 CRUD（W05）；应收正式写（W11）；经营质量写（W15）
- 合同正文起草/在线编辑/用印；从 PDF 反推金额
- 客户自动合并；卡号卡密；改写共享汇合文件
- `relatedSalesOrders` 允许空；禁止本阶段创建 `sales_order` 写路径

## 4. 代码落点与目录布局

### 4.1 owns_modules

- `backend/entities/src/contract`
- `backend/database/src/repository/contract`
- `backend/services/src/customer`、`contract`
- `backend/apps/web-api/src/core/handler/admin/customer`、`contract`

owned_paths 额外允许 party/customer/files/jobs 实体与仓储（见阶段 JSON）；汇合文件只写 PATCHNOTES。

### 4.2 建议树

```text
services/customer/{mod,dto,create,query,revision,assignment,status,reveal,idempotency}
services/contract/{mod,dto,upload,query,terminate,download,export}
handler/admin/customer/*  handler/admin/contract/*
```

开发顺序：实体 → repository（不改 DatabaseExt）→ services → handler → S02-PATCHNOTES。

## 5. 数据模型与索引

| 集合 | 关键规则 |
| --- | --- |
| `party` / `party_revision` | party_no 唯一；信用代码规范化唯一；修订区间不重叠 |
| `customer_account` / `customer_assignment` | customer_no 唯一；同时点恰好一 OWNER |
| `contract` / `contract_revision` | contract_no 唯一；无 DRAFT；恰好一份 PDF；快照字段 |
| `file_asset` / `document_attachment` | PDF ≤20MB；扫描通过才可正式 |
| `background_job` | 导出 job_type；request_id 幂等 |

事务：新建客户四件套；上传合同原子；归属变更结束旧 OWNER+新建同事务。

## 6. API 与权限草图

| 路径 | 权限建议 |
| --- | --- |
| `GET/POST /admin/customers`、revision/details/assignments/disable/enable/sensitive-reveals | `customer:*` |
| `GET /admin/contracts`、upload-pdf、revisions/upload-pdf、terminate、pdf-download-url、exports、selectable-for-sales-order | `contract:*` |

动作契约服务端计算；`allowedActions` 覆盖 W03/W04 mock 使用的动作码。本阶段不改 routes。

## 7. 前端集成点

- `erp-client/features/customers/*`、`contracts/*`
- types 以现有 TS 为基准；query keys：`customerKeys`、`contractKeys`
- 新增 api.ts；env 开关 mock；PDF 客户端+服务端复验

## 8. 实现任务清单

1. 建模 party/customer/contract/files/jobs + 单测
2. 仓储 command/query/_with_session
3. services customer + contract（数据范围 mine/collaborating/team）
4. handler permission 宏；PATCHNOTES
5. 测试：新建/修订冲突/上传 v1/v2/终止/幂等

## 9. Worktree / 并行约定

```bash
git worktree add ../erp-s02 -b feat/erp-s02-customer-contract <after-S01-base>
```

禁止触碰全部汇合文件与其它域 owns。→W05 只读暴露 contractId+revisionId；←S01 复用 JWT/RBAC/上传。

## 10. 验收标准

- [ ] 客户三 scope 与新建四件套；修订乐观锁；reveal 短时可审计
- [ ] 合同无「新建合同」；上传 v1/v2 不改历史；selectable 仅 EFFECTIVE+客户启用
- [ ] 终止后不可选；导出注册 background_job；非 PDF/>20MB 拒绝
- [ ] 风格/文档范围/PATCHNOTES；`must_compile=false`；集成后 cargo 门禁

---

*阶段 ID：S02 · 分支：feat/erp-s02-customer-contract · phase_tag：p1 · must_compile：false*
