# 阶段 17 执行文档验证合同

- 默认调用必须保持原 document handoff 检查。已用原脚本与新脚本在同一当前文档上运行，JSON 报告及 exit 1 逐项相同。
- `--execution` 必须保留 18 阶段、1–15 连续章节，可附第 16 节；允许 `[ ]` / `[x]` 的实际任务记录。状态、input.json、真实输入/代码/证据 Git commit 和必填证据必须相符。
- 前序最高本地门禁通过且真实证据绑定完整时允许下一阶段执行。不得自动升级为已验收。`local_execution_complete` 只有全部 18 阶段本地门禁状态及证据完整时才为 true；本次实际为 false，阶段 17 执行中。
- 公共门禁读取已有命令区块及退出记录，不重跑命令；原合同 scanner 的 exit 1/2 保留，不混成其他公共门禁失败。运行结果不等于人工验收或真实数据库证明。
- `--sources` 与 `--execution` 互斥；业务迁移已开始时拒绝源快照模式。不得用迁移后的源覆盖编制清单。
- 阶段 15 第 6 节只追加 3 条精确非 owned 历史源→实际目标表，注明为阶段 17 适配。仅该三条合同、source-map phase15=0、repository owner15=0 同时成立时，才按范围复核处理；实际空 files.tsv 可表示零行，其他必填空证据失败。
- `--selftest` 包含 6 个测试组，覆盖阶段缺节/重号、模式互斥、缺真实退出、失败重跑、真实 Git 对象/未提交证据/缺字段/缺文件/伪 SHA/状态抬高/数据库声明、精确范围归属与未完成状态。自检只使用 Python 与临时 Git，不运行 Cargo、DB 或历史 tests。

执行：

```bash
python3 backend/docs/superpowers/plans/domain-crate-migration/tools/verify_plan.py --selftest
python3 backend/docs/superpowers/plans/domain-crate-migration/tools/verify_plan.py --execution
```

当前门禁：selftest exit 0、execution exit 0、local_execution_complete false；定向 git diff --check 通过。最终证据/状态更新后由集成负责人再次运行 execution。源码及日志 SHA 见同名 JSON。
