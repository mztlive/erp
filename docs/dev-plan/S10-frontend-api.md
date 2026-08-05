# S10 前端 API 集成

## 1. 元信息

- 分支：`feat/erp-s10-frontend-api`
- 业务期：`p1p2`
- 依赖阶段：`S05`、`S06`、`S07`、`S08`、`S09`
- `must_compile=false`（前端独占；不改 backend 汇合文件）
- 前端本分支须能独立 `npm run lint` 与既有 `erp-client/scripts/test-*.mjs`

## 2. 目标与业务范围

1. **替换 features 数据层为真实 HTTP**  
   - `features/*/api.ts`（或无 api 时 queries 内 fetch）→ web-api admin  
   - **保留** types 视图契约与 React Query key 形状；页面路由不变

2. **承接服务端语义**  
   - `lockVersion`/`expectedLockVersion`、`idempotencyKey`  
   - `FormalActionResponse`：`succeeded|failed|unknown`；unknown 禁止本地伪造成功  
   - `allowedActions`/`actionBlockers` 一律服务端，禁止按角色名推断

3. **敏感字段 mask + 短时 reveal**；`permissionVersion` 变化 invalidate + 清明文

4. **退役**正式路径对 `@/mock/*`、`session-state` 写依赖；默认走真实 API

5. **对接** permission 宏生成 catalog 与 W19 访问审计

### 覆盖工作面

W01–W30（缺 W24）全部 `frontend_surfaces` 见阶段 JSON；与 `ui-workspaces` 索引一致。

## 3. 明确不在范围

- 发明未写单据/状态；可配置动态审批流 UI；三期总账/银行流水
- 改造商城侧；改 App Router 页面路径；并行改 backend 汇合/域实现
- 页面裸 fetch；前端重算正式余额/毛利；W19 展示敏感旧新值正文

## 4. 代码落点

### owns_modules

```text
erp-client/lib/api/   # client, errors, idempotency, formal-action, sensitive-reveal, permissions
erp-client/features/<全部 W 对应域>/
erp-client/PATCHNOTES.md
```

无 api.ts 的域：从 queries 抽 api.ts。backend 目录本阶段不拥有，仅联调对照。

### HTTP 客户端契约

| 项 | 要求 |
| --- | --- |
| Base URL | `NEXT_PUBLIC_ERP_API_BASE` |
| 鉴权 | Bearer JWT；401 清会话 |
| 响应 | 解包 `ApiResponse` |
| 冲突 | `VERSION_CONFLICT` → 冲突对话框 |

## 5. 数据模型与索引

`data_model_tables: []`。不新建集合；消费 S05–S09 既有表与 DTO。

## 6. API 与权限草图

路径前缀 `/admin/...`；以 S05–S09 最终路由为准。代表面：workspace、work-items、customers、contracts、sales-orders、procurement、purchase-orders、fulfillment、inventory、finance、master-data、access-audit、supplier-api、commerce、permissions/catalog。  
`PATCHNOTES.md` 逐条列出最终路径、permission key、幂等查询路径。

## 7. 前端集成点

### 替换顺序

1. `lib/api/*` 基础设施  
2. access-audit + 权限 catalog  
3. workspace + unified-task-queue  
4. 主数据/客户 → 交易链 → 二期协同/治理  

### Query Key

保留 `workspaceHomeKeys`、`unifiedQueueKeys`、`customerKeys` 等前缀；必须纳入 userId/role/permissionVersion/dataScopeVersion/筛选。

### FormalAction / 敏感 / 权限

mutation 成功 invalidate list+detail；unknown 同键重查；reveal 集中 lib；导航按 catalog 裁剪。

## 8. 实现任务清单

1. 契约差异表写入 PATCHNOTES  
2. lib 基础设施  
3. 按域替换 api/queries；正式路径 mock 归零  
4. W02 统一动作与 handlerKey  
5. 敏感+W19  
6. 权限生成物接入  
7. 测试与 lint  
8. 后端缺口只记 PATCHNOTES，不改 routes  

## 9. Worktree / 并行约定

- 允许：`erp-client/features/**`、`lib/**`、PATCHNOTES  
- 禁止：`backend/**` 汇合与域实现；改 URL 路由表；扩展 mock 正式写语义  
- 与后端文件零重叠；合并受 depends_on 约束  

## 10. 验收标准

- [ ] W01–W30 主路径真实 API；写带幂等；unknown 不伪造成功  
- [ ] lockVersion 冲突可恢复；allowedActions 一致；reveal 可审计  
- [ ] 菜单依赖服务端权限；features 无 mock 正式写  
- [ ] lint 通过；风格符合 erp-client AGENTS；`must_compile=false`（cargo 由 S11）

---

*阶段 ID：S10 · 分支：feat/erp-s10-frontend-api · phase_tag：p1p2 · must_compile：false*
