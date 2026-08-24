# 自动化矩阵

> 初始状态为 `planned`。实现和执行后必须按真实资产与断言更新为 `covered`、`partial`、`blocked` 或 `manual_only`，不得仅因测试文件存在而标记覆盖。

| 用例编号 | 所属模块 | 优先级 | 测试类型 | 集成测试层级 | 集成测试状态 | 集成测试资产 | 集成测试函数/断言摘要 | E2E 可达性 | E2E 状态 | E2E 资产 | E2E 测试名/断言摘要 | 执行命令 | 环境依赖 | 豁免/剩余人工验证 | 备注 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| TC-RESP-001 | 责任规则 | P0 | 功能 | unit/service_it | planned | `backend/services/src/procurement_responsibility/*` | SKU 优先并返回唯一 A | reachable | planned | `e2e/tests/flow-procurement-responsibility-multi-po.spec.ts` | 销售行只读显示采购 A | Rust + Playwright | 无 | 双轨 |
| TC-RESP-002 | 责任规则 | P1 | 边界 | unit | planned | 同上 | 分类区域和父分类回退 | not_worth_it | planned | 可并入主 E2E fixture | 可选断言负责人 | Rust | 分类树 fixture | 浏览器仅抽样 |  |
| TC-RESP-003 | 责任规则 | P0 | 异常 | service_it | planned | 后端集成测试 | 同层冲突阻止提交且无副作用 | partial | planned | 主 E2E 或 API 数据准备 | 页面显示逐行错误 | Rust + Playwright | 冲突数据通常需集成层注入 | 双轨部分 |
| TC-RESP-004 | 销售预览 | P0 | 异常 | service_it/frontend_unit | planned | 销售提交测试、前端表单测试 | 草稿保存、提交被阻止 | reachable | planned | 主 E2E | 无规则错误与状态不变 | Rust + frontend + Playwright | 无 | 双轨 |
| TC-RESP-005 | 责任规则 | P0 | 权限 | service_it/api_it | planned | 责任规则资格测试 | 停用/无权限账号均拒绝 | partial | planned | 规则管理 E2E | 管理员保存失败提示 | Rust + Playwright | 权限撤销分支由集成层覆盖 | 双轨部分 |
| TC-TASK-001 | 采购任务 | P0 | 状态 | service_it | planned | 销售生效任务集成测试 | 非末级通过无任务、最终生效有任务 | reachable | planned | 主 E2E | 完整审批时序与任务出现 | Rust + Playwright | 无 | 双轨 |
| TC-TASK-002 | 采购任务 | P0 | 权限 | service_it | planned | 任务分组测试 | A/B 任务和行范围不重叠 | reachable | planned | 主 E2E 扩展 | A/B 各自可见 | Rust + Playwright | 可用两个浏览器上下文 | 双轨 |
| TC-TASK-003 | 采购任务 | P0 | 幂等 | service_it | planned | 任务派发并发测试 | 重试仅一条有效任务 | not_reachable | planned | 无 | 无稳定浏览器入口触发生效重放 | Rust | 数据库事务/唯一索引 | 浏览器豁免，集成层必须覆盖 |  |
| TC-TASK-004 | 采购任务 | P0 | 回滚 | service_it | planned | 销售生效失败测试 | 解析失败无部分任务 | not_reachable | planned | 无 | 浏览器难稳定在审批间撤权并注入失败 | Rust | 隔离数据库 | 浏览器豁免，集成层必须覆盖 |  |
| TC-PO-001 | 采购创建 | P0 | 功能 | service_it/frontend_unit | planned | 剩余数量服务测试、数量表单测试 | 4+6 生成两张单且剩余 0 | reachable | planned | 主 E2E | 分两次真实建单并完成任务 | Rust + frontend + Playwright | 合格供给 fixture | 双轨 |
| TC-PO-002 | 采购拆单 | P1 | 功能 | service_it | planned | 创建依据拆单测试 | 供应商/责任独立采购单 | partial | planned | 可选 E2E | 抽样两个供应商 | Rust | 多供给 fixture | 浏览器抽样 |  |
| TC-PO-003 | 采购数量 | P0 | 边界 | unit/service_it/frontend_unit | planned | 数量校验测试 | 0/超量拒绝，等于剩余成功 | reachable | planned | 主 E2E | 输入超量错误、6 成功 | Rust + frontend + Playwright | 无 | 双轨 |
| TC-PO-004 | 采购创建 | P0 | 并发 | service_it | planned | 并发创建集成测试 | 两个 6 最多一个成功 | not_reachable | planned | 无 | 浏览器时序不稳定 | Rust | 并发屏障、MongoDB 事务 | 浏览器豁免，集成层必须覆盖 |  |
| TC-PO-005 | 剩余数量 | P0 | 数据一致性 | unit/service_it | planned | 覆盖状态矩阵测试 | 五种状态占用、作废释放 | not_reachable | planned | 无 | 浏览器逐状态成本过高且不稳定 | Rust | 全状态 fixture | 浏览器豁免，主 E2E 覆盖草稿 |  |
| TC-PO-006 | 任务恢复 | P0 | 状态 | service_it | planned | 作废/减量重开测试 | 剩余 4 且任务恢复 | partial | planned | E2E 可覆盖作废分支 | 作废后“继续建单”重现 | Rust + Playwright | 采购作废入口 | 若当前 UI 无作废入口则集成层覆盖 |  |
| TC-PO-007 | 采购选源 | P0 | 异常 | service_it/frontend_unit | planned | 无供给测试、空态测试 | 任务保留且提示维护供给 | reachable | planned | E2E 异常场景 | 打开任务看准确空态 | Rust + frontend + Playwright | 无供给 fixture | 双轨 |
| TC-PROGRESS-001 | 采购进度 | P0 | 数据一致性 | service_it/frontend_unit | planned | 进度 DTO 和视图模型测试 | 三处均剩余 6 | reachable | planned | 主 E2E | 销售详情/任务/依据一致 | Rust + frontend + Playwright | 无 | 双轨 |
| TC-TASK-005 | 任务转交 | P1 | 权限 | service_it/api_it | planned | 工作任务转交测试 | B 接手，旧采购单不变 | partial | planned | 管理员转交 E2E | A/B 刷新可见性 | Rust + Playwright | 管理员入口 | 可后续独立流覆盖 |  |
| TC-UI-001 | Web 界面 | P0 | 可用性 | frontend_unit | planned | 响应式组件测试 | 关键按钮和信息模型 | reachable | planned | 主 E2E | 桌面完整动作，移动端真实导航/操作 | frontend + Playwright | 两种 viewport | 双轨 |
| TC-SEC-001 | 认证 | P0 | 安全 | api_it | planned | HTTP 合同测试 | 新接口均拒绝未登录 | reachable | planned | 登录跳转 E2E | 未登录访问管理页进入登录 | Rust + Playwright | 真实认证 | 浏览器抽样，API 全量 |  |
| TC-SEC-002 | 规则权限 | P0 | 安全 | api_it | planned | HTTP 权限测试 | 无 manage 写请求拒绝 | reachable | planned | 规则管理 E2E | 无权限无按钮且直达不可用 | Rust + Playwright | 角色权限 fixture | 双轨 |
| TC-SEC-003 | 责任范围 | P0 | 越权 | service_it/api_it | planned | 创建依据越权测试 | A 不能读/写 B 范围 | reachable | planned | 双账号 E2E | A 看不到 B 行 | Rust + Playwright | 双账号 | 直接伪造请求由 API 测试覆盖 | 双轨 |
| TC-SEC-004 | 输入安全 | P1 | 安全 | api_it | planned | DTO 校验测试 | 操作符和非法枚举拒绝 | not_reachable | planned | 无 | 浏览器表单无法构造对象注入 | Rust | HTTP 测试客户端 | 浏览器豁免 |  |
| TC-SEC-005 | 文案 | P1 | 安全/可用性 | frontend_unit | planned | UI 文案测试/扫描 | 内部禁用词不出现 | reachable | planned | 主 E2E | 错误提示使用业务文案 | frontend + Playwright | 错误 fixture | 代码扫描为辅助，不替代浏览器断言 |  |
| TC-SEC-006 | 审计 | P0 | 审计 | service_it | planned | 审计副作用测试 | 关键动作写正确审计 | partial | planned | 管理员/采购 E2E | 完成动作，审计详情由集成层断言 | Rust + Playwright | 审计仓储 | 浏览器不验证数据库字段 | 双轨部分 |
| TC-PERF-001 | 责任解析 | P1 | 性能 | perf | planned | 后端基准/集成脚本 | 200 行/1,000 规则 P95 与查询数 | not_reachable | planned | 无 | 性能不由浏览器 E2E 承担 | 专项命令 | 大数据 fixture | 独立性能环境 |  |
| TC-PERF-002 | 创建依据 | P1 | 性能 | perf | planned | 数据库性能脚本 | 1,000×20 行分页和索引 | not_reachable | planned | 无 | 性能不由浏览器 E2E 承担 | 专项命令 | 大数据 fixture | 独立性能环境 |  |
| TC-PERF-003 | 并发创建 | P0 | 性能 | service_it/perf | planned | 并发集成测试 | 20 请求总覆盖≤100且无孤立数据 | not_reachable | planned | 无 | 浏览器并发不可控 | Rust/专项命令 | 隔离数据库 | 集成层为完成门禁 |  |
| TC-PERF-004 | 进度读取 | P2 | 稳定性 | service_it/perf | planned | 读取稳定性测试 | 500 次读取无状态变化 | not_worth_it | planned | 无 | 无需浏览器重复 500 次 | 专项命令 | 隔离数据库 | 集成层覆盖 |  |

## P0 完成门禁

- `reachable` 或 `partial` 的 P0 用例必须有真实 Playwright 业务断言；无法完成时必须在执行报告中标记阻塞并说明不可替代原因。
- `not_reachable` 的 P0 并发、回滚和状态矩阵用例必须有真实 MongoDB 服务层/API 集成测试。
- 当前所有条目仍为规划状态，不能视为自动化已覆盖或已通过。
