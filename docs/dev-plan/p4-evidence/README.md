# P4 历史交付证据效力声明

## 1. 目录性质

本目录仅保存各 P4 批次完成时的交付快照、命令结果和当时已知缺口。文件内容不得作为当前业务、
数据、API 或页面实现合同，不得据此恢复已经废止的字段、状态、接口或交互。

## 2. 现行依据

后续实施与验收必须按下列顺序采用现行合同：

1. `docs/approval-workflow-contract.md`；
2. `docs/erp-data-model.md`；
3. `docs/dev-plan/api-contract.md`；
4. `docs/ui-workspaces/wNN-*.md` 与 `docs/ui-glossary.md`；
5. `docs/dev-plan/P4-frontend.md`。

上述现行合同与本目录证据冲突时，必须修改代码、测试、mock 和验收脚本以符合现行合同，
不得回改历史证据伪造原交付状态。

## 3. 已知失效内容

历史证据中出现的 `claim`、`complete`、`UNCLAIMED`、`IN_PROGRESS`、租约、领取令牌、
`scope=hold`、客户端责任状态以及通用任务动作均已失效。审批与待办迁移必须以
`docs/approval-workflow-contract.md` 第 12 章为准。
