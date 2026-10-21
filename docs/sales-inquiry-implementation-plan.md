# 询价、销售与采购衔接执行计划

基准日期：2026-10-21。状态：本地实现完成；检查结果见 §10，未完成业务验收。需求依据：[询价业务需求合同 v1.0](sales-inquiry-contract.md)。接口依据：[询价接口与存储合同](sales-inquiry-api-contract.md)。

## 1. 执行范围与完成标准

1. 按 P0 至 P6 顺序执行；前阶段出口条件未满足，不得将依赖功能标记为完成。
2. 多轮往返、销售供应商隔离、可供应/部分供应/不供应、额外成本及费用采购转换均为必交付范围。不得将其中任何一项降为备注或无限期后续事项。
3. 本次执行依据为用户 2026-10-21 的实施指令；允许完成本地代码与规定检查。生产数据修改、部署和外部消息发送另行执行相应授权。
4. 每阶段记录修改路径、验证命令及结果、未验证行为和阻塞项。不得以编译通过替代业务验收。
5. 实施前读取根规则、目标子项目规则和目标 crate README；保留无关工作区改动。Rust 及前端代码执行各自现行规范。

## 2. 当前基础与实施定位

以下为 2026-10-21 文档及代码定位，不表示已完成运行验收；实施开始时复核入口是否变化。

| 能力 | 当前入口 | 执行动作 |
| --- | --- | --- |
| 商品池 Excel | `erp-client/features/master-data/lib/export-sellable-excel.ts` | 沿用原导出入口与全部既有列，合并客户选品、配送填写及回传解析 |
| 销售地址分配 | `backend/crates/erp-sales/src/entity/sales_order/delivery_allocation.rs` | 复用数量守恒，补稳定来源分配关联 |
| 销售来源模式 | `backend/crates/erp-sales/src/entity/sales_order/selection_source.rs` | 增加询价来源与逐行数量占用，不混用选品方案身份 |
| 供应商条款 | `backend/crates/erp-supply/src/entity/supplier_offering/offering.rs` | 保留通用条款，新增限定范围的询价报价依据 |
| 正式采购选源 | `backend/crates/erp-procurement/src/entity/purchase_order/offering_source.rs` | 支持精确专项条款来源及版本核验 |
| 供给分配写入 | `backend/crates/erp-processes/src/procure_to_pay/sourcing_create.rs` | 接入询价候选，保持原子分配及审批 |
| 采购地址约束 | `backend/crates/erp-procurement/src/entity/purchase_order/delivery_allocation.rs` | 保持直发与入仓地址边界 |
| 费用采购 | `docs/purchase-expense-entry-contract.md` 及对应现有实现 | 扩展来源预填、额度占用和转换，不新建另一套费用审批 |

## 3. 领域责任与数据所有权

| 模块 | 实施责任 |
| --- | --- |
| `erp-sales` | 询价需求、状态、需求版本、对销售反馈及接受事实；需求与费用稳定身份；转销售数量及费用归属/额度台账 |
| `erp-supply` | 内部供应商报价、报价版本、专项适用条款、内部费用收款及报价依据 |
| `erp-procurement` | 实际采购依据、报价采用来源、费用采购来源及既有采购生命周期 |
| `erp-workflow` | 询价责任工作项、改派、完成及关闭；不新增询价财政审批含义 |
| `erp-processes` | 导入确认、提交/反馈/接受/重询、转销售、选源及费用转换的跨域事务 |
| `erp-read-models` | 独立销售视图、采购视图、工作台及受限追溯查询 |
| `erp-identity` / `erp-audit` | 功能权限、数据范围消费登记、审计动作及资源登记 |
| `erp-support` | 文件资产关系和必要的导入后台任务；不得借用历史期初导入语义 |
| `web-api` | 路由、独立请求/响应合同、权限标注和组合根装配 |
| `erp-client` | 新增询价功能模块；衔接商品池、销售开单、采购工作台和费用采购 |

1. 业务领域禁止直接互相依赖。跨域读取通过消费方窄 Port，跨域写入由命名 Process 编排。
2. 对销售结果不得保存供应商字段。内部报价与对销售反馈通过受控关联连接；供应商明细不得放入共享响应结构。
3. 转单数量和费用额度采用各自稳定来源身份跨版本记账，不得只记录“某轮已转”布尔值。
4. 公开给前端的来源引用由服务端核验并生成，客户端不得自行声明已接受版本、供应商适用资格或已占用金额。

## 4. 阶段安排

### P0：冻结接口、状态与存储合同

- [ ] 逐条映射需求合同 IQ01–IQ24 到模型、命令和验收方式。
- [ ] 定义需求版本、反馈轮次、接受记录、稳定商品/地址/费用身份及跨版本来源台账。
- [ ] 定义销售响应与采购响应字段白名单、文件访问规则和任务责任范围。
- [ ] 定义专项报价如何进入现有采购创建依据；列出通用条款与专项条款校验差异，不允许任意覆盖采购价。
- [ ] 定义转销售及费用转换在草稿、审批、驳回、撤回、作废、正式变更时的占用/释放矩阵。
- [ ] 定义暂估费用确认、费用涨价重新接受、ERP 外费用登记与来源防重规则。
- [ ] 冻结 API 路径、权限码、请求/响应 DTO、错误码、分页方式、导入文件及行数限制；写入配套 API 合同。
- [ ] 设计集合及索引：单号唯一、需求版本唯一、反馈轮次唯一、责任任务去重、命令回执唯一、来源查询及条件余额更新。
- [ ] 明确旧销售/采购无询价来源时的读取行为；制定新增字段和索引迁移及回滚方案。
- [ ] 同步需求合同 §11 列出的受影响条款；修复在线选品缺失文档引用，保持客户已提交选品结果的既有语义。

出口：接口及状态矩阵无未定义写入分支；销售隔离、专项报价和额度归属有唯一实现口径；受影响基线已同步。

### P1：询价基础、客户模板与导入

依赖：P0。

- [ ] 实现询价草稿、需求版本、商品及地址分配、销售维护范围和索引。
- [x] 公司商品池只保留原导出，统一提供带图选品、配送需求和填写说明；原列全部保留，销售发送客户前自行处理内部信息。导入保留 SKU 文本身份，仅消费明确填写的需求字段。
- [ ] 实现文件解析、精确匹配、逐行错误、重复行预览、确认写入及幂等回执。
- [ ] 实现销售询价列表、创建/编辑页、导入预览与错误修正；保持后台刷新时的编辑草稿。
- [ ] 实现提交采购、责任分派及待办，不以普通销售详情读取权替代任务授权。
- [ ] 添加规则及编排库单测，覆盖导入边界、地址守恒、授权和原子步骤失败停止。

出口：销售可将客户选品文件确认成询价并提交，采购仅取得当前责任需求。覆盖 IQ01、IQ02、IQ09、IQ22 的本阶段范围。

### P2：多轮报价、供给结论与额外成本反馈

依赖：P1。

- [ ] 实现采购内部报价、专项适用条件、对销售供给结果和费用明细。
- [ ] 实现可供应、部分可供应、不供应、待确认及逐地址可供量校验。
- [ ] 实现补充需求、发布完整反馈、销售接受、重询、关闭及任务同步。
- [ ] 实现需求版本与反馈轮次分别递增；已发布不可覆盖，内部草稿不对销售开放。
- [ ] 实现费用暂估/确认、含价标记、费用单独重询及接受事实。
- [ ] 实现采购工作台、销售反馈页、逐行差异和完整轮次历史；销售侧只展示供给结果。
- [ ] 核验列表、详情、历史、导出、通知及附件权限，禁止供应商字段透出。
- [ ] 添加状态迁移、任务改派、价格条件、跨视图序列化和直接越权访问库单测。

出口：同一需求可完成至少三轮往返，支持部分/不供应与费用重询，销售无法读取供应商。覆盖 IQ03–IQ09、IQ15、IQ16、IQ24。

### P3：询价转销售与跨版本数量控制

依赖：P2。

- [ ] 实现服务端开单依据及询价来源快照，继承客户、商品、数量和地址。
- [ ] 实现销售明确确认成交价，供给价保持参考语义。
- [ ] 实现按行、地址和数量部分转单；跨需求版本及反馈轮次共享来源余额。
- [ ] 在同一事务内创建销售草稿、登记数量占用及命令回执；销售生效后通过独立命令明确费用归属。
- [ ] 衔接销售草稿编辑、提交、驳回、撤回、作废、生效和正式变更，落实占用/消耗/释放矩阵。
- [ ] 对修改客户、SKU、地址、交期或数量档位执行来源重验，不能保留错误来源标记。
- [ ] 在销售详情展示来源询价及反馈版本，采用销售安全投影。
- [ ] 添加部分转单、幂等、跨轮次超量、版本冲突及失败停止库单测。

出口：无需重录商品及地址即可创建销售草稿；历史来源固定；任何版本组合不能重复转同一数量。覆盖 IQ02、IQ05、IQ10–IQ12、IQ21–IQ23 的销售部分。

### P4：询价报价复用及采购追溯

依赖：P3。

- [ ] 在现有采购依据中加入询价专项条款，并保留通用供给条款入口。
- [ ] 实现“沿用询价结果”，只预填有效候选；条件失效时返回明确原因。
- [ ] 保留手动选源和规则推荐；改选记录原因，推荐提供依据，不改变库存优先及集采直发规则。
- [ ] 校验分批转单与采购是否满足数量档位、起订量、报价期限及地址条件。
- [ ] 在采购创建事务内重验责任、销售来源、专项条款、资格、可供量及最新覆盖。
- [ ] 保持现有库存预占、仓发、商品采购拆单和审批的原子编排。
- [ ] 保存需求来源链和实际报价采用链；采购详情及追溯按权限展示，销售详情不反向透出供应商。
- [ ] 添加专项范围越权、条件过期、改选、直发/入仓分配、幂等和拆单规则库单测。

出口：三种选源方式均可完成正式采购；实际采购成本与销售可见供给价区分；库存足额允许无商品采购。覆盖 IQ06、IQ12–IQ14、IQ21–IQ23 的选源部分。

### P5：额外成本转换费用采购

依赖：P3；完整业务联验在 P4 完成后执行。

- [ ] 实现已接受费用向一张或多张生效销售的归属和金额分配，跨版本累计核算。
- [ ] 扩展现有费用采购来源查询及预览，保持既有人员范围，不以询价任务扩大费用建单资格。
- [ ] 拦截暂估、已含商品价、已办理、范围或额度不足的费用。
- [ ] 实现费用确认转换命令，原子登记来源占用、采购及审批、审计和回执。
- [ ] 复用收款供应商、付款主体、付款条件、费用行数和类别限制；不同结算条件不得错误合单。
- [ ] 衔接撤回、驳回、重提、作废、采购变更和确认发生，完成额度占用及释放。
- [ ] 为财务直接登记增加对应询价费用来源关联与防重；不得同时建立应付。
- [ ] 在采购工作台展示费用待办理、审批中、已办理、剩余额度及异常原因；销售仅查看允许公开的费用进度。
- [ ] 添加分摊舍入、重复转换、跨版本额度、暂估拦截、涨价重验和已发生费用保护库单测。

出口：询价费用可在明确确认后形成独立费用采购；多销售、多轮次不重复计费；商品与费用来源链完整。覆盖 IQ15–IQ20、IQ22、IQ23 的费用部分。

### P6：完整验收与交付准备

依赖：P1–P5 全部满足出口。

- [ ] 完成 IQ01–IQ24 的证据矩阵，分别登记单测、静态检查及业务验收结果。
- [ ] 核验权限生成物、审计注册、数据范围消费登记、索引装配及通知字段隔离。
- [ ] 核验历史无询价来源的销售和采购仍可正常读取及按原流程操作。
- [ ] 验证新增字段、索引和功能开关的发布顺序；关闭入口不破坏既有引用读取。
- [ ] 完成两组业务样例：多轮询价后部分开单及改选；含暂估运费的询价确认、分摊与费用采购。
- [ ] 对真实数据库并发超转、额度超分配、提交结果未知及失败回滚单独登记验证方式和证据；未执行则标记未验证，不以替身测试替代。
- [ ] 分别记录代码完成、检查通过、提交、部署、数据迁移及业务验收状态；未执行事项不得标为完成。

出口：全部需求有可核对结果，未验证项明确列示；完整业务验收未完成前不得宣称全流程交付完成。

## 5. 验证执行约束

1. 实施执行本节规定的库单元测试和静态检查；不得以文档已更新代替代码及行为证据。
2. 后续后端实现只新增和执行库单元测试；不新增、修改或执行集成测试、`tests/`、`--test`、`--include-ignored`、真实 MongoDB/S3 依赖命令或验收 examples。
3. 单元测试覆盖真实规则及生产编排的成功和失败分支；不得通过搜索源码字符串或复制实现证明行为。替身测试只证明编排，不证明真实数据库事务和并发。
4. 前端执行现行 lint、格式检查、类型检查和构建；不恢复单元测试框架，不新增 `*.test.*`。类型检查使用项目现行入口，不虚构不存在的 npm script。
5. 开发阶段按受影响 crate 执行库单测和必要边界门禁；签名变化及提交阶段质量检查按实施当时的 `backend/AGENTS.md` 执行。
6. 权限宏、数据范围、审计、跨领域引用发生变化时执行对应门禁，并纳入必要生成物。
7. 用户未主动要求时，不使用 computer/browser 技能。业务手工验收由指定验收人执行并提供结果；自动运行真实服务验收须遵守当次明确授权及仓库限制，不因本计划列有验收场景而自行执行。
8. 未取得运行证据的 IQ 项应注明“规则已验证，实际业务未验证”或具体缺口，不得留空或默认通过。

## 6. 迁移、发布与回退要求

1. 新增来源字段允许既有无询价订单为空，不伪造历史询价、报价或费用来源。
2. 新集合索引先校验名称、唯一性和查询范围；在领域内定义并由组合根统一登记。需要修改旧集合时提供可重复执行的迁移及回退脚本。
3. 专项报价不可被旧版本采购逻辑当作通用供给消费。发布前明确后端、权限生成物、前端和任务消费者的兼容顺序。
4. 回退优先关闭新增提交、转单及转换入口，保留已生成单据及来源读取。不得删库、删除来源台账或清除既有采购和应付作为回退手段。
5. 存在专项来源单据后，旧代码若无法安全读取或处理，不得直接回退至该旧版本；执行兼容修复或前向修复。
6. 部署与业务验收分别记录。本计划完成或文档合入不代表已发布功能。

## 7. 阶段交付记录模板

每阶段完成时填写以下字段，不记录讨论过程：

| 字段 | 填写要求 |
| --- | --- |
| 阶段及状态 | P0–P6；未开始、进行中、实现完成、验收完成或阻塞 |
| 交付内容 | 实际实现的规则、接口及页面 |
| 修改路径 | 明确文件或模块范围 |
| 验证证据 | 命令、退出结果、对应 IQ 编号及证据位置 |
| 未验证行为 | 数据库并发、回滚、界面或真实业务等实际缺口 |
| 数据变更 | 字段、索引、迁移及回退方案 |
| 版本与发布 | 提交号、是否推送、是否部署，分别填写 |
| 出口判定 | 满足条件或具体剩余工作，不使用“基本完成”代替结论 |

## 8. 本次交付状态

记录日期：2026-10-21。下表登记代码及合同落点；检查以 §10 的实际结果为准。所有阶段均未进行真实数据库、浏览器或业务手工验收，不登记“验收完成”。

| 阶段 | 当前状态 | 已登记交付 | 出口仍须核验 |
| --- | --- | --- | --- |
| P0 | 本地实现及合同复核完成 | `sales-inquiry-api-contract.md`；状态、DTO、权限、分页、导入限制、集合、索引、来源及兼容回退；§9 IQ01–IQ24 映射 | 发布执行人确认权限授予、启动建索引及回退操作条件 |
| P1 | 本地实现完成 | `erp-sales/entity/sales_inquiry`；`erp-processes/sales_inquiry_import`；`sales_inquiry/access.rs`、`tasks.rs`；前端客户工作簿、预览及需求编辑 | 真实导入确认、界面操作和责任任务业务验收 |
| P2 | 本地实现完成 | 独立采销 DTO；不可变需求/反馈历史；任务范围、内部报价/附件；补充、接受、重询及关闭；公开费用确认状态 | 多责任人发布完整性、直接越权、文件流转及三轮往返业务验收 |
| P3 | 本地实现完成 | `order_to_cash/inquiry.rs`；销售来源映射及草稿预留、生效消耗、作废释放；既有销售创建接入来源；前端按依据预填 | 跨轮次余额、销售正式变更、来源安全投影和异键并发防超转的运行验收 |
| P4 | 本地实现完成 | `erp-supply/inquiry` 专项报价/费用依据/累计余额；现有采购 Basis、sourcing、重提及变更接入精确修订、原价税率、地址及额度；改选原因纳入指纹和采购来源 | 正式变更/作废变更的额度调整、三种选源界面及重提条件复验的业务验收；无可核验累计承诺的低起订量分批必须失败关闭 |
| P5 | 本地实现完成 | 明确费用归属；既有费用采购及财务登记可空来源；`sales_inquiry_expense.rs` 占用、调整、发生和释放；采购与财务来源入口及原命令恢复 | 多销售分摊、采购生命周期、财务外部已支付来源防重及既有人员范围的业务验收 |
| P6 | 本地检查已登记；业务验收未执行 | §9 证据矩阵、§10 检查结果及存量体积问题、API 合同发布回退要求 | IQ01–IQ24 实际业务验收；真实数据库并发、回滚和未知提交；提交、推送、部署及索引实际创建单独登记 |

1. 上表“本地实现完成”只登记代码交付；检查结果以 §10 为准，不表示真实数据库或业务验收通过。P0–P6 的原执行项保留为出口核验清单，取得对应证据后逐项确认，不因代码完成批量勾选。
2. 前端询价编辑采用按账号、询价及采购任务隔离的编辑副本；后台查询不重置已输入资料。写命令保存原路径、方法、完整载荷及命令键，未知结果跨刷新保留并核对原命令。
3. 采购专项条款沿用现有库存优先、直发与入仓分流。新增代码不构成库存足额、仓库采购不含客户资料等既有规则已完成本次实际验收的证据。
4. 本次不自动执行业务数据迁移、服务重启、部署、真实数据库测试或验收例程；执行范围继续受仓库测试约束和实际用户授权限制。
5. 询价聚合按实际持久化 BSON 限制为 14 MiB；超限拒绝并提示复制新询价继续，不截断历史。该限制与 5 MiB Excel、32 MiB 解压、2000 配送行、200 需求行及每行 100 地址分别执行。
6. 费用采购允许在稳定来源身份内显式采用新接受轮次，增加金额和条款变化重新核验；已发生或已付款受保护。付款按采购单核销，部分付款不将全部费用来源标为已发生；正式减额在原事务内核验净已付款、净已收票下限，并保护已确认发生或财务登记的费用。验收须覆盖部分付款后允许减至财务下限及低于下限拒绝。
7. 局部重询及新反馈尚未接受期间，最近已接受轮次中未受影响的商品及费用继续按原条件和余额使用；尚未接受重询的受影响范围合并阻断。接受新轮次后，旧轮次不得新增占用或增额；需求版本变化使旧需求版本的接受结果全部失效。接受时重新核验已产生的有效占用，不以发布时余额代替当前余额。

## 9. IQ01–IQ24 实施与验证映射

下列测试名均指向实际生产规则的库单元测试，已随 §10 的整仓库库单测执行。各行只核销对应规则及编排的库测试范围；真实业务验收统一为“未执行”。源码存在、函数名称及替身调用轨迹不作为真实数据库回滚、并发或业务通过证据。

测试路径简称：

- `领域用例`：`backend/crates/erp-sales/src/entity/sales_inquiry/tests.rs`。
- `导入规则`：`backend/crates/erp-sales/src/entity/sales_inquiry_import.rs` 内联测试。
- `导入解析`：`backend/crates/erp-processes/src/sales_inquiry_import/parse.rs` 内联测试。
- `销售来源`：`backend/crates/erp-sales/src/entity/sales_order/inquiry_source.rs` 内联测试。
- `专项规则`：`backend/crates/erp-supply/src/inquiry/tests.rs`。
- `采购专项`：`backend/crates/erp-procurement/src/entity/purchase_order/inquiry_quote.rs` 和 `creation_basis.rs` 内联测试。
- `任务规则`：`backend/crates/erp-workflow/src/entity/work_item/inquiry.rs` 内联测试。

| 编号 | 实施文件或模块 | 已有规则测试 | 尚需独立验证的结果 |
| --- | --- | --- | --- |
| IQ01 | `sales_inquiry_import/{parse,preview,confirm}.rs`；`erp-sales/entity/sales_inquiry_import.rs`；前端 `customer-workbook.ts`、`inquiry-import-dialog.tsx` | 导入解析 `parses_versioned_three_sheet_package_and_locates_formula_rows`、`refuses_archives_with_excessive_expanded_entry_size`；导入规则 `confirmation_rejects_other_actor_version_input_errors_and_unconfirmed_merges` | 正确文件预览确认、错误修正重预览、正式写入原子性；真实文件与界面未验 |
| IQ02 | 导入规则；`order_to_cash/inquiry.rs`；销售来源；`procure_to_pay/inquiry_supply.rs` | 导入规则 `duplicates_are_explicitly_merged_and_distinct_delivery_dates_keep_line_identity`；销售来源 `selection_requires_unique_stable_rows_and_conserved_addresses` | 导入、部分转销售及直发采购的同一稳定地址数量全程一致 |
| IQ03 | `erp-sales/entity/sales_inquiry/lifecycle.rs`；前端询价历史及重询操作 | 领域用例 `three_feedback_rounds_preserve_accepted_and_submitted_history`、`unaccepted_new_feedback_preserves_old_accepted_unaffected_goods_and_fees` | 三轮采销往返、局部重询期间旧接受结果的范围、各轮显示及接受记录业务验收 |
| IQ04 | `lifecycle.rs` 的 `reinquire`、`publish`；需求及反馈独立快照 | 领域用例 `three_feedback_rounds_preserve_accepted_and_submitted_history` | 仅重询价格时需求版本不变，完整界面及任务轮次变化 |
| IQ05 | `lifecycle.rs`、`rules.rs`；销售来源冻结映射及专项需求版本 | 领域用例 `occupied_stable_sku_address_and_fee_identity_cannot_be_rewritten`；销售来源 `draft_source_rejects_customer_sku_revision_due_date_and_address_changes` | 新需求版本、受影响专项报价重验及既有正式单据保持原来源 |
| IQ06 | `erp-sales/dto/sales_inquiry`；`sales_inquiry/access.rs`、`read.rs`；HTTP `handler/sales_inquiry/internal.rs` | 领域用例 `independent_sales_and_procurement_projections_enforce_field_whitelists`；HTTP `sales_and_internal_quote_routes_use_independent_permissions` | 列表、详情、历史、模板、附件及直接内部访问的实际权限隔离 |
| IQ07 | `feedback.rs`；公开价格段及逐地址供量；转销售来源规则 | 领域用例 `only_final_complete_and_address_specific_feedback_can_be_published`、`all_unavailable_feedback_is_publishable_and_close_preserves_history` | 部分供应地址边界、不供应行阻止转单及原因展示 |
| IQ08 | `feedback.rs`、`lifecycle.rs`；服务端合成最终反馈 | 领域用例 `only_final_complete_and_address_specific_feedback_can_be_published` | 多责任人反馈仍有待确认或缺行时的最终发布阻断 |
| IQ09 | `sales_inquiry/access.rs`、`tasks.rs`；任务规则；内部附件当前任务守卫 | 任务规则 `inquiry_grant_expires_with_owner_version_state_or_scope`；领域用例 `current_task_work_is_required_and_reassignment_does_not_revive_old_work` | 改派、补充、发布、关闭后的旧任务 URL、命令及附件立即失效 |
| IQ10 | `ledger/quantities.rs`；`order_to_cash/inquiry.rs`；`service/sales_order/inquiry.rs`；`dto/sales_order/inquiry_basis.rs` | 领域用例 `quantity_ledger_survives_requery_and_rejects_new_version_reset`、`acceptance_rechecks_ledger_changed_after_feedback_publication`；开单依据 `partial_reinquiry_basis_keeps_unaffected_old_acceptance_until_new_round_is_accepted`、`requirement_revision_blocks_every_old_basis_even_for_unchanged_lines` | 多次部分转单与跨轮次、跨版本累计；真实数据库并发未验 |
| IQ11 | `sales_inquiry/execution.rs`；导入确认永久回执；既有销售创建命令及来源 CAS | 领域用例 `stale_version_and_invalid_command_replay_fail_without_changes`；导入规则 `permanent_receipt_recovers_original_result_and_rejects_other_payload_or_scope` | 同键回放、异键并发防超量、未知提交恢复的真实事务行为 |
| IQ12 | 销售来源；`erp-supply/inquiry/terms.rs`；采购专项 | 销售来源 `exact_price_scope_rejects_expiry_quantity_tiers_and_uncovered_address`；专项规则 `expired_quote_changed_address_and_insufficient_batch_are_rejected`、`cumulative_batches_require_explicit_terms_and_verified_commitment` | 报价过期、数量档位、起订量与当前修订在实际转单及重提路径一致 |
| IQ13 | `erp-read-models/purchase_center/creation_basis`；`procure_to_pay/inquiry_supply.rs`；采购选源界面 | 采购专项 `special_cost_tax_and_source_are_frozen_separately_from_general_offering`、`special_quote_revision_is_an_exact_split_and_basis_identity_dimension` | 沿用、改选、推荐三入口的最终校验；改选原因及两条来源链显示；未获证据不得判完成 |
| IQ14 | 既有 sourcing 库存及仓库路径；`procure_to_pay/inquiry_supply.rs` 的仓库来源地址转换 | 复用既有采购地址/库存规则；本次专项仓发完整回归证据待登记 | 库存足额无商品采购；实际入仓采购行不携带客户收件人或地址 |
| IQ15 | `feedback.rs` 费用含价标记；`ledger/fees.rs`；`sales_inquiry_expense.rs` | 领域用例 `estimated_or_included_fee_is_reference_only` | 已含运费与另计运费的界面、分配及采购应付结果 |
| IQ16 | `InquiryFeeConfirmation`；接受与重询记录；费用来源资格守卫 | 领域用例 `estimated_or_included_fee_is_reference_only`、`new_accepted_fee_can_resize_existing_allocation_and_unhandled_reservation` | 暂估阻断、费用重询及涨价后重新接受；实际费用采购路径 |
| IQ17 | `sales_inquiry/fee_allocation.rs`；`ledger/fees.rs`；前端费用归属面板 | 领域用例 `new_accepted_fee_can_resize_existing_allocation_and_unhandled_reservation`、`fees_cannot_be_duplicated_or_release_registered_facts` | 多张生效销售归属、分摊合计及避免逐单重复复制 |
| IQ18 | `sales_inquiry_expense.rs`；费用采购原命令回执；稳定费用台账 | 领域用例 `fees_cannot_be_duplicated_or_release_registered_facts`、`occupied_stable_sku_address_and_fee_identity_cannot_be_rewritten` | 跨反馈版本、多销售及重复转换的实际采购回执和余额 |
| IQ19 | `procure_to_pay/{expense_create,expense_confirm,draft_edit,void_order}.rs`、`change/posting.rs`；`sales_inquiry_expense.rs` | 领域用例 `fees_cannot_be_duplicated_or_release_registered_facts`、`new_accepted_fee_can_resize_existing_allocation_and_unhandled_reservation`、`unoccurred_fee_can_decrease_but_occurred_fee_remains_protected`；财务下限 `purchase_change_amount_must_cover_paid_and_invoiced`；费用来源 `accepted_fee_conversion_can_fill_missing_basis_but_cannot_replace_confirmed_payee`、`purchase_edits_cannot_introduce_forged_fee_sources_or_switch_sales`、`feedback_round_can_change_only_with_identical_stable_fee_binding` | 撤回、驳回、作废、正式变更、已发生及已付款保护完整生命周期 |
| IQ20 | `finance_posting/extra_cost/receipt.rs`；`erp-finance/dto/inquiry_fee.rs`；`sales_inquiry_expense.rs`；`sales_inquiry/expense_read.rs` | 财务回执 `full_payload_rejects_same_key_with_changed_business_input_or_evidence`、`identical_evidence_content_replays_with_new_asset_id_key_and_temporary_reference`；费用候选 `candidate_channels_keep_procurement_and_finance_handling_exclusive`、`financial_response_does_not_serialize_internal_payment_basis`；共用费用占用与已办理保护规则 | ERP 外已支付登记保留来源、阻断费用采购及第二笔应付；原键刷新恢复、真实账务与附件存储未验 |
| IQ21 | `service/sales_order/inquiry.rs`、`service/sales_review/effective.rs`；采购变更专项守卫 | 销售来源 `draft_reduction_and_removed_rows_keep_stable_origin_without_overwriting_snapshot`、`formal_reduction_does_not_release_consumed_quantity_and_cannot_regrow_past_original`；领域用例稳定来源规则 | 正式变更触发现有采购影响及履约处理，已消耗余额和询价历史保持事实 |
| IQ22 | `sales_inquiry/recovery.rs`；导入永久回执；前端编辑及命令存储 | 领域用例 `stale_version_and_invalid_command_replay_fail_without_changes`；导入规则永久回执测试 | 响应丢失、刷新后原键恢复、编辑内容保留及任务/单据不重复；界面未验 |
| IQ23 | 询价 policy 事务；导入确认事务；既有销售/采购/费用/财务原事务来源步骤 | 规则层失效拒绝及回执恢复单测；真实数据库中途失败与回滚未执行 | 任一步失败均无来源、费用、任务或单据部分成功；不得用纯规则测试替代真实回滚证明 |
| IQ24 | `feedback.rs`、`lifecycle.rs`；任务完成和关闭命令 | 领域用例 `all_unavailable_feedback_is_publishable_and_close_preserves_history` | 全不供应发布、关闭、历史读取和不生成任何销售或采购的业务验收 |

## 10. 检查、发布及验收记录

| 项目 | 当前记录 | 证据要求 |
| --- | --- | --- |
| Rust 格式 | 通过，退出 0 | `cargo fmt --all -- --check` |
| Rust 工作区编译 | 通过，退出 0 | `cargo check --workspace --locked` |
| Rust Clippy | 通过，退出 0 | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` |
| 库单元测试 | 受影响的销售、跨域读取、跨域流程三库共 1,894 个通过、0 个失败、41 个既有忽略测试未执行；财务金额下限回归另 1 个通过 | `env -u ERP_TEST_MONGO_URI cargo test -p erp-sales -p erp-read-models -p erp-processes --lib --locked`；财务库过滤 `purchase_change_amount_must_cover_paid_and_invoiced`；未执行集成测试、真实 MongoDB/S3 测试或 `--include-ignored` |
| 领域及 BPM 边界 | 均通过，退出 0；领域门禁包含审计入口检查 | `bash scripts/check-domain-boundaries.sh --cutover`、`bash scripts/check-bpm-boundaries.sh` |
| 权限生成物 | 工作区编译后与暂存生成物无差异，退出 0；修复未改权限声明，未重复执行重建漂移脚本 | `git diff --exit-code -- erp-client/lib/permissions.generated.ts erp-client/lib/audit-actions.generated.ts`；生成物随功能代码一起交付 |
| Rust 源码体积 | 当前扫描为 10 个文件、162 个方法超限；本轮修复的方法及新增进度模块不在超限清单，不登记全量体积门禁通过 | 生产文件 800 行、方法 50 有效行；存量问题不由本次交付核销 |
| 前端格式、lint、类型及构建 | 全部通过，退出均为 0 | `npm run format:check`、`npm run lint`、`npx tsc --noEmit --pretty false`、`npm run build`；未新增前端测试 |
| 差异空白检查 | 通过，退出 0 | `git diff --check HEAD` |
| 真实数据库并发、回滚及未知提交 | 未执行 | 单独提供授权范围内业务验收记录；不得将库单测视为已验证 |
| 浏览器或手工业务验收 | 未执行 | IQ01–IQ24 逐项结果及实际角色、单据身份；未主动要求不使用 computer/browser 技能 |
| 本地提交、推送 | 均未执行；改动保留在本地工作区 | 权限生成物按路径暂存以执行现有漂移检查，不构成提交 |
| 服务重启、部署、索引实际创建 | 未执行 | 分别记录部署版本、启动结果和领域 `ensure` 成功结果 |

发布与回退执行 [API 合同 §11](sales-inquiry-api-contract.md#11-发布迁移与回退)。新增来源为空的旧单保持原业务语义；回退先关闭入口并保留已产生单据及来源，不通过删库、删台账或清应付恢复旧状态。
