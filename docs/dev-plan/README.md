# dev-plan：阶段执行唯一来源

> 状态：生效
>
> 本目录是 ERP 后端与前端分阶段实施的唯一来源。任何阶段划分、文件所有权、冻结清单、分支命名和门禁只以本目录为准。

## 1. 目录职责

| 文件 | 职责 |
| --- | --- |
| `README.md` | 本文件。声明目录效力、阶段模型和阅读顺序 |
| `conventions.md` | 登记冻结文件、`owns` 前缀、P0 amendment 流程、集成测试独占目录和质量门禁 |
| `domains.md` | 登记参与改造的 crate 与业务域，及其所有权归属 |
| `_meta.json` | 机器可读的阶段 ID、分支、`owns`、依赖和验收命令 |
| `approval-workflow.md` | 审批流程改造专项：阶段映射、政策矩阵引用、试点、逐类型批次和切换状态 |

本目录不重新定义业务语义。业务语义的权威来源是 `docs/approval-workflow-contract.md`、`docs/erp-phase-1.md`、`docs/erp-phase-2.md` 和 `docs/erp-data-model.md`。

## 2. 阶段模型

| 阶段 | 性质 | 说明 |
| --- | --- | --- |
| DOC-A | 文档合同 | 签署权威业务合同、本实施计划与本目录。**是 P0-A 的唯一前置**，不得与 P0 并行 |
| DOC-B | 文档合同 | 数据模型同步。**是 P2 的前置** |
| DOC-C | 文档合同 | 页面与术语同步（全部受影响 W 文档，含 W01/W02/W05/W19/W24、`ui-glossary.md`）。**是 P4 的前置** |
| DOC-D | 文档合同 | 线协议与运维（OpenAPI、错误目录、runbook、`openapi:lint`）。依赖 P3-HTTP，**是 P6-PILOT 的前置** |
| P0 | 共享地基 | workspace、依赖边界、跨层名称、公共入口、依赖注入和权限生成。分 P0-A / P0-B / P0-C 三波 |
| P1 | 领域模型 | 纯领域 crate 模型与实体集成模型 |
| P2 | 持久化 | Repository、索引、CAS |
| P3 | 应用与协议 | Service 编排、业务适配、HTTP |
| P4 | 前端 | 工作面、组件、工作台 |
| P5 | 环境切换 | 开发业务数据重置脚本与 runbook |
| P0-D | 硬切换清理 | 全部类型接入后删除旧模型、旧运行时、旧责任动作和旧权限；不提供兼容读取 |
| P6 | 验收 | `P6-PILOT` 纵向试点门禁；`P6-FINAL` 全量集成与发布门禁 |

固定顺序约束：

1. **DOC-A 必须先合并**。DOC-A 未生效时不得开始 P0；实施人员不得代替业务合同设置默认值；
2. DOC 被拆成四段，各段只卡它的下游阶段：DOC-B 卡 P2，DOC-C 卡 P4，DOC-D 卡 P6-PILOT。任何一段未完成时，其下游阶段不得开始，但不阻断其他阶段；
3. P0-A 必须在 P1—P5 开始前合并，并按 `conventions.md` 第 2 节创建全部目标模块占位；
4. P1—P5 在 rebase 最新 P0 后可独立实施；
5. P0-B、P0-C 必须在 `P6-PILOT` 前分别独立完成；
6. `P6-PILOT` 通过后才允许按 `_meta.json.perDocumentTypeStages` 的固定顺序逐类型接入；每个类型的 P3/P4 都是正式阶段对象；
7. 全部逐类型阶段完成后必须执行 `P0-D`，删除为准备期编译保留的旧代码；
8. `P6-FINAL` 是唯一可以把专项合同标记为「已实施」的门禁。

## 3. 阅读顺序

实施人员按下列顺序进入：

```text
docs/dev-plan/README.md            本文件
docs/dev-plan/conventions.md       冻结清单与 owns 规则
docs/dev-plan/domains.md           域归属
docs/dev-plan/<专项>.md            专项阶段矩阵
<专项实施计划目录>                 具体阶段执行文档
```

## 4. 当前专项

| 专项 | 阶段文件 | 实施计划目录 | 状态 |
| --- | --- | --- | --- |
| 审批流程改造 | [approval-workflow.md](./approval-workflow.md) | `docs/approval-workflow-implementation-plan/` | DOC-A 已合并，P0-A 已解锁；DOC-B / DOC-C 合同就绪、待按独立 PR 合并；DOC-D 等待 P3-HTTP |
