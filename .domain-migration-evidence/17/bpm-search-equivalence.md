# 阶段 17 BPM 搜索批处理等价合同

最终来源为 `39c55021d8bb1d030eade4afb3e135e4b5a8a18b` 的 `backend/scripts/check-bpm-boundaries.sh`，当前文件已逐字节匹配实际 Git blob。

- `search_rs` 必须继续使用同一 ACTIVE_RUST_SOURCES 顺序、同一 ERE，以及 grep 的 `-Hn -E` 选项。调用者必须先拒绝空库存。
- 旧函数逐文件执行 grep；新函数以 NUL 分隔参数交给 `xargs -0`，仅按系统 argv 限制分批，路径中的空格及 glob 字符不拆分。输出仍为文件名、原始行号及匹配内容。
- 两侧都将无匹配退出归一为成功；调用者继续依原匹配行数判定重复/缺失。优化不修改 ID 名单、手写 struct 检查、宏定义检查或 ProcessKind 映射规则。
- 本次实际执行 21 个 ERE 的新旧函数输入/输出夹具比较，覆盖九类 ID、手写 struct、宏、重复行、无匹配、多文件及含空格路径；stdout、stderr、exit 全相等。
- 完整生产 BPM 门禁由根代理实际执行 exit 0，原日志及 SHA 绑定在 JSON；本审核未重跑 Cargo metadata，不把函数夹具作为完整 workspace 门禁。

源码、前后函数文本、21 条结果及完整门禁日志 SHA 见同名 JSON。
