# 阶段 17 性能探针等价验收补充

## 1. 执行范围与输入

- 基线固定为 `/private/tmp/erp-domain-crate-00-measure`，HEAD `400ab4f7855255b284fe8a8e1caffe27acc96083`。
- 候选源码冻结为 `39c55021d8bb1d030eade4afb3e135e4b5a8a18b`。所有最终证据路径均读取该提交 actual blob 并核对当前文件字节相同；初始审计 HEAD 与哈希单独保留在 JSON 的 initial_audit。
- 初始探针合同 SHA256 为 `90b68e088ceb02c499762575b36aee0620c60d46f1edb4971f25206faa2da211`。该版本 finance 补丁不满足序列化等价合同。冻结提交已勘误为同一 `0.00` 文本的 parse 表达；新合同 actual blob 已绑定，旧版本仍作为历史缺陷证据保留。
- 本代理完成静态审计、测试补丁与本轮证据绑定；未自行运行 Cargo、数据库或应用。集成负责人已执行两侧纯内联测试及严格 Clippy，以下结论引用其实际日志。Finance 两项测试最终位于领域 lib.rs，调用公开真实 zero_amount；Customer 替代表达使用 borrowed 局部变量满足 Clippy，未添加 allow。
- 基线执行采用 `400ab4f...` 加临时测试补丁，**新增 4 个测试不属于该原始提交**。执行记录同时保留原始/临时追加字节 SHA256；执行后原三文件已精确恢复。本轮再次核对原字节和整个基线树 clean。
- 精确路径、源 SHA256、Git blob、补丁匹配次数与 18 组前序记录见同名 JSON。

## 2. 固定函数与测量记录核对

| 探针 | 阶段 00 真实片段位置（backend 相对） | 阶段 17 真实片段位置（backend 相对） | 现状 |
| --- | --- | --- | --- |
| customer | `services/src/customer/profile/validation.rs:26` | `crates/erp-customer/src/service/customer/profile/validation.rs:26` | ContactInput 的真实 trait 实现；两侧 before 各一次、after 零次 |
| sales | `services/src/sales_order/command/identity.rs:38` | `crates/erp-sales/src/service/sales_order/command/identity.rs:25` | 实际提交指纹函数；两侧 before 各一次、after 零次 |
| finance | `services/src/receivable/mapping.rs:259` | `crates/erp-finance/src/service/receivable/mapping.rs:181` | 实际零金额 helper；两侧 before 各一次、after 零次 |

三个当前叶在追加测试前的完整 SHA256 分别与阶段 05 Customer、阶段 09/10 Finance、阶段 10 Sales 记录相同。00、05、09、10 的 18 组 `probe.json` 均使用相同 id/symbol/before/after/equivalence；各组记录 5 个有效样本与源码已恢复。这只能证明所记录补丁一致、测量完成，不能证明补丁业务等价。

`measure-incremental.py::load_probe` 只读取路径和片段；`SourceGuard::ensure_before/apply_after/restore` 检查唯一匹配并按字节恢复；`measure` 将 equivalence 文本原样写入证据。脚本不执行 Option、请求指纹或 Amount 等价测试。

## 3. 三项等价结论与既有证据边界

### C-01：客户 Option 映射可接受，原测试没有覆盖探针对照

- 原 DTO 的 `mobile` 两侧均为 `Option<String>`；`required_value` 的签名、返回借用生命周期与数据来源相同。
- `as_deref()` 与 `as_ref().map(String::as_str)` 均将 None 保留为 None，将 Some 借用为原 String 的 str。不会 trim、重新编码或分配字符串；空串和 Unicode 字节均保留。
- 原 `validation.rs` 两项测试调用真实 SaveCustomerProfileRequest 的重放上下文及指纹，其中包含固定客户请求摘要；它们没有直接覆盖 required_value 的 None/empty/Unicode。
- 客户实体 FactSet 的纯规则测试使用 `Fact` 替身，能够验证本域规则，但不能替代真实 CustomerProfileContactInput 的适配器测试。
- 新测试 `contact_required_value_preserves_none_empty_and_unicode` 已追加，直接构造真实 DTO 并调用真实 trait 方法。同次测试也对实际 input.mobile 执行替代表达并核对同一 expected；两侧同次测试均已通过，覆盖真实函数及替代表达；候选以 borrowed 局部变量保存 as_ref 结果后执行 map，不改变测试语义。

### S-01：借用摘要输入可接受，完整请求黄金测试两侧已通过

- 两侧函数均对 `(actor_id, sales_order_id, request)` 进行 serde_json 序列化，再将全部字节送入 SHA256 并 hex 编码；错误仍为 Internal，文案未变。
- `Sha256::digest(payload)` 与 `Sha256::digest(&payload)` 读取相同 Vec 字节；此处 Vec 在返回前不再使用，所有权与借用差别不改变业务结果。
- SubmitSalesOrderRequest、SalesOrderEditableDraftRequest、SalesOrderDraftLineRequest 的字段及字段内注解在旧新路径归一后相同。金额、ID、日期、GoodsLineFields/VoucherLineDraft 仍使用实际迁移类型。
- 原 `card_projection_input_tests::submission_idempotency_identity_is_stable_and_payload_bound` 使用真实 SubmitSalesOrderRequest，但只证明 version 变化会改变 hash；没有固定完整 JSON 字节或固定最终 hash。建单测试使用 json Value，是建单泛型函数测试，不能替代提交请求合同。
- 新 `complete_submission_payload_and_hash_match_frozen_goods_and_voucher_bytes` 覆盖完整 goods/voucher 请求的真实反序列化→重序列化和实际 fingerprint。黄金数据包含 actor/order、版本、幂等键、合同/修订、完整可编辑表头、全部行字段、Unicode、None/空串、定点数和日期；同次测试还对真实 serialized 字节调用 Sha256::digest(&serialized)，核对同一固定摘要；预期不是调用生产 hash helper 动态生成。
- 固定 goods SHA256：`5947b0f9feb013e11f451e34a3297602566a3edc3417eb27ba49f6ea6399b296`；voucher：`a34501e69937f7a41c7a14e46ae7da8e713b3b94ca3f2f16615826065f754b42`。固定黄金字节与实际 Rust 请求类型的序列化、生产指纹及借用字节摘要已在两侧测试中确认。

### F-01：原 finance 补丁拒绝用于等价性能验收

- 两侧实际 Amount 的 FromStr 均委托 Decimal 解析，TryFrom 只检查 `value.normalize().scale()` 上限，然后 `Ok(Self(value))`；不会 rescale。
- 两侧 Serialize 在 JSON 路径输出内部 Decimal.to_string()；BSON wire 路径以该字符串构造 Decimal128。解析 `0.00` 与 `0` 数值相等，但 scale 分别为 2/0，JSON 分别为 `"0.00"`/`"0"`，BSON Decimal128 的指数及原始字节不同。
- 此结论来自真实 Amount 源与已安装 rust_decimal 1.42.1 源的静态复核；该库 `decimal_tests.rs` 对解析 `0` 的默认 Display 明确期望 `0`。新增旧补丁反证已由集成负责人在两侧真实 Amount 类型上运行通过；不是仅凭数值比较判定等价。
- 原 `zero_is_exact_with_two_decimal_display` 只覆盖 Amount::zero()/解析 `0.00`；原 JSON/BSON roundtrip 使用非零样本。它们都是实际类型测试，但不覆盖原探针反例。阶段 00 合同快照只记录类型上限与形态，也没有运行两种零字符串的等价断言。
- 不允许修改金额构造、序列化、配置或函数位置来使错误探针成立。集成负责人已锁定合法替代：保持 before 不变，after 改为 `"0.00".parse::<Amount>().expect("固定零金额必须可解析")`。
- 新 `zero_amount_parse_spelling_preserves_value_json_and_decimal128_wire` 调用真实 zero_amount，比较合法替代表达的数值、scale、完整 JSON、原始 BSON wire 与 Decimal128 展示。
- 新 `former_zero_literal_probe_changes_scale_json_and_decimal128_wire` 显式锁定旧补丁反例：数值相等，但 scale、JSON 和 Decimal128 wire 不相等。
- 原 finance 00/09/10 时长记录必须保留为历史事实并标明非法等价探针，不可作为修正探针的基线或最终阈值依据。修正后须在两个固定实现位置重新测量 check/build。

## 4. 脚本自检的证据分类

- `measure-incremental.py::prove_backup_restore` 在临时文件里放置一段 Rust 文本，验证原始字节替换、故障恢复与哈希。它不编译代码，也不构造客户 DTO。
- `measurement-facts-selftest.py::fixture_measure` 使用 `before_call()/after_call()` 文本和 fake Cargo 回调；验证真实测量控制流的 14 次调用、5 样本、恢复与锁释放。这是测量基础设施测试，不是三项业务等价测试。
- 该脚本的 reference source/AST 对照验证测量算法未变，不证明 Amount 或完整请求行为。
- 禁止将上述模型/文本自检写成“真实探针等价测试已通过”。

## 5. 冻结执行证据与恢复结果

| 证据 | 实际结果 | 边界 |
| --- | --- | --- |
| 基线临时追加 4 项探针测试后的全库 --lib | 3266 passed / 0 failed / 71 ignored，12 个 lib 结果；4 项探针测试逐名 ok | 不是原始 400ab4f 提交自带测试 |
| 冻结候选全库 --lib | 3655 passed / 0 failed / 68 ignored，33 个 lib 结果；4 项探针测试逐名 ok | 绑定 39c55021 actual blob |
| HTTP 黄金测试 | 10 个测试函数全部 ok，矩阵覆盖 494 次真实响应断言 | lib harness 记录 10 函数；494 是冻结矩阵 case 分母 |
| 严格 Clippy | 集成负责人报告 exit 0；sealed 日志记录 Finished dev profile | 未新增 allow 绕过检查 |
| 基线恢复 | 执行记录的 3 个 original_sha256 与本轮读取字节全部相同；git status --porcelain 为空 | 测试临时追加已全部撤销 |

- 基线执行记录：`/private/tmp/erp-cutover17-baseline-probe-execution.json`；日志：`/private/tmp/erp-cutover17-baseline-probe-lib-tests.log`。
- 候选日志：`/private/tmp/erp-cutover17-lib-tests-sealed.log`；严格 Clippy：`/private/tmp/erp-cutover17-clippy-sealed.log`。
- 日志全文 SHA256、基线 augmentation 原字节/执行字节、冻结源码 actual blob 与测试逐名结果已绑定同名 JSON。
- 当前合法探针仍处于 before 状态；各片段 before 恰好一次、after 零次，函数位置未变。性能测量必须在恢复后的干净基线和冻结候选上执行，不带基线临时测试补丁。
- Finance 旧时长记录不得改写或用于新探针阈值；修正后的 check/build 测量由集成负责人另行执行。本报告不宣称已取得新的性能结果或真实数据库验证。
