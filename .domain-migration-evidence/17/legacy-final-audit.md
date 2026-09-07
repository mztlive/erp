# 阶段 17 最终旧实现清零审计

## 1. 输入与证据边界

- 唯一工作树：`/private/tmp/erp-domain-crate-17-cutover`。
- 阶段 16 输入：`a537414eb8f78c43ebc383a3457a45c437dececc`。
- 阶段 00 输入/输出：`400ab4f7855255b284fe8a8e1caffe27acc96083` / `475734cc301a8d8516fd29788cfc8ce937a09cb1`。
- 源码提交绑定：`bound`；提交：`39c55021d8bb1d030eade4afb3e135e4b5a8a18b`。
- 本审计只执行 Python 文件读取、TOML/TSV/JSON 解析及 Git 只读命令；不执行 Cargo、历史测试或数据库操作。
- 真实数据库运行未验证；本结果不构成事务运行、完整业务语义或性能阈值验收。

## 2. 核验结果

| 核验项 | 结果 | 实际范围 |
| --- | --- | --- |
| 旧生产树 | pass | 3 个旧 crate 的 6 个 src/manifest 根；33 个实际输入文件逐 blob 哈希与当前缺失记录 |
| 源清单与准备文件 | pass | phase=17 的 7 行；owner_phase=17 为 0 行；全部 141 个准备文件已检查缺失及实际目标文件存在 |
| 历史测试 | pass | 31 个文件与阶段 00 输入、00 输出、16 输入逐字节一致；其中旧三层路径下 27 个 |
| 第三方锁定 | pass | 516 个第三方完整 package entry 比较，包含依赖与 checksum |
| Manifest | pass | 35 个 manifest，34 个成员；normal/build/dev、target、workspace 继承、package 重命名与 path |
| 实际依赖图 | pass | 550 个 packages，550 个 resolve nodes，19 个实际业务域；kind=test 为 0 |
| 活动源码旧引用 | pass | 1837 个源文件，31218 个 use 路径；含内联 cfg(test)、test-support、活动 target 与 build.rs |

## 3. 可复核文件与命令

- 完整逐文件/逐第三方包/逐依赖边记录：`/private/tmp/cutover17-legacy-final-audit.json`；SHA-256：`bfb8c1bb7115832204c82e3a97524ea07505b01369b456b6206b04136173ce16`。
- 实际 metadata：`/private/tmp/erp-cutover17-metadata-final.json`；SHA-256：`b90db074d9e0834139fddbd10e9413a4e8eef8fffb8a065a6fc7ce74715e4cdf`。
- metadata 由集成负责人实际执行 `cargo metadata --format-version 1 --locked` 生成，退出码 0；本审计只解析该产物。
- 本脚本逐条记录 `git ls-tree`、`git ls-files`、`git show`、可选提交绑定的实际参数、退出码及 stdout SHA-256；记录位于 JSON `commands`。
- 现有源码扫描器被导入为纯函数使用；其脚本哈希与全部读取文件哈希登记于 JSON `files`。没有调用扫描器的 Cargo 入口。
- planning source_sha256 保留在逐行证据中；删除核验使用阶段 16 实际 blob SHA-256，不将早期编制哈希误当当前输入。

## 4. 封存条件

1. root 冻结源码后，以同一输入和 metadata 重跑本审计，并传入 `--source-commit <commit>`。
2. 所有已读当前文件必须逐字节等于该提交 blob；所有缺失旧路径在提交树中也必须缺失。
3. 输入、manifest 或源码有后续变更时必须重跑，不复用旧哈希作当前证明。

## 5. 失败项

- 无。本结论仅覆盖本记录列明的静态与依赖图范围。
