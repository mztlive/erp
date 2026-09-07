# 增量编译测量执行合同

## 1. 目标与交付

按 2026-09-07 用户指令，编译指标测量已停止，性能验收后置，不作为本轮逻辑迁移完成条件。本合同保留阶段 00 基线及阶段 05/09/10 中间复测的历史规则，并约束后续单独启动的性能验收。目标是减少一次真实内部代码编辑后的 check/build 等待时间，并确认无依赖领域保持 Fresh。

活动测量器为 `backend/scripts/measure-incremental.py`。最终基线与候选必须显式使用同一份已冻结且自检通过的测量脚本、compile-probes.json 和本合同。阶段 00 对照提交 `400ab4f7855255b284fe8a8e1caffe27acc96083` 不含该测量脚本，不得伪造其历史存在性。

## 2. 环境固定

1. 测量使用专用 worktree，包含阶段 00 已验收的真实输入提交；不在用户日常脏工作区更改探针。
2. 基线和候选版本位于同一物理设备，每个版本、场景、命令各有独立 CARGO_TARGET_DIR；不删除或清理日常构建缓存。
3. 明确记录 rustc/cargo 完整版本、toolchain、profile、codegen backend、features、RUSTFLAGS、构建并行数与后台负载。只记录与构建有关的允许字段，不转储整个环境或 config.toml。
4. 当前配置已经使用 nightly、dev incremental、line-tables-only 与本地 Cranelift；基线/候选必须一致。测量期间不得升级工具链、依赖版本、改变 flags/features 或切换 LLVM/Cranelift。
5. 不启动 web-api、数据库、浏览器或外部系统。停止本次测量自己启动的进程；不得停止用户已有服务。
6. 源文件编辑、日志解析、metadata/tree 和格式化都不计入测量区间；Cargo 子进程从启动到退出的 wall time 为样本。
7. 内置 SSD 与当前外部卷的对照属于独立实验，必须各自预热并记录；不能将更换存储与架构改动混在一个提速数字里。

## 3. 三场景补丁

使用 [compile-probes.json](compile-probes.json) 的固定源路径、最终路径、符号和 before/after 对。

| 场景 | 被修改实现 | 等价性检查 |
| --- | --- | --- |
| Customer | 客户联系方式 required_value 的 Option 借用映射 | None、空值、普通/Unicode 字符串返回相同借用内容 |
| Sales | sales_submission_fingerprint 的摘要输入借用 | 完整请求 JSON 字节和摘要完全相同 |
| Finance | zero_amount 对同一0.00文本改用parse调用FromStr | Amount 值、JSON 字符串、BSON Decimal128及小数位数同形 |

财务探针必须保留输入文本 `0.00`。原计划中把该文本改成 `0` 的补丁不能保持 Decimal scale 与 JSON/BSON 表示，已在阶段17纠正；该旧补丁的历史计时不计入最终验收。最终仅允许同一 `zero_amount` 函数对相同文本使用 `FromStr` 与 `parse::<Amount>()` 两种调用写法，先核验实际类型与两侧源码，再完整重测。

这些是已有生产路径内的实现改动；不使用空注释、touch 文件或未被调用的新增函数充当有效样本。测量补丁不提交、不部署，不改变公开 API/DTO 或业务输出。

阶段 05/09/10 必须让相应纯实现保留在目标领域，而不是把探针也迁入公共层或 Process。若前序迁移必需调整上下文使精确片段变化，只能更新同一函数、同一等价改动的匹配上下文；重新固定 before/after 并在两边运行等价性测试，不得更换为更便宜的函数。

每次替换前要求 before 片段恰好匹配一次且 after 尚未出现；不满足即失败，不做模糊或全文件正则替换。

## 4. 脚本命令行接口

阶段 00 实现以下参数：

```text
python3 scripts/measure-incremental.py
  --repo /absolute/path/to/isolated/repo
  --revision baseline|candidate
  --spec /absolute/path/to/compile-probes.json
  --measurement-contract /absolute/path/to/compile-measurement.md
  --scenario customer|sales|finance
  --mode check|build
  --target-dir /absolute/path/to/dedicated/target
  --output /absolute/path/to/new/evidence-directory
  --samples 5
```

- `--repo` 是仓库根，脚本在其 backend 中运行 Cargo；必须验证是专用 worktree，且选定源文件没有既有未提交修改。
- `--revision` 选择 baseline_path 或 final_path。中间阶段复测使用已迁目标路径，输出必须记录实际 commit。
- `--output` 必须是新的空目录；避免覆盖原始验收证据。
- 目标目录必须与 cargo metadata 获取的日常目标目录不同；不能指向另一组正在运行的测量目录。
- 使用 Python 参数数组启动子进程，禁止 shell=True 或把源代码/路径拼进 shell；源文件按原始字节备份和恢复。
- 不提供 cargo clean 选项；脚本只操作自己的探针和输出目录。

## 5. 单组测量算法

1. 保存选定源文件的原始字节、哈希、权限及所用补丁；建立信号处理和 finally 恢复机制。校验当前正处于 before 状态。
2. 收集 Cargo metadata、选定 crate 的反向依赖闭包、目标配置与 commit；这些步骤不计时。
3. 在本组独立 target 上完整预热 before 版本。
4. 再运行一次无修改命令。所有工作区业务编译单元必须 Fresh；记录 no-op 时间。若仍重编译，输出 fingerprint 原因并使本组失败，不继续统计有效样本。
5. 进行一次不计入结果的 after 预热：应用补丁、构建成功、恢复 before，再构建 before 成功。确认补丁编译有效且恢复完成。
6. 重复五次：确保 before 已成功构建 → 替换唯一片段为 after → 运行计时 Cargo 子进程 → 保存样本 → 恢复 before → 不计时构建 before。每轮只计 after 的构建时间。
7. 每轮都记录退出码、实际编译与 Fresh 单元。选定业务 crate 没有 dirty 编译单元时，该次不是有效编辑样本；构建失败、非预期源码变化、环境变化或输出不完整时整组失败。
8. 最后恢复原始字节并校验哈希；失败或 SIGINT/SIGTERM 也必须走恢复。SIGKILL 等无法捕获的中断通过预先落盘的 source-backup 与 recovery.json 恢复，恢复前先核验当前内容属于本次探针。
9. 样本不足五个、任何退出非零或源码未恢复时，输出不得带 passed=true，也不得计算可用于验收的改善率。

## 6. Cargo 子命令

check 模式：

```bash
CARGO_LOG=cargo::core::compiler::fingerprint=info cargo check -p web-api --locked --message-format=json --timings
```

build 模式：

```bash
CARGO_LOG=cargo::core::compiler::fingerprint=info cargo build -p web-api --bin web-api --locked --message-format=json --timings
```

脚本通过环境变量传入本组 CARGO_TARGET_DIR。标准输出按行解析 Cargo JSON，标准错误单独保存。`compiler-artifact.fresh` 为 true/false 区分 Fresh 和编译产物；必须按 package_id、target.name、target.kind、profile 记录，不能仅用包名去重而丢失 lib/bin/build-script 差异。

`build-script-executed` 和没有 `fresh` 的事件不能伪装为编译成功证据；结合 fingerprint 日志与 timings 确定构建脚本触发原因。Cargo timings 显示编译单元与依赖等待，不能把整段 binary 时长直接声称为纯链接耗时。[Cargo timings](https://doc.rust-lang.org/cargo/reference/timings.html)

无修改重复构建的异常必须在发生当次捕获 fingerprint 信息；事后只看缓存目录不足以解释原因。[Cargo 重编译诊断](https://doc.rust-lang.org/cargo/faq.html#why-is-cargo-rebuilding-my-code)

## 7. 输出文件

每组 output 必须包含：

```text
environment.json
metadata.json
reverse-dependencies.json
probe.json
recovery.json
source-backup
warmup/
noop/
sample-01/ ... sample-05/
summary.json
```

每个 sample 目录包括 `cargo.jsonl`、`cargo.stderr.log`、`units.json`、`timings.html` 与 `sample.json`。所有 timings 报告在下一次构建覆盖前复制到对应目录。记录原始源文件内容仅限这三个无凭据探针文件；其他业务配置不得采集。

summary.json 至少包含：

```json
{
  "scenario": "customer",
  "mode": "check",
  "revision": "candidate",
  "commit": "实际提交哈希",
  "valid_samples": 5,
  "seconds": [],
  "median_seconds": null,
  "source_restored": false,
  "noop_all_business_units_fresh": false,
  "measurement_passed": false
}
```

上例为空结构示意，不是已执行结果；成功输出必须包含五个实测秒数、计算后的中位数和真实恢复结果。比较报告还须保存基线/候选原始目录、配置一致性检查及改善率。

## 8. 统计与验收

```text
median = 五个有效 wall time 排序后的第三个值
improvement = (baseline_median - candidate_median) / baseline_median × 100%
regression = (candidate_median - baseline_median) / baseline_median × 100%
```

必须分别检查两个维度：

- 结构：无依赖关系的领域保持 Fresh；Sales 内部修改不重新编译 Finance；Finance 内部修改不重新编译 Sales。公共基础修改导致的合理传播单列，不作为领域隔离失败。
- 耗时：check 的三个场景至少两个改善不低于 30%，任何场景回退不超过 10%；build 的三个场景同样独立满足上述门槛。

入口和真实依赖的 Process/Read Model 重编译、最终二进制链接仍可能发生。中间阶段残留旧大 crate 导致的重编译必须如实记录；不能将中间结果写成最终隔离已经达标。

后续性能验收中，结构通过但耗时不达标时，性能事项保持未通过，依据实测检查宏、单态化、build.rs、feature 传播和入口依赖范围。该结果不改变本轮逻辑迁移完成状态。不得改变样本函数、构建命令、设备或阈值制造通过结果。

## 9. 最终测量事实与独立判定

- `measurement-facts.json` 必须记录实际执行的脚本、spec、合同路径与 SHA256。两个版本必须使用同一冻结版本；路径相同不能替代内容哈希相同。
- 每次 Cargo 原 JSONL 对应 `units-full.json`，保留完整 target/profile/features/filenames 对象及输入 SHA256；原 `units.json` 继续作为兼容分类证据，不用摘要冒充完整 Cargo metadata。
- 每组 `cargo-metadata-full.json` 保存同一次真实 metadata 结果；其默认 target 与该组显式测量 target 分别记录。`build-environment.json` 仅记录允许的构建环境指纹，禁止凭据或整个环境转储。
- `capture-domain-measurement-context.py` 在每组前后记录实际 host、hardware、物理设备、jobs、脚本、构建配置与第三方依赖指纹。两侧显式传入 `--measurement-script`、`--measurement-spec`、`--measurement-contract`；存在配置解析缺口时不得认定环境相等。
- 全部 12 组顺序执行，各用独立 target。目标锁采用排他创建，已有或疑似失效锁一律拒绝，不覆盖正在使用的锁。
- `evaluate-domain-performance.py` 从12组原始样本重算中位数、Fresh/Dirty、环境一致性和每个模式的阈值。样本有效、环境相等、领域隔离、check阈值、build阈值必须分别记录，最终通过须同时成立。
- 19个业务域均须包含在候选隔离判定中；不得只检查Sales和Finance。负载端点只构成观察记录，不能证明全程没有变化；无法支持环境可比时保留失败或未核验状态。
- 所有性能文件保留失败原始样本。未经证据支持不得删除慢样本、替换场景、缩小目标集合或调整通过阈值。
