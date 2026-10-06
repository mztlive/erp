# 供应商门户接口执行合同

版本：1.1

修订日期：2026-10-05

状态：修复接口合同；质量门禁按交付验收文档登记；真实运行验收未执行

本合同补充 [供应商门户建设与执行合同](supplier-portal-contract.md) 的接口、权限和任务约定。接口交付状态以实际路由及验收证据为准。

## 1 身份与响应

1. 所有接口使用统一 `ApiResponse`。金额、税率和数量使用十进制字符串；未提供数量使用 `null`。
2. 门户凭证主体为 `supplier_portal`，账号类型为 `supplier`，携带账号版本和绑定版本。服务端每次重读账号、绑定及供应商启用状态。客户端不得指定当前供应商。
3. 内部凭证主体为 `backoffice`，账号类型为 `admin`。两类凭证不得互相使用。
4. 门户会话独立保存；退出或失效只清理相应会话及查询缓存。
5. 未知对象和不属于当前供应商的对象统一返回 404。身份失效返回 401；只读人员写入返回 403；非法输入返回 400 或 422；版本及命令内容冲突返回 409。
6. 业务写命令必须携带 `idempotency_key`；登录和自助密码修改采用独立身份生命周期规则。网络中断或结果未知时保留输入和原操作号，由用户恢复原命令；优先返回原回执，无回执时按原内容及原操作号重新授权和安全执行。同号不同内容必须拒绝。
7. 申请及可供写入使用字段允许列表，不接受内部责任、供应商绑定、审核结果或其他商务字段夹带。

## 2 门户接口

| 方法与路径 | 执行动作与边界 |
| --- | --- |
| `POST /supplier-portal/login` | `{account,password}`；返回独立 token 与当前身份，不接受供应商 ID |
| `GET /supplier-portal/session` | 返回当前账号、供应商必要名称及门户角色 |
| `POST /supplier-portal/password` | 验证原密码并更换；新密码执行与登录相同的 6–32 位边界，成功后账号版本变化，旧会话失效 |
| `GET /supplier-portal/offerings` | 当前供应商供给分页；支持名称及订货编码、关系和可供状态筛选 |
| `GET /supplier-portal/offerings/{id}` | 当前条款、可供及版本；只返回外部字段 |
| `GET /supplier-portal/offerings/{id}/revisions` | 本供应商供给的条款历史 |
| `POST /supplier-portal/offerings/{id}/availability` | 必填可供版本、`AVAILABLE` 或 `OUT_OF_STOCK`、数量、原因及操作号；API 来源只读 |
| `GET /supplier-portal/catalog` | 本供应商定向开放的精确 SKU 目录，不含公司销售价格 |
| `GET /supplier-portal/dictionaries/{kind}` | `brand`、`category`、`unit` 独立候选，分类返回完整路径及 `hierarchy` 根叶链，单位包含允许精度 |
| `GET /supplier-portal/category-mapping-suggestion` | 按当前供应商、原始完整分类路径及商品类型读取先前确认的建议，必须人工再次核对 |
| `GET /supplier-portal/applications` | 报价、供给变更、新品及合作条款申请列表 |
| `POST /supplier-portal/applications` | 保存报价、条款或停止供给草稿；不写入生效条款 |
| `GET /supplier-portal/applications/{id}` | 本供应商申请原稿、历次提交、对外决定和实际结果 |
| `PUT /supplier-portal/applications/{id}` | 按版本修改草稿、退回或撤回申请；待确认原稿不可修改 |
| `POST /supplier-portal/applications/{id}/submit` | 按版本冻结提交并创建具体处理人的确认任务 |
| `POST /supplier-portal/applications/{id}/withdraw` | 按版本撤回尚未完成申请，同时关闭对应任务 |
| `POST /supplier-portal/new-products` | 保存一个商品及多个 SKU 的新品草稿，允许资料充分的未匹配字典输入 |
| `GET /supplier-portal/cooperation` | 当前商务条款、付款条件及必要采购联系人 |
| `POST /supplier-portal/cooperation/applications` | 保存独立的合作条款草稿 |
| `POST /supplier-portal/batch` | 按明确模式预检整批、分单元执行及恢复原操作结果 |
| `POST /supplier-portal/applications/{id}/files` | 当前申请归属验证后上传受控素材 |
| `GET /supplier-portal/files/{id}/download` | 重验申请或自己供给关联、文件版本与治理状态后下载 |

请求参数与响应字段采用后端领域及 Process 公开 DTO 的序列化名称。`Instant` 及供应商实际报送时间采用 Unix 秒整数。前端不得用内部管理端商品、文件或字典查询替代门户接口。

素材上传查询必须提供 `expected_version` 和 `idempotency_key`；成功返回文件标识、文件版本和 `request_version`，页面以返回申请版本继续编辑。同一文件内容及原操作号可以恢复原结果。第一版素材支持 JPEG、PNG、WebP 图片及静态 PDF 资料，文件不超过 5 MiB；扩展名、MIME 与实际内容一致，拒绝伪造文件头、损坏或截断内容。格式完整性校验不代表外部病毒检测结果，具体检查及治理执行第 3.5 节。

素材下载查询必须且只能提供一个来源：`request_id`（本供应商新品原稿或历次提交）、`offering_id`（自己的供给当前图片）、`sku_id`（当前仍定向开放的 SKU 当前图片）。SKU 未配置自身图片时可回退至当前商品修订的公共图片，必须冻结并核对真实商品、SKU、当前修订及素材来源版本；不得按任意历史素材回退。文件标识本身不构成访问权。下载不返回对象存储地址，读取前后重验来源、治理与文件版本，并核验登记内容指纹和真实字节。

## 3 申请与审核

1. 供给申请快照用 `kind` 区分 `EXISTING_QUOTE`、`TERMS_CHANGE`、`STOP_SUPPLY`。首次报价保存 SKU、`target_version` 与供应商订货编码；后两类保存自己的供给及提交时版本。
2. 新品输入保存名称、商品类型、品牌原始值、分类完整路径、图片与附件；每个 SKU 保存稳定行标识、规格、原始单位、包装、条码、订货编码、供货条款、数量和实际报送时间。
3. 原始字典输入与内部规范化结果分别保存。选择公司字典不能覆盖供应商品牌、分类路径或单位原文。审核必须显式指定匹配目标及完整核对版本。单位同义执行第 3.2 节的显式确认；单位含义、包装、规格或价格变化必须退回供应商确认。
4. 提交与撤回请求为 `{expected_version,idempotency_key}`。审核请求必须携带申请版本、任务 ID、任务版本、操作号和 `approve` 或 `return`；退回必须填写供应商可见原因。
5. 新品通过还须携带内部字典映射及新商品、供给维护责任；复用商品或 SKU 时携带明确目标和其当前版本。新建 SKU 显式未上架，不把供货价写成销售价。
6. 同一申请的商品、SKU、供给、初始可供、素材绑定、申请结果、任务完成、审计及命令结果必须在同一事务中成功或回滚。
7. 批量每批最多 100 行，表格不超过 5 MB。预检失败时不得开始新写入；执行时每个供给行或新品商品组独立事务。成功单元锁定，失败或未知单元保留原输入与操作号。
8. 新品及首次报价确认必须提供 `availability_reported_at_confirmed=true`，证明内部人员已核对本次全部初始可供报送时间。未配置自动过期时限时不自动判定过期，不得把审核时间写成新报送时间；需要更新的资料退回供应商确认。
9. 包装声明记录原单位、基础单位、每包数量、原单位报价及供应商确认。第一版由供应商明确填写已按基础单位核对的条款和数量，不自动换算或四舍五入；缺少依据、未确认或超出基础单位精度时拒绝提交或确认。
10. 新品批量 `prepare` 保存原草稿供独立补图，`submit` 必须以原草稿 ID 和当前版本整批提交成待确认申请及任务。不得把保存草稿计作最终批量提报，不复制其他申请或素材。批量已有商品报价及调价直接提交申请，可供批量即时更新事实。
11. 分类映射建议查询必须携带 `original_category_path` 和 `product_kind`，供应商标识仅取当前身份。历史映射以供应商、原始完整路径和商品类型为唯一作用域。`confirmation_required` 仍须显式选用；目标或祖先分类的状态、版本、名称或层级变化时返回 `recheck_required`，不得自动应用或改写历史归属。确认记录在新品生效事务内保存真实核对人、时间、原因及分类路径版本。

### 3.1 首次报价目标依据

`snapshot.kind=EXISTING_QUOTE` 的 `snapshot.target_version` 为必填对象，字段如下；标识和版本来自供应商实际查看的定向目录，不能由内部审核人事后补造。

| 字段 | 内容 |
| --- | --- |
| `sku_version` | 目标稳定 SKU 版本 |
| `sku_revision_id`、`sku_revision_version` | 当前 SKU 修订身份及版本 |
| `product_id`、`product_version` | SKU 当前所属商品身份及版本 |
| `product_revision_id`、`product_revision_version` | 当前商品修订身份及版本 |
| `unit_id`、`unit_version` | SKU 当前基础单位身份及版本 |

所有版本必须为正整数，标识不得缺失。保存、提交及确认检查正式依据；确认在正式写事务中再次读取并比较全部字段，同时检查商品、SKU、单位启用及引用归属。任何字段变化返回冲突，供应商必须重新核对后修改可编辑申请并提交；待确认原稿及历史不得由审核人替换版本。门户名称、图片、规格及基础单位采用当前正式事实；规格展示和搜索使用稳定 SKU 的正式规格签名。

### 3.2 新品内部映射

审核 `normalized_product` 保留 `name`、`brand_id`、`brand_version`、`category_id`、`category_version`、`sku_mappings`，并必须携带 `category_hierarchy`。该数组从根到所选叶分类，每项为 `{id,version,name,parent_id,product_kind}`；根节点 `parent_id=null`，其他节点的父级必须等于前一节点 ID，叶身份及版本必须等于所选分类。分类候选的 `hierarchy` 提供同一结构。最终事务逐级比较当前节点的身份、版本、名称、父级、商品类型及启用状态；任何祖先变化拒绝沿用旧核对。

`sku_mappings` 每项为 `{row_id,name,unit_id,unit_version,unit_synonym_confirmation,target_sku}`。不需要同义确认时 `unit_synonym_confirmation=null`；原始单位不同于目标名称、代码或符号时必须提供：

```json
{
  "original_unit": "pcs",
  "same_unit_meaning_confirmed": true,
  "reason": "原稿按单件计价和填报数量，pcs与件表示同一基础单位，无包装换算"
}
```

`original_unit` 必须与本行供应商完整原文严格一致，`reason` 非空且不超过 500 字；同义确认不能改变数量、价格、包装或报价依据，不能把“箱→瓶”作为同义映射。实际换算须由供应商确认并形成新的原稿。`target_sku` 为显式复用身份 `{sku_id,version,revision_id}`；`existing_product` 为 `{product_id,version,revision_id}`。新建及复用都在最终事务重验身份、授权、版本和条码归属。

跨申请批量应用映射必须由内部人员明确勾选，逐张核对原始单位、包装、分类路径及商品类型的一致性。应用只填写各申请的内部映射输入，不能改变供应商原稿或自动提交决定；各申请分别发送含自身申请版本、任务版本及操作号的审核命令，并独立重验当前字典和目标版本。

### 3.3 新品批量两阶段命令

`POST /supplier-portal/batch` 请求为 `{mode,phase,validate_only,recovery_only,rows}`，各行是 `{row_id,idempotency_key,input}`。`mode` 使用 `availability`、`quote`、`terms` 或 `new_product`；仅 `new_product` 必须提供 `phase`，其余模式不得提供非空阶段。

| 阶段 | 行 `input` | 原子结果 |
| --- | --- | --- |
| `mode=new_product,phase=prepare` | `{input: NewProductInput,expected_version:null}`；服务端使用外层行操作号 | 按商品组创建原草稿，返回 `result.id`、`result.version`、`result.status=draft`；不创建任务或正式商品供给 |
| `mode=new_product,phase=submit` | `{id: 原草稿ID,expected_version: 补图后的当前版本}`；使用与准备不同的稳定行操作号 | 提交同一原草稿，返回原申请 ID、版本及 `pending` 状态，同事务建立内部审核任务；不重新创建申请、不对外返回内部任务细节 |

供应商在准备结果关联的原草稿补齐图片和资料，再生成最终提交命令。每条 SKU 必须有自身图片或可共同引用的商品公共图片，文件必须属于该申请且治理有效。准备和最终提交分别整批预检：最多 100 条实际 SKU 或供给行，检查归属、当前版本、订货编码重复、资料、字典、条款和处理人；最终提交另外检查全部图片。任何失败阻断本批新写入。预检通过后每个商品组独立事务冻结原稿并创建任务，逐组执行保留部分成功，不提供整批回滚。

`validate_only=true` 只预检，不执行新写入。逐行返回 `row_id`、`status`、`error`、`result`；状态使用 `valid`、`validation_failed`、`succeeded`、`replayed`、`failed`、`unknown`。准备的 `succeeded` 表示草稿保存成功，只有最终提交的成功结果可以登记为批量提报完成。恢复原成功结果返回 `replayed`，成功行不得重新生成命令。

### 3.4 拒绝、冲突及未知结果

1. 明确输入拒绝且确认未写入时，保留输入，恢复可编辑状态并定位错误；修改后建立新的操作号，不沿用异载荷的旧号。
2. 409 冲突保留输入及当前操作依据，先读取最新对象和版本，展示变化并要求用户明确核对后才解除旧操作及继续修改；不得只替换版本号自动重试。
3. 网络中断、超时或提交结果未知时，冻结原内容和操作号，禁止会改变命令内容的编辑、复制或重建。用户触发恢复后先读取回执；有回执返回原结果，无回执使用原操作号及原内容重新授权、预检并安全执行。
4. 批量 `recovery_only=true` 表示原命令恢复：回执优先；无回执且整批预检合法时可按原命令执行，并非永久只读查询。恢复过程中权限、状态或版本仍不能确定时保留 `unknown`，不能登记失败或成功。
5. 当前身份与资格重验通过，且原命令预检因已确定的输入、业务状态或版本规则拒绝而仍无回执时，必须在同一事务重验并写入相同动作、操作号及原内容指纹的拒绝终态回执，与可能迟到的原业务争用同一唯一回执。不能仅凭一次查询无回执或一次预检失败解除 `unknown`。封存提交成功后返回 `failed`，其回执重放继续返回同一终态；用户读取当前事实并明确核对后可准备新操作号。封存提交结果未知时继续返回 `unknown`，保留原号恢复，不能重建申请。

6. 恢复整批预检未通过或仅执行恢复预检时，无成功或拒绝终态回执的单元继续返回 `unknown`；本次未执行不能证明旧请求未提交。整个恢复 HTTP 请求因认证、权限或其他错误被拒绝时，客户端仍保留原未知行；只有确定的逐行终态才能解除。恢复执行本次失败也不能排除更早原请求已经提交；没有成功或拒绝终态时继续返回 `unknown`。授权、范围快照或数据库错误不得封存为业务拒绝。

### 3.5 素材检查及治理

| 格式 | 执行要求 |
| --- | --- |
| JPEG、PNG、WebP | MIME 分别为 `image/jpeg`、`image/png`、`image/webp`；完整真实解码，非零尺寸，单边不超过 8192 像素，输出内存预算 64 MiB |
| PDF | MIME 为 `application/pdf`；完整静态 PDF 及经典交叉引用表；实际解析全部对象、引用、页树及内容流，拒绝加密、动作和脚本、嵌入文件、外部引用、对象流及交叉引用流 |

所有格式单文件上限 5 MiB。PDF 上限为 10,000 个对象、100,000 个检查节点、50 层嵌套、500 页、单流 8 MiB 和累计解码 32 MiB。可检查流仅接受无过滤器、最多三层的 `FlateDecode`、`ASCII85Decode`、`ASCIIHexDecode`、`RunLengthDecode`，或独立 `DCTDecode` JPEG 流；JPEG 流执行实际解码及尺寸一致性检查。未知过滤器、无法检查的参数、损坏或预算超限必须拒绝，不能降级为仅文件头检查。

真实内容检查通过只登记 `ContentChecked`，持久化及传输值为 `content_checked`。该中间状态在来源、关联和其他治理条件有效时可用，不能表示病毒检测通过；后续独立安全扫描可转为 `Passed`、`Quarantined`、`Rejected`，序列化值分别为 `passed`、`quarantined`、`rejected`。隔离和拒绝状态阻断绑定及下载。客户端不得提交治理状态；文件读取前后重验当前状态及版本，正式绑定不能阻断后续安全扫描结果。

## 4 内部接口与权限

| 方法与路径 | 所需权限 |
| --- | --- |
| `GET /admin/supplier-portal/accounts` | `supplier_portal_account:list` |
| `POST /admin/supplier-portal/accounts` | `supplier_portal_account:create` |
| `PUT /admin/supplier-portal/accounts/{id}` | `supplier_portal_account:update` |
| `GET /admin/supplier-portal/catalog-grants` | `supplier_portal_catalog:list` |
| `POST /admin/supplier-portal/catalog-grants` | `supplier_portal_catalog:update` |
| `GET /admin/supplier-portal/applications` | `supplier_portal_request:list` |
| `GET /admin/supplier-portal/applications/{id}` | `supplier_portal_request:detail` |
| `POST /admin/supplier-portal/applications/{id}/review` | `supplier_portal_request:review` |
| `GET /admin/supplier-portal/dictionaries/{kind}` | `supplier_portal_request:review` |
| `GET /admin/supplier-portal/applications/{id}/duplicates` | `supplier_portal_request:review`，另验商品和供给对象范围 |
| `GET /admin/supplier-portal/applications/{id}/files/{file_id}/download` | `supplier_portal_request:detail`，另验原稿或历史明确引用 |
| `GET /admin/supplier-portal/offerings/{id}/impacts` | `supplier_portal_request:detail`，另验供给及采购对象范围 |

1. 上述权限不替代供应商及商品对象的 DataScope、维护责任、创建或修改资格和任务处理资格。供给审核只要求本次供给动作所需资格，不统一附加 `supplier:update`；合作条款修改供应商商务档案时才检查相应供应商修改资格。
2. 新品的字典补建沿用各字典维护入口及权限；商品审核权限不授予字典创建权。
3. 账号开通必须绑定已存在且启用的供应商，不赋内部角色或组织。
4. 目录开放仅授予所选 SKU 的必要资料读取与首次报价资格；不授予销售资料或商品修改权。
5. 账号开通请求采用扁平 `{supplier_id,account,name,password,role,idempotency_key}`；更新采用 `{expected_account_version,expected_binding_version,role,active,idempotency_key}`。列表必须明确 `supplier_id`，返回分页，不执行全库扫描。
6. 现存内部角色不因服务启动获得新增门户专项权限。管理员必须显式配置所需专项动作及真实业务范围；推荐模板仅在首次创建或主动应用时包含新动作。普通供应商、SKU 和采购人员目录仍按各自查询权限及范围读取，页面禁止未经授权的后台查询。
7. 未完成履约影响沿冻结的真实供给选源关联、当前采购责任人及开放履约任务投影；缺少选源关联的旧记录不得按供应商和 SKU 猜测归属。提示不取消单据，不修改原金额、条款、库存或付款事实。
8. 采购新建、恢复草稿再次提交、版本及变更提交沿真实 `supplier_offering_source` 重验供给、条款、可供版本及当前关系状态，在最终写入事务中拒绝旧选源依据。永久停止关系必须独立显示履约警告，可供仍为有货时不能隐藏停供事实。

## 5 任务与验证

1. 专用任务类型为 `SUPPLIER_PORTAL_REVIEW`，对象类型为 `supplier_portal_request`，单人确认，不借用多级审批类型。
2. 默认处理人取供给或供应商当前维护人；必须是启用的内部账号，具备本次实际商品、供给或合作条款处理权限及范围，不能只核对门户详情和审核权限。没有合法处理人时阻断提交。
   首次报价或新品指定新维护人时，最终事务同时核对该维护人是启用内部账号、主属组织有效，并具备所承担商品或供给维护动作的权限及范围；处理人的资格不能替代被指定维护人的资格。
3. 任务转交沿用工作项转交流程，记录原处理人、新处理人、版本及原因；新处理人重新核对本次实际业务资格及范围，不能转给无法完成当前动作的人。供应商不能转交内部任务。
4. 通过、退回和撤回必须重验申请与任务版本；该任务不能通过通用关闭入口完成业务决定。
5. 验证范围执行建设合同第 13 节：仅库单元测试、静态及构建门禁，不执行真实 MongoDB、S3、服务写入或 E2E。运行验收须另取得环境授权及独立证据。
