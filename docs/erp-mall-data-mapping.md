# 商城存量数据与 ERP 规范模型映射

本文约定福利商城存量数据进入 ERP 的兼容边界、映射方式、两期同步方向、导入验收和安全要求。

本文覆盖五张商城存量表：

- `pay_card_sell`
- `erp_customer`
- `erp_supplier`
- `product_spu`
- `product_sku`

这些表用于理解现状和构建兼容层，不作为新 ERP 核心表的结构模板。旧字段只有在业务语义、单位、状态代码和来源责任已经确认后，才允许写入 ERP 正式业务实体。

本文按当前两期已经确认的**单公司、人民币**业务口径设计。旧表存在 `tenant_id` 不代表 ERP 需要多租户；来源数据存在价格字段也不把多币种扩展为两期核心能力。

---

## 1. 结论与约束

### 1.1 规范模型与兼容层必须隔离

商城存量表普遍把基础资料、当前状态、展示配置、汇总指标、外部供应商信息和商城玩法放在同一张表中。ERP 不复制这种结构，而是分为两层：

| 层次 | 作用 | 数据责任 |
| --- | --- | --- |
| **兼容层** | 识别来源对象、保存允许同步的来源快照、维护同步游标、执行字段转换、处理映射差异 | 如实记录商城提供的数据，不补猜业务语义，不直接形成资金、库存和利润事实 |
| **规范业务层** | 保存客户、供应商、商品、销售单、供应关系、库存、票款和经营事实 | 遵守 ERP 的稳定身份、历史版本、正式单据、不可变快照和纠正事实规则 |

兼容层发生变化，不得迫使 ERP 核心表增加商城专用字段。商城专用玩法始终只保留在商城，ERP 的接口、来源快照、诊断区和业务实体均不接收、不保存、不校验玩法字段。

### 1.2 五张表的定位

| 来源表 | 在兼容层中的定位 | 进入 ERP 的方式 |
| --- | --- | --- |
| `pay_card_sell` | 一期卡券销售单商业事实来源 | 通过来源销售单身份、完整快照、内容指纹和映射校验形成 ERP 卡券销售单及版本 |
| `erp_customer` | 客户基础资料候选来源 | 经客户去重、身份确认和联系人拆分后形成客户及历史版本 |
| `erp_supplier` | 供应商基础资料、旧结算配置和旧汇总数据候选来源 | 供应商、联系人、能力、资质、结算条款分别映射；余额和累计值不得直接形成资金台账 |
| `product_spu` | 商城商品聚合信息和陈列信息来源 | 进入商品暂存，拆分为商品族、商城发布内容和供应商商品候选关系 |
| `product_sku` | 商城可售规格、价格库存缓存和供应商 SKU 候选来源 | 进入 SKU 暂存，经规格、单位和供应关系确认后形成 ERP SKU |

### 1.3 不以兼容牺牲正确性

以下规则固定执行：

- 旧表主键只作为来源身份，不直接作为 ERP 主键。
- 旧表字段名相似不表示语义相同；没有数据字典和样本证据时不得自动合并。
- 商城库存、余额、累计销量和是否盈利等汇总值不得直接形成 ERP 正式台账。
- 商城支付状态和支付凭证不得自动生成 ERP 客户回款单。
- 旧表的 `tenant_id` 只作来源审计，不传播到 ERP 核心表，也不据此增加多租户模型。
- 旧表的 `deleted` 不直接映射为 ERP 正式单据删除。
- 金额、数量、税率、时间和状态必须完成明确转换，不允许按字段类型猜测单位。
- 所有映射失败均进入差异任务，不得静默丢弃、默认归属或手工补建重复业务单据。

---

## 2. 规范模型

正式表名、字段和约束以 [`erp-data-model.md`](erp-data-model.md) 为唯一依据。
本文只说明旧商城数据如何进入该模型，不另建一套同义业务对象。

### 2.1 客户与供应商

| 规范对象 | 主要职责 |
| --- | --- |
| `party` / `party_revision` | 企业主体稳定身份、法定名称和历史版本 |
| `customer_account` / `supplier_account` | 同一主体分别承担的客户、供应商业务角色 |
| `party_contact` / `party_address` | 联系人与地址及其有效期 |
| `party_tax_profile` | 税号和税务资料 |
| `party_bank_account` | 加密银行账户及带密钥查询指纹；密钥版本随记录保存 |
| `customer_assignment` | 客户主负责销售和协作销售的有效期归属 |
| `supplier_commercial_profile_revision` | 经确认的结算方式、对账周期和付款条件历史 |
| `supplier_capability_revision` | 实物、虚拟、线下服务、API、印刷等能力历史 |
| `supplier_qualification_revision` | 资质、有效期、附件及适用能力历史 |

`erp_customer` 与 `erp_supplier` 不能各自独立去重后直接创建两个法人主体。
名称、税号、统一社会信用代码、银行资料和联系方式的相似结果都只生成候选，
**绝不自动合并**。销售、采购和财务按职责确认两个来源对象确属同一企业后，才创建
一个 `party`，再分别创建客户和供应商角色。来源 `tax_no` 经格式校验和业务确认确为
统一社会信用代码时，同时写入 `party.unified_credit_code` 和 `party_tax_profile`；
不能确认时只进入税务档案候选，不占用统一信用代码唯一键。旧授信额度、可用额度、
预付款和累计金额不属于当前两期规范基础资料，也不生成期初资金事实。

### 2.2 商品、SKU 与供应关系

| 规范对象 | 主要职责 |
| --- | --- |
| `product` / `product_revision` | 商品族稳定身份、名称、分类、品牌和历史版本 |
| `sku` / `sku_revision` | 正式销售、采购、履约和库存引用的最小稳定身份，以及公司销售可见价、市场价和启停状态 |
| `supplier_catalog_product` / `supplier_catalog_product_revision` | 供应商 SPU 稳定身份及 Excel/API/手工来源修订 |
| `supplier_catalog_sku` / `supplier_catalog_sku_revision` | 供应商 SKU 稳定身份、来源报价及目录版本；API 连接可空 |
| `supplier_product_mapping` | 供应商 SKU 到公司 SKU 的逐 SKU 确认映射；SPU 不形成映射 |
| `supplier_offering` / `supplier_offering_revision` | 公司 SKU 与供应商 SKU 的供给稳定身份，以及采购确认的一件代发/集采两项供给价、集采起订量、区域、可供状态和有效期版本；不设置供给方式字段 |
| `product_publication` / `product_publication_revision` | 商城发布稳定身份，以及展示、销售价、区域、上下架和固定供给版本 |
| `product_revision_media` / `product_publication_revision_media` | 经审核的商品版本媒体和商城发布媒体，统一关联 `file_asset` |
| `warehouse_sku_policy` | 按仓库和 SKU 维护的库存预警策略，不接受来源全局阈值直接覆盖 |
| `stock_movement` / `stock_balance` | 正式库存流水及其可重建余额 |

SPU 可以作为商品族，但所有正式销售、采购和库存动作最终落到 SKU。每个 SKU 必须有一个基础计量单位。
“公司商品池”只是公司 `product` / `sku` 的业务称呼和销售查询视图，不建立
`product_pool_entry` 或其修订。`sales_visible_price_gross`、`market_price` 均属于公司
`sku_revision`；销售资格由启用 SKU、已维护销售可见价和至少一条业务时点有效的
`supplier_offering_revision` 派生。供应商供给仍是独立关系和独立修订，不能复制进公司 SKU。
旧商城价格只能在逐 SKU 确认后成为公司 `sku_revision` 的销售可见价/市场价、商城发布价或
供应商供给价候选；实际采购成本来自正式采购、供应商订单和成本事实。旧商城库存只作来源核对，不能写入 `stock_balance`。

### 2.3 销售单

卡券销售单与实物和服务销售单共用统一销售单实体。卡券销售单固定使用：

- `business_type = VOUCHER`
- 一张销售单恰好一条卡券明细
- 卡券类目和履约期限属于表头版本
- 面额、数量、成交金额、配赠条件和卡形态属于唯一明细版本
- 第一期主责系统为商城；切换时点 T 起商城停止创建 B2B 销售单，全部 B2B 销售单统一由 ERP 服务
- 业务来源（创建入口 MALL/ERP，恒不变）与第一期主责机制分别记录，互不替代

规范对象至少包括：

| 规范对象 | 主要职责 |
| --- | --- |
| `sales_order` | ERP 稳定身份、销售单号、业务性质、业务来源（创建入口 MALL/ERP，恒不变） |
| `sales_order_line` | 跨版本稳定明细身份；卡券单全生命周期恰好一个 |
| `sales_order_revision` | 客户、合同、结算主体、税务、开票、履约期限和内容指纹 |
| `sales_order_revision_line` / `sales_order_voucher_line_revision` | 唯一卡券行的金额、数量、面额、配赠和卡形态 |
| `external_identity_map` / `external_identity_target` | 一期来源商城和来源销售单号到同一 ERP 销售单的永久谱系 |

切换时点 T 起商城停止创建 B2B 销售单，全部 B2B 销售单统一由 ERP 服务；同一张销售单不新建、不换单号、不替换稳定身份。

---

## 3. 兼容层数据结构

### 3.1 外部来源

`source_system` 记录一个可独立识别和对账的外部系统。

| 字段 | 约束 |
| --- | --- |
| `code` | 唯一来源代码，例如一个确定的福利商城生产实例 |
| `system_type` | `ERP`、`MALL` 或 `SUPPLIER` |
| `name` | 业务显示名称 |
| `status` | 启用或停用 |

生产与验证环境使用不同的 `code` 和独立连接配置，不共用来源身份。来源时区和
无时区时间解释规则固化在接口或导入规则版本中。数据库主机名、账号、密码、
Token 和签名密钥不属于该业务表，统一由安全配置管理，不进入业务日志和导出文件。

### 3.2 外部对象身份

`external_identity_map` 保存来源稳定身份，`external_identity_target` 保存它到
一个或多个 ERP 规范对象的可审计谱系。

| 表 | 关键字段与约束 |
| --- | --- |
| `external_identity_map` | `source_system_id`、`object_type`、来源原值 `external_id`、二进制比较键 `external_id_key`、`mapping_status`；按“来源 + 类型 + 比较键”唯一 |
| `external_identity_target` | `internal_object_type`、`internal_object_id`、`relation_role`、有效期、状态和确认审计 |

五张表的来源身份固定如下：

| 来源表 | `object_type` | `external_id` |
| --- | --- | --- |
| `pay_card_sell` | `CARD_SALES_ORDER` | 非空且规范化校验通过的 `sell_order` |
| `erp_customer` | `CUSTOMER` | 字符串化的来源 `id` |
| `erp_supplier` | `SUPPLIER` | 字符串化的来源 `id` |
| `product_spu` | `PRODUCT_SPU` | 字符串化的来源 `id` |
| `product_sku` | `PRODUCT_SKU` | 字符串化的来源 `id` |

客户编码、供应商编码、税号、条码和供应商商品编号都可能为空或重复，不作为未经核验的来源身份键。

根业务身份同一时点只允许一个有效 `PRIMARY` 目标。SPU 或 SKU 确需拆成多个规范
对象时使用多个 `COMPONENT` 目标；多个来源对象合并到同一 `party`、`product`
或 `sku` 时，各来源身份使用 `MERGED_INTO`；来源行到正式修订的谱系使用
`REVISION_SOURCE`。人工合并、拆分和解除映射必须保留旧目标和审计，不能覆盖历史。

### 3.3 导入行与销售单快照

五张旧表的批量导入使用 `legacy_import_batch` 和 `legacy_import_row`：

- 原始 SQL 只在受控临时 ETL 区读取；
- 持久层只保留经过安全白名单和规范化的导入包、manifest、规范化行引用，以及确有
  审计关联需要时对原始文件计算的 `source_file_hmac`；不持久化原始 SQL 的普通摘要；
- `source_file_hmac` 同时保存算法版本和 `hmac_key_version`，密钥只在密钥管理系统中
  保存。密钥轮换不覆盖既有指纹；新批次使用新密钥版本，既有记录继续用所记版本在
  密钥管理系统中受控校验，以重现历史结果；
- 清洗后的白名单导入包通过其 `file_asset.content_hmac + hmac_key_version` 保存独立的
  带密钥完整性值，不能复用原始文件 HMAC 代替合规包完整性校验；
- 每行按“批次 + 来源对象类型 + 来源行键”唯一；
- 行记录解析、映射、导入状态、错误码和正式目标谱系；
- 成功形成正式数据的白名单导入包、manifest、规则版本、成功结果和映射审计长期保留；
  失败合规包及行列明细保留 30 天，失败批次元数据、汇总计数、脱敏错误码和审计长期
  保留；
- 部分成功批次必须把长期成功包与 30 天失败诊断包生成两个独立 `file_asset`，不得把
  两种保留期限的内容混在同一对象中；
- 重放使用原批次或明确的修复批次，不新造来源身份。

第一期持续拉取 `pay_card_sell` 使用专用 `mall_sales_order_snapshot`，而不是另一张
通用影子业务表。快照追加保存来源单号、来源更新时间、商城原状态、规范化白名单
内容、商业 `content_hash`、观察时间、处理状态和成功形成的
`sales_order_revision`。可选原始报文只能以加密受控引用保存。

`content_hash` 覆盖销售状态、客户、合同、结算主体、卡券类目、唯一明细、金额、
税率、开票要求、履约期限。项目名称和业务备注随版本同步但不进入指纹，其变化不产生
版本差异。玩法、卡号、
卡密、绑定、激活、支付状态和展示设置不进入快照或指纹。只有与**当前**销售版本
指纹一致时才记为无变化；A → B → A 的第三次 A 仍保留新观察快照和新版本。

### 3.4 第一期同步水位

持续同步只使用数据模型中的：

- `mall_sales_sync_job`：期初基线、增量、每日全量核对和单号补拉；
- `mall_sales_sync_cursor`：每个商城唯一安全高水位；
- `mall_sales_sync_cursor_tie`：记录高水位时刻已持久化的来源销售单号。

固定算法：

1. 期初基线开始前记录时间 `B`，基线成功后把初始水位设为 `B`，不能设为基线结束时间；
2. 增量查询使用 `[high_water_updated_at - overlap_window, safe_now]`，按
   `(update_time, sell_order)` 稳定排序；重叠窗口由接口契约固定，不由运行人员临时修改；
3. 同秒记录通过 `mall_sales_sync_cursor_tie` 去重；同一来源单、来源时间和内容指纹
   重复时幂等；
4. 分页内所有白名单快照持久化成功后才能提交该页；区间全部读取成功后才推进水位；
5. 比当前已应用来源时间更早的迟到快照直接丢弃，不回退当前销售版本；
   同一来源时间却出现不同商业内容时进入差异任务，不猜测先后顺序；
6. 每日以正式单清单和商业指纹做全量核对，补拉只使用原来源身份。

### 3.5 映射差异

持续销售同步的差异使用 `master_mapping_task`；批量导入的行级错误使用
`legacy_import_row`，必要时创建统一 `work_item`。差异类型至少区分身份冲突、字段
缺失、未知状态、金额或单位异常、关系孤儿和安全违规。

销售、运营、采购和财务只确认各自业务语义；系统管理员只能补拉、重放和技术排障，
不能替代业务确认。处理结果可以确认一对一、合并、拆分、请求来源修复或拒绝导入，
但不能直接修改商城商业事实，也不能绕过正式单据规则补写资金、库存或利润。

### 3.6 第二期发送与确认

第二期统一使用：

- `sales_order_projection_delivery`：卡券销售执行投影及商城确认；
- `product_publication_delivery`：商品发布版本及商城确认；
- `inbox_message`：商城事件去重和处理结果；
- `integration_attempt` / `integration_error_task`：尝试、结果未知、重试和人工处理。

发送/确认使用投递记录 + 定时重试 + 人工处理，不建 outbox 消息表：业务事务与投递记录
同事务提交，每个投递持有稳定 `message_key` 和业务幂等键。
发送失败不得回退已经生效的 ERP 销售事实，应生成接口异常任务并阻断对应商城执行
或发布。结果未知时先按原外部请求号查询，重试仍使用原幂等键。

---

## 4. `pay_card_sell` 映射

### 4.1 字段组映射

| 旧字段组 | 规范目标 | 映射规则 |
| --- | --- | --- |
| `id` | `legacy_import_row.source_row_key` 和来源诊断 | 只作来源行身份，不作为 ERP 销售单身份 |
| `sell_order` | `sales_order.order_no`、`external_identity_map.external_id` | 一期使用“来源商城 + 来源销售单号”映射同一张 ERP 销售单；按 `utf8_bin` 原值做唯一判断，只移除协议明确禁止的首尾空白，不做大小写折叠 |
| `sell_status` | `sales_order.source_status_code`，以及经字典证明的 `commercial_status` | 先原样保存来源代码，再分别转换 ERP 商业状态；不得用它覆盖履约、回款、开票和关闭进度 |
| `company_id`、`parent_company_id` | 客户、结算主体候选外部身份 | 两者的实际业务关系必须用数据字典和样本确认，禁止直接把母公司当结算主体 |
| `company_name`、`company_address`、`company_person`、`company_mobile` | 销售单客户快照 | 与已确认客户映射同时保存，不反向覆盖客户基础资料 |
| `sales_id`、`sales_name` | 来源销售主管及 `document_participant` 候选 | DDL 注释为“销售主管”；确认人员身份和历史职责后记录相应参与角色，不自动改成客户负责人或负责销售 |
| `entry_name`、`sell_msg`、`project_remark` | `sales_order_revision.project_name` / `business_remark` | 按明确的长度和合并规则进入正式商业版本，但不纳入 `content_hash`；附件只保存另有文件证据的内容 |
| `card_type_id`、`category_id`、`card_name` | 卡券类目候选 | 三者语义可能重叠，确认实际主键、枚举和历史变化方式后才能建立卡券类目映射 |
| `card_type` | 储值卡/次卡来源分类候选 | DDL 已给出 `1=储值卡、2=次卡`；禁止当作电子卡/实体卡形态。一期禁止映射到 ERP 卡券类目或正式商业分类字段；必须仅作为来源快照保留，不得据此新建或改写 ERP 分类身份 |
| `sell_card_type`、`order_type` | `sales_order_voucher_line_revision.card_form` 候选 | 必须用代码字典和真实样本确认哪个字段表达电子卡/实体卡，二者冲突时拒绝应用 |
| `card_sell_amount` | 唯一卡券明细数量 | 字符串先完成格式和单位校验；卡券数量必须为大于零的整数 |
| `sell_price` | 成交单价候选 | 必须确认金额单位和是否含税 |
| `total_sell_money` | 成交金额候选 | 必须确认单位，并与数量、成交单价及业务舍入规则核对 |
| `original_price`、`card_value` | 面额或面值小计候选 | 两者的单位和计算关系必须以数据字典及样本确认，不按字段名自动选择 |
| `card_end_time` | 销售单履约期限候选 | 确认为合同卡券有效期后，写入销售单表头版本 |
| `card_end_time_type`、`card_end_time_date_type`、`card_end_time_value` | 履约期限规则来源快照 | 用于解释绝对日期或相对有效期；最终 ERP 商业事实仍保存明确截止时间 |
| `invoice_type`、`tax_point` | 开票要求和税率候选 | 确认枚举和 `6`、`0.06`、`6%` 等单位后转换 |
| `audit_url`、`process_instance_id`、`status` | 存量审批证据候选 | 可证明存量单正式性，但不得伪造成 ERP 内部审批操作 |
| `creator`、`updater`、`create_time`、`update_time` | 来源审计 | 保留来源值；ERP 导入操作人另记为系统集成账号 |
| `deleted` | `legacy_import_row` 或 `mall_sales_order_snapshot` 来源状态 | 原样留痕并先进入差异；只有正式状态证据明确证明作废或关闭时，才按 ERP 状态规则处理，不物理删除 |
| `tenant_id` | `legacy_import_row` 白名单审计值 | 只作来源审计，不映射 ERP 公司或租户身份 |

### 4.2 拒绝照搬

以下字段属于商城执行、玩法、展示或技术流程，不进入 ERP 销售单核心模型：

- `card_list_id`
- `can_use_status`
- `pay_status`
- `activation_time`
- `card_location`
- `is_difference`
- `is_optional_category`
- `yph_status`
- `can_use_goods`
- `show_sell_price`
- `one_more_seat`
- `card_amt_to_wallet_num`
- `sign_status`
- `end_time`
- `end_time_type`
- `card_amt_to_wallet_ratio`
- `card_amt_to_wallet`
- `sequence_status`
- `sequence_amount`
- `sequence_pwd_status`
- `limit_pay_status`
- `limit_pay_value`
- `movie_coverage_region`
- `movie_coverage_area_ids`
- `expire_hide_status`
- `start_hide_day`
- `contact_status`
- `contact_number`
- `contact_start_time`
- `contact_end_time`
- `recharge_flag`
- `max_binding_card`

`card_location` 是商城卡券适用区域，属于玩法执行范围；`sign_status` 是商城签收执行
配置。二者均不进入 ERP 商业模型，不在建设范围内。禁止将 `sign_status` 旧代码写入备注
或其他非正式字段以冒充客户履约事实。

`operators_id`、`operators_name` 在确认其代表运营商、供货方还是其他商城对象前，只能保存为允许列表内的来源诊断字段。

`pay_status` 和 `pay_url` 不进入 ERP，也不形成客户回款事实。期初已收金额仍按第一期票款规则初始化并逐单复核。

`bind_verify_prefix`、`bind_verify_suffix`、`bind_verify_answer` 不进入 ERP；其中验证答案属于禁止同步字段。

### 4.3 P0 阻塞校验

以下任一项未通过时，不得把来源行应用为 ERP 正式卡券销售单：

1. `sell_order` 为空、仅空白、重复或已经被另一张 ERP 销售单占用；
2. 无法确认哪些 `sell_status` 属于草稿、生效、关闭和作废；
3. 无法证明后续关闭或作废的来源单是否曾经达到正式生效状态；
4. 客户、合同、结算主体或卡券类目无法映射；
5. 无法从来源接口或相关数据补齐合同、结算主体和电子卡/实体卡形态；
6. `card_sell_amount`、`sell_price`、`total_sell_money`、`original_price`、`card_value` 的格式或单位未确认；
7. 面额、数量、面值小计、成交金额和配赠公式无法核对；
8. `tax_point` 或 `invoice_type` 代码未知；
9. 履约期限不存在、冲突或无法确定优先规则；
10. 来源快照出现不应进入 ERP 的绑定秘密；
11. `update_time` 未能覆盖商业字段变化，或增量接口不能稳定返回同秒更新记录。

来源表当前状态不足以证明“曾经生效”时，商城必须通过状态历史、审批历史或正式单据清单接口补充证据。

---

## 5. `erp_customer` 映射

### 5.1 字段组映射

| 旧字段组 | 规范目标 | 映射规则 |
| --- | --- | --- |
| `id` | 客户外部身份 | 使用“来源系统 + `CUSTOMER` + 来源 `id`”，不直接作为 ERP 客户主键 |
| `name`、`short_name` | `party_revision` | 名称必填；简称可空；保存完整历史版本 |
| `contact`、`mobile`、`telephone`、`email`、`fax` | `party_contact` | 导入为客户角色的默认联系人候选，后续允许一个主体维护多个联系人 |
| `tax_no` | `party_tax_profile`；经确认时同时写 `party.unified_credit_code` | 先校验格式并跨客户、供应商两表生成候选；不得仅凭税号自动合并。对已确认的客户主体，该值经确认确为统一社会信用代码后同时写两个目标；若另有供应商来源命中相同代码，仍须另行完成人工主体归并后才能共用一个 `party` |
| `tax_percent` | 默认税务提示候选 | 只有确认单位和业务用途后才能成为客户默认值；正式销售仍以合同和销售单版本税率为准 |
| `bank_name`、`bank_account`、`bank_address` | `party_bank_account` | 通过专用安全导入通道加密保存，按财务字段权限访问 |
| `customer_types` | `legacy_import_row` 白名单暂存 | 当前两期没有正式客户分类对象；只解析并保留受控来源值用于迁移审计，不写入 `party`、`customer_account` 或另造分类关系 |
| `status` | `customer_account.status` | 使用客户专用状态字典，不与其他表的 `status` 共用转换 |
| `remark` | 来源业务备注 | 在明确用途前只保留在清洗后的导入证据，不承载结构化分类和结算规则 |
| `creator`、`updater`、`create_time`、`update_time` | 来源审计 | 原样留痕并与 ERP 操作人分开 |
| `deleted`、`tenant_id`、`sort` | 来源状态和展示信息 | 不作为客户稳定身份；删除转停用或差异处理，排序不进入核心基础资料 |

### 5.2 拒绝照搬

- 不把单个联系人字段固化在客户主表。
- 不把单个银行账户固化在客户主表。
- 不把 `customer_types` 字符串继续保存为逗号列表，也不为它新增两期范围外的正式分类对象。
- 不把 `tax_percent` 当作所有销售单的强制税率。
- 不按客户名称自动去重。
- 不把来源 `tenant_id` 直接写入 ERP 公司维度。

### 5.3 分面阻塞门禁

| 分面 | 阻塞条件 | 不受影响的范围 |
| --- | --- | --- |
| 身份暂存 | 有效行名称为空；同一来源 `id` 对应冲突主体；命中多个 `party` 候选且未经销售确认 | 无冲突行可继续暂存 |
| 客户角色启用 | `status` 与 `deleted` 组合无法解释；主体映射未确认 | 已确认主体和历史来源证据仍保留 |
| 财务使用 | 税率单位未知；银行账户未进入加密链路；税务或银行冲突未经财务确认 | 不阻断客户身份、联系人和非资金业务暂存 |
| 来源分类 | `customer_types` 编码未知 | 不阻断任何正式对象；字段保持未映射并只留在白名单暂存，当前两期不生成分类关系 |

客户和供应商候选必须做一次跨表主体归并。匹配用于生成候选，不自动合并；销售、
采购和财务按各自职责确认后，才能让两个业务角色引用同一 `party`。

---

## 6. `erp_supplier` 映射

### 6.1 字段组映射

| 旧字段组 | 规范目标 | 映射规则 |
| --- | --- | --- |
| `id` | 供应商外部身份 | 使用来源主键作为外部键，不直接作为 ERP 主键 |
| `number`、`purchase_number` | 供应商旧编码或别名 | 两者都没有唯一约束；确认后作为外部别名，ERP 供应商编码独立生成 |
| `name`、`status`、`remark` | `party_revision` 与 `supplier_account.status` | 跨客户、供应商角色去重确认后形成主体版本和供应商角色 |
| `tax_no` | `party_tax_profile`；经确认时同时写 `party.unified_credit_code` | 校验并参与跨角色候选匹配，但不得仅凭税号自动合并。对已确认的供应商主体，该值经确认确为统一社会信用代码后同时写两个目标；若另有客户来源命中相同代码，仍须另行完成人工主体归并后才能共用一个 `party` |
| `contact`、`mobile`、`telephone`、`email`、`fax` | `party_contact` | 导入为供应商角色的默认联系人候选，支持一对多 |
| `tax_percent` | 税务提示候选 | 单位和适用范围确认后使用；采购单税率仍以生效单据快照为准 |
| `bank_name`、`bank_account`、`bank_address` | `party_bank_account` | 通过专用安全导入通道加密保存并记录访问审计 |
| `type` | `supplier_capability_revision` 候选 | “普通、云仓、核销”不能直接替代 ERP 的多选能力，需业务映射表和采购确认 |
| `reconciliation_period`、`settlement_type`、`settlement_period` | `supplier_commercial_profile_revision` | 经采购和财务确认后形成版本化商务条款 |
| `credit_status`、`credit_amount` | 来源财务核对字段 | 当前两期不建设供应商授信协议；仅在财务专用清洗结果中核对，不进入规范基础资料或余额台账 |
| 五类证照字段 | `supplier_qualification_revision`、`supplier_qualification_capability` 和附件 | 解析 JSON 后拆分；补充证书编号、机构、生失效日期和适用能力 |
| `summary_time` | 旧汇总快照时间 | 只辅助解释旧累计字段 |
| `creator`、`updater`、`create_time`、`update_time` | 来源审计 | 与 ERP 操作审计分开 |
| `deleted`、`tenant_id`、`sort` | 来源状态和展示信息 | `tenant_id` 只作来源审计，不作为供应商身份或账套身份 |

### 6.2 拒绝照搬

以下字段是旧系统预警设置或汇总快照，不进入供应商核心基础资料：

- `low_balance_warning`
- `balance_available_warning`
- `balance_available`
- `days_available_warning`
- `days_available`
- `redeem_order_price`
- `redeem_order_count`
- `refund_order_price`
- `payment_amount`
- `available_payment_amount`
- `unconfirmed_payment_amount`
- `available_credit_amount`

这些字段不得生成供应商付款、应付、预付款、退款、授信可用额或期初余额。当前两期不迁移旧供应商累计余额和可用额度。

`submit_status`、`recycle_status` 是旧表单流程状态，不直接成为 ERP 供应商生命周期。

`low_balance_warning`、`balance_available_warning`、`days_available_warning`、`credit_status` 使用“0 开启、1 关闭”的反向语义。仅当业务确认迁移预警配置时，必须转换为正向命名的 `enabled`；否则不得迁移，且不得按数据库布尔值直接复制。

### 6.3 分面阻塞门禁

| 分面 | 阻塞条件 | 不受影响的范围 |
| --- | --- | --- |
| 身份暂存 | 来源 `id` 冲突；名称为空；跨表命中多个主体候选且未经确认 | 其他无冲突供应商可继续暂存 |
| 供应商角色启用 | `status`、`submit_status`、`recycle_status` 组合无法解释 | 主体历史和联系人证据仍保留 |
| 采购使用 | `type` 无法映射能力；必要资质缺失、失效或 JSON 无法安全解析 | 不阻断主体和供应商角色暂存 |
| 财务使用 | 银行账户未进入加密链路；税率、结算周期或付款方式代码未知 | 不阻断身份及非资金资料 |
| 正式过账 | 任一旧累计金额被当作付款、应付、实际成本或期初余额 | 所有旧累计字段继续只作核对，不形成正式事实 |

只有经过采购确认的有效能力和必要资质才能用于为公司 SKU 创建或延续有效供给，以及用于采购单。

---

## 7. `product_spu` 映射

### 7.1 字段组映射

| 旧字段组 | 规范目标 | 映射规则 |
| --- | --- | --- |
| `id` | SPU 外部身份 | 使用来源主键建立映射 |
| `name` | `product_revision.name` 候选 | 经商品审核后形成 ERP 规范名称 |
| `category_id`、`brand_id` | `product_category`、`product_brand` 映射候选 | 外部分类和品牌必须先从补充字典建立独立身份映射 |
| `category` | 暂存 `source_product_kind` 候选 | `PHYGOODS`、`VIRGOODS` 等代码须确认；不得与分类树混用，也不得直接写公司 `product_kind`。新建公司商品时仍须业务显式确认独立类型，并补齐线下服务等 ERP 类型 |
| `spec_type` | SKU 规格提示 | 以实际 SKU 和规格数据校验结果为准，不单独决定模型 |
| `keyword`、`introduction`、`description` | 白名单暂存及 `product_publication_revision.sales_description` 候选 | 运营确认面向商城的销售说明后才进入发布版本；关键词等没有规范目标的内容继续只留暂存，不塞入商品名称或备注 |
| `pic_url`、`slider_pic_urls` | `file_asset`、`product_revision_media` 或 `product_publication_revision_media` 候选 | 下载和内容安全校验通过后，经商品或发布审核进入对应版本关系，并保存用途、排序和替代文本；不继续保存逗号字符串 |
| `status`、`delisting_type`、`sort` | 商城发布状态和排序 | 不直接等同 ERP 商品启用状态 |
| `delivery_types`、`delivery_template_id`、`use_delivery_template`、`express_area_ids`、`city_template_id` | 发布能力和 `product_publication_revision.sales_region` 候选 | 运营确认后，区域进入发布版本，能够准确对应的配送能力进入 `product_capabilities`；旧模板 ID 和无法映射的代码只留白名单暂存，不新增旧模板影子表 |
| `supplier_spu_id`、`supplier_type`、`third_channel_id` | 外部供给关系候选 | 进入供应商商品暂存；旧 SPU 只作为来源容器。确认供应商连接后还必须定位或创建精确 `supplier_catalog_sku_id`，才能建立 SKU 映射与供给关系 |
| `category_tax_rate_id` | 来源税收分类引用候选 | 五张表没有被引用对象，必须补充旧税收分类字典并经财务确认；不得把旧 ID 直接认作 ERP 税务分类 |
| `supplier_tax_rate` | 供应关系税率候选 | 不放商品基础资料，写入供应关系或采购条款版本 |
| `create_time`、`update_time`、`creator`、`updater` | 来源审计 | 与 ERP 审核和发布审计分开 |
| `deleted`、`tenant_id` | 来源状态 | 不直接删除 ERP 商品或决定公司身份 |

### 7.2 拒绝照搬

- `price`、`market_price`、`cost_price`、`company_cost_price`、`suggested_retail_price` 不继续放在 SPU 主表。经采购确认的 `price` 与 `market_price` 分别进入公司 `sku_revision.sales_visible_price_gross` 与 `sku_revision.market_price`；`suggested_retail_price` 仅可作为商城发布价候选，供货价候选进入 `supplier_offering_revision`，实际成本来自正式业务事实。
- `stock`、`total_stock` 不形成 ERP 正式库存。期初库存以基准日实盘为准，按 SKU 和仓库导入。
- `give_integral`、`sub_commission_type` 属于商城营销或分销规则。
- `sales_count`、`virtual_sales_count`、`browse_count`、`is_profit` 是派生指标，不进入正式业务事实。
- `charge_prompts`、`store_ids`、`type_ids`、`cake_sync_all_flag`、尺码推荐字段、限购字段和 `replica` 属于渠道内容、旧集成或展示规则，不进入商品核心模型。
- `delivery_types`、`express_area_ids`、`store_ids`、`type_ids` 不继续使用字符串集合。
- 只有 DDL 明确以 `-1` 为默认哨兵的 `price`、`cost_price`、
  `company_cost_price`、`suggested_retail_price` 才能在字典确认后转换为“未维护”。
  `market_price` 出现 `-1` 先记为异常，不静默吞掉。

### 7.3 分面阻塞门禁

| 分面 | 阻塞条件 | 不受影响的范围 |
| --- | --- | --- |
| 历史身份 | 来源 `id` 冲突、名称为空，或无法建立 SPU 与历史 SKU 的来源关系 | 已停用、下架或删除但被历史消费引用的对象仍必须保留映射，不能因不可售而丢弃 |
| 正式启用 | 独立 `product_kind` 未经显式确认、分类与商品类型不兼容；没有可解释的 SKU；SKU 缺基础单位或规范规格冲突 | 可继续保留来源身份和历史追溯；旧商城商品性质候选不能代替业务确认 |
| 商城发布 | 分类、品牌、媒体、区域、发布状态无法确认或不安全 | 不阻断 `product` / `sku` 身份和历史消费归集 |
| 供应关系 | `supplier_type` 语义未知，外部供应商商品身份冲突 | 不阻断商品身份；只阻断创建 `supplier_offering` |
| 库存与成本 | 任何来源汇总库存或价格冲突被尝试写入正式库存、采购成本或利润 | 冲突只进诊断，不阻断历史商品身份 |

历史回填范围必须覆盖 `T` 以前被商城订单引用的全部 SPU/SKU，包括已下架、停用
和来源软删除对象。只有“可用于新销售、采购或发布”的资格需要当前有效状态。

---

## 8. `product_sku` 映射

### 8.1 字段组映射

| 旧字段组 | 规范目标 | 映射规则 |
| --- | --- | --- |
| `id` | SKU 外部身份 | 使用来源主键建立映射 |
| `spu_id` | 所属商品映射 | 必须先找到已暂存或已确认的 SPU |
| `name` | `sku_revision.name` | 与规范规格共同形成 SKU 展示名称 |
| `properties` | `sku_revision_attribute_value` 和 `sku.specification_signature` | 使用补充的属性/属性值字典解析 JSON，按稳定代码排序并生成规格签名 |
| `bar_code` | `sku_revision.barcode` | 空值允许；非空值需检查重复、校验码和历史复用 |
| `weight`、`volume` | `sku_revision.weight_kg`、`sku_revision.volume_m3` | 从来源浮点值转换为定点数；分别固定为千克和立方米，负数、非有限数或单位不明时不应用 |
| `pic_url`、`pic_urls` | `file_asset`、`product_revision_media` 或 `product_publication_revision_media` 候选 | 完成安全校验、用途、排序和替代文本审核后进入相应版本关系 |
| `description` | `product_publication_revision.sales_description` 候选 | 只有运营确认属于商城销售说明时才进入发布版本，否则只留白名单暂存 |
| `supplier_sku_id`、`supplier_type` | 供应商外部 SKU 候选 | 必须结合补充的供应商连接和供应商商品身份，确认后建立 `supplier_product_mapping` / `supplier_offering` |
| `min_nums` | `supplier_offering_revision.bulk_minimum_order_quantity` 候选 | 默认只作为供应商集采起订量候选，经采购确认后写供给版本；不得自动同时写商城最低购买量。只有运营用独立证据确认商城购买约束时，才写 `product_publication_revision.minimum_purchase_quantity` |
| `stock_warning` | `warehouse_sku_policy.minimum_available_quantity` 人工候选 | 来源没有仓库维度，不能直写全局正式值；必须由仓储为明确 `warehouse_id + sku_id` 逐项确认后建立或更新仓库级策略 |
| `status` | SKU 来源状态 | 通过专用字典转换，不直接等同 ERP 启用状态 |
| `create_time`、`update_time`、`creator`、`updater` | 来源审计 | `updater` 的旧类型异常不影响 ERP 审计模型，统一按安全字符串解析来源值 |
| `deleted`、`tenant_id` | 来源状态 | 不直接删除 ERP SKU 或决定公司身份 |

### 8.2 价格、库存和供给拆分

| 旧字段 | 规范去向 |
| --- | --- |
| `price` | 经采购确认后写 `sku_revision.sales_visible_price_gross`；不是商城发布价的自动来源 |
| `market_price` | 经采购确认后写 `sku_revision.market_price`；不得由销售价或成本推导 |
| `suggested_retail_price` | 商城 `product_publication_revision` 销售价候选；仍须运营确认 |
| `cost_price`、`company_cost_price`、`third_channel_cost_price` | `supplier_offering_revision` 供货价或旧成本参考候选，不形成实际成本 |
| `stock`、`total_stock` | 来源库存快照；自有库存以实盘和库存流水为准 |
| `stock_release_day`、`stock_release_count`、`stock_release_time` | 旧商城库存释放策略；除非业务重新确认需要，否则不进入 ERP |
| `supplier_sku_id` | 供应商连接内的外部 SKU 身份，不是 ERP SKU 身份 |

只有 DDL 明确以 `-1` 为默认哨兵的 `price`、`cost_price`、
`company_cost_price`、`suggested_retail_price` 才能在字典确认后转为“未维护”。
`market_price`、`third_channel_cost_price` 出现 `-1` 先进入异常。旧整数价格只有在
确认单位为分后才能精确换算。

### 8.3 拒绝照搬

- `first_brokerage_price`、`second_brokerage_price` 属于商城分销佣金规则。
- `sales_count` 是派生销量。
- `stock_release_*` 是旧商城库存释放实现，不成为 ERP 库存台账规则。
- `stock`、`total_stock` 不替代按 SKU、仓库和动作记录的正式库存。
- 多个成本字段不继续放在 SKU 主表。
- `properties` 不继续以任意 JSON 作为唯一规格定义。
- 来源 `double` 重量和体积不直接进入财务、物流计算；只有成功转换为
  `sku_revision.weight_kg`、`sku_revision.volume_m3` 的定点值才可使用。

### 8.4 分面阻塞门禁

| 分面 | 阻塞条件 | 不受影响的范围 |
| --- | --- | --- |
| 历史身份 | `spu_id` 孤儿；来源 `id` 冲突；无法确定被历史订单引用的 SKU 身份 | 当前不可售不构成历史身份阻塞 |
| 正式 SKU | `properties` 非法、属性或值缺失、属性重复、规格签名重复；基础单位缺失；条码冲突 | 来源行仍保留，可继续用于差异处理 |
| 新供给 | `supplier_type` 未知，或外部 SKU 在同一连接内冲突 | 不阻断 SKU 身份和历史消费归集 |
| 新发布 | 发布价格单位未知、媒体不安全、销售说明/区域/最低购买量未经运营确认，或商城状态无法解释 | 不阻断 SKU 启用和供给确认 |
| 物流使用 | 重量、体积为负数、非有限数或单位不明 | 不阻断不依赖该物流属性的历史追溯 |
| 库存预警 | 来源阈值无法归属明确仓库和 SKU，或仓储未确认 | 不阻断 SKU 和库存流水；不得生成全局预警值 |
| 正式库存/成本 | 来源库存缓存或旧成本被尝试写入正式台账 | 立即阻断该过账，但不阻断 SKU 身份 |

`updater` 的旧数值类型异常只影响来源审计解析，不得写入 ERP 用户身份，也不应阻断
业务身份导入。

---

## 9. 通用转换规则

### 9.1 金额

- 两期正式金额均为人民币，不把 `currency_code` 设计为两期核心业务维度。
- 每个来源金额字段必须确认是人民币元还是人民币分，以及含税或不含税。
- 正式金额使用定点数，不使用字符串和浮点数。
- 含税或不含税单价最多保留 4 位小数。
- 每条明细的含税金额、不含税金额和税额分别舍入到分。
- 表头合计等于已舍入明细金额之和。
- 发票尾差单独记录，不修改原销售或采购单价。
- 旧 `int(11)` 是 32 位整数，不适合作为长期累计金额类型。

### 9.2 数量与计量单位

- 通用数量使用定点数，并关联明确计量单位。
- 每个 SKU 只有一个基础计量单位。
- 卡券数量必须为整数。
- 重量和体积存定点数并显式记录单位。
- 数量字符串在解析前执行空白、千分位、科学计数法、负数和小数校验。

### 9.3 税率

- 规范税率统一保存为明确比例值。
- `6`、`0.06`、`6%` 必须通过来源规则转换为同一结果。
- 客户或供应商默认税率只作录单提示，正式税率保存在合同和单据版本。

### 9.4 状态和布尔值

- 每张来源表使用独立状态字典。
- 不因字段都叫 `status` 就共用枚举。
- 反向布尔值转换为正向语义，例如 `enabled=true`。
- 未知代码不得默认映射到启用、完成或已支付。

### 9.5 软删除与历史

- 来源 `deleted` 原样保存在来源快照。
- 未进入正式业务的来源对象可标记拒绝或失效。
- 已形成 ERP 正式单据的对象收到来源删除标记时先进入差异处理；只有来源状态字典能够明确证明作废或关闭后，才按正式业务规则追加状态版本或纠正事实。
- 客户、供应商、商品和 SKU 的停用保留稳定身份和历史版本。

### 9.6 来源租户

旧表 `tenant_id` 只作为来源审计值。当前两期 ERP 是单公司系统，不建设租户表、租户外键、租户隔离中间件或租户级唯一约束。不同外部系统实例由 `source_system` 隔离，不把来源租户概念传播到 ERP 核心模型。

### 9.7 审计

- 来源 `creator`、`updater` 保存为来源操作人原值。
- 成功映射到 ERP 员工时另存映射身份，不覆盖来源值。
- 导入、自动同步和重放的 ERP 操作主体为系统集成账号。
- 人工映射、拒绝、合并、拆分和解除映射必须记录处理人、时间、依据和前后值。

### 9.8 来源单号与时间

- `pay_card_sell.sell_order` 使用 `utf8_bin`，来源身份默认按原字节语义区分大小写；
  仅移除接口协议明确禁止的首尾空白，不做大小写折叠、全半角转换或字符替换。
- `external_identity_map.external_id_key` 按上述规范化结果的 UTF-8 字节生成并使用二进制
  唯一约束；不能依赖数据库默认大小写不敏感排序规则。
- 所有无时区 `DATETIME` 先按来源系统约定时区解释，再转换为统一时基。
- `card_end_time` 落到 `voucher_expiry_at` 时必须固定“截止时刻是否包含”规则；
  不能直接截成日期丢失当日时刻。页面按业务时区展示。
- 夏令时歧义、空时间、未来异常时间和来源时间倒退均进入差异，不按 ERP 接收时间补猜。

---

## 10. 两期同步方向

### 10.1 第一期：商城向 ERP 提供商业事实

第一期数据方向如下：

```text
商城卡券销售单
→ 安全白名单接口
→ 来源身份与快照
→ 内容指纹
→ 客户、合同、结算主体和卡券类目映射
→ ERP 卡券销售单及不可变版本
→ 应收派生
→ 财务逐单复核回款与开票
```

规则：

- 期初导入商城已生效及之后状态、且未作废的正式卡券销售单；草稿不导入。
- 后续由 ERP 按来源更新时间主动轮询完整快照。
- 快照唯一依据为“来源商城 + 来源销售单号 + 来源更新时间 + 商业内容指纹”；
  只有新快照与当前 ERP 销售版本指纹相同时才不生成新版本。
- 映射失败不阻断商城执行，但不得生成客户应收、收入和经营结果。
- 商城支付、绑定、激活、卡号、卡密和玩法不进入 ERP 第一期主流程。
- 客户、供应商、SPU 和 SKU 存量表可进入基础资料暂存，但只有经责任部门确认的数据才能成为 ERP 正式基础资料。
- 商品和 SKU 的正式库存以基准日仓库实盘为准，不采用商城库存字段。

### 10.2 第二期切换（T 时点）

五张旧表不能直接承载第二期的统一服务。切换还必须取得正式卡券销售单清单、稳定卡实例引用、
原销售单身份、初始余额及商城写入口状态。切换固定执行：

1. P0 与 P1 能力同一生产发布批次就绪，销售和财务分别确认按客户拆分的存量单清单；
2. 商城永久关闭 B2B 建单入口并启用商业字段只读，期间不产生本次范围内的新建、
   变更、制卡、卡实例、余额、支付、取消、退款、完成和余额恢复；
3. ERP 完成最后一次一期增量同步及全量指纹核对，记录最后水位；
4. 记录切换时点 T（`mall_consumption_cutover.enabled_at`）：T 起商城停止创建 B2B
   销售单，全部 B2B 销售单统一由 ERP 服务；切换时点的当前 ERP 销售单版本作为
   第一份执行投影修订，不生成销售版本；
5. 切换不换单号、不复制销售单、不改变应收、回款和发票；
6. 切换完成后停止一期轮询，再开放 ERP 卡券建单和商城员工业务；不设计商城建单
   重开或回退。

### 10.3 第二期：ERP 向商城

卡券执行投影：

```text
ERP 卡券销售单审批生效
→ 不可变销售单版本
→ 投递记录 + 定时重试
→ 商城执行投影
→ 商城确认接收
→ 商城配置玩法、制卡、绑定和激活
```

规则：

- ERP 执行投影只发送销售单身份和版本、客户、卡券类目、履约期限、唯一明细面额和数量、卡形态及生效时间。
- 成交金额、配赠、税率、开票要求和应收不发送给商城。
- 商城玩法、卡号、卡密、绑定和余额仍由商城负责；商城确认接收前在商城侧阻断受该版本影响的执行，ERP 只记录确认结果。
- 同一销售版本和目标商城使用同一幂等键；发送失败不回退 ERP 生效事实。

商品发布不能只发送三个 ID。`product_publication_revision` 至少投递：

- ERP `product_id`、`sku_id`、SKU 修订和发布修订身份；
- 名称、规格、图片引用；
- 分类和面向商城的销售说明；
- 含税销售价、销项税率和基础计量单位；
- 销售区域、上下架或暂停状态、商品能力和有效期；
- 经运营确认的最低购买量；
- 本发布修订唯一固定的 `supplier_offering_revision` 身份。

`product_publication_revision.category_id` 从已审核的 `sku_revision.category_id` 继承，
不能直接使用旧商城 `category_id`；销售说明写
`product_publication_revision.sales_description`，发布图片通过
`product_publication_revision_media` 关联 `file_asset`。旧 `min_nums` 不能自动复制为
`minimum_purchase_quantity`，只有运营独立确认其确为商城购买约束时才写入。

商城确认前该发布修订不标记为商城生效。供货价变化不自动改销售价，停止供应或数据
过期通过新的暂停发布版本处理。

### 10.4 第二期：商城向 ERP

第二期商城必须另行提供以下契约化数据集；五张旧表不包含这些事实。实时接口和历史
回填使用同一 schema、同一业务事实键和同一正式目标，不能为回填维护一套字段更少的
兼容报文。

#### 10.4.1 五类事实共同信封

每个关键事实先以 `inbox_message` 去重，再形成 `mall_order_fact`。共同信封必填：

| 字段组 | 必填字段 | 处理规则 |
| --- | --- | --- |
| 来源身份 | `mall_id`、`source_event_id`、`payload_schema_version` | “来源商城 + 来源事件 ID”在消息层唯一；验签失败不形成正式事实 |
| 事实身份 | `fact_type`、`business_fact_key`、`external_order_no`、该类结果业务单号及 `external_order_version` | 商城订单号不能单独作幂等键；业务事实键在实时与回填之间唯一 |
| 关联 | 后续结果必填 `original_payment_business_fact_key`；取消、退款、余额恢复另填 `after_sales_request_id` | 接收层先解析为 `original_payment_fact_id`；不得按当前订单状态猜测原支付 |
| 时间 | `occurred_at`、`source_sent_at` | `T` 前后只按事实实际发生时间判断，不按 ERP 接收或回填时间判断 |
| 安全 | `signature`、`payload_digest` | 签名原文、Token 和密钥不落业务日志；验签通过后先保存不可变事实再归集 |
| 来源方式 | `data_source` 取 `REALTIME` 或 `BACKFILL` | 只表示传输方式，不改变幂等键、字段含义和正式目标 |

`fact_type` 只允许：

- `PAYMENT_SUCCEEDED`
- `ORDER_CANCELED`
- `REFUND_SUCCEEDED`
- `ORDER_COMPLETED`
- `CARD_BALANCE_RESTORED`

待支付、支付中、取消中、退款中、履约中等中间状态拒绝进入正式事实。五类业务事实键
分别按规范模型固定为：

- 商城 + `PAYMENT_SUCCEEDED` + 商城订单号 + 订单版本；
- 商城 + `ORDER_CANCELED` + 商城订单号 + 取消版本；
- 商城 + `REFUND_SUCCEEDED` + 退款单号 + 退款版本；
- 商城 + `ORDER_COMPLETED` + 商城订单号 + 完成版本；
- 商城 + `CARD_BALANCE_RESTORED` + 恢复单号 + 恢复版本。

ERP 按上述组成字段重新计算规范 `business_fact_key` 并与报文值核对；商城不能发送任意
自定义键。组成字段为空、版本格式非法或重算不一致时拒绝该消息，不以事件 ID 降级替代
业务事实键。

#### 10.4.2 卡实例基线与余额快照

卡实例基线的商城必填内容只有：

- 不能反推出卡号、卡密的不透明稳定卡实例引用；
- 原销售单外部身份，即来源商城和来源销售单号；
- 初始余额；
- 基准时间。

ERP 先用 `external_identity_map` 解析原销售单，再写 `mall_card_instance` 的
`origin_sales_order_source_identity_id` 和 `origin_sales_order_id`。用于历史分析的
`origin_sales_order_revision_id` 由 ERP 在基线落库时从该销售单不可变修订中确定，
商城不传 ERP 内部销售版本、唯一明细 ID 或卡券类目；唯一明细和类目从该修订推导。
只有商城本身能够长期稳定提供来源基线版本时，才可选传 `source_baseline_version`，
它不是接收必填项。

凡会被实时或历史事实引用的卡实例，无论当前有效、已耗尽或已到期，都必须形成基线。
同一稳定引用重复发送且来源销售单身份、初始余额和基准信息完全一致时只确认接收；
发生冲突时保留原基线并追加 `mall_card_instance_correction`，不得覆盖。

周期余额快照必填稳定卡实例引用、快照时间、余额和来源事件 ID，形成
`mall_balance_snapshot`。商城能稳定提供时可选传 `source_snapshot_version`；它只用于
乱序和审计，不取代“卡实例 + 快照时间”的幂等约束。当前有效卡持续提供周期快照。

#### 10.4.3 `PAYMENT_SUCCEEDED`

支付成功事实除共同信封外，必须一次性携带以下完整快照：

| 快照 | 必需内容 | 规范目标 |
| --- | --- | --- |
| 订单 | 商城用户稳定标识、所属客户来源身份、下单时间、支付成功时间、`gross_amount`、`discount_amount`、`freight_amount`、`paid_amount` | `mall_order` |
| 履约地址 | 收货人、收货手机号、结构化地址及履约备注 | 加密后写 `mall_order.address_snapshot_encrypted`，不建设员工主档 |
| 商品明细 | 来源明细 ID、ERP 商品 ID、SKU ID、商品发布修订、名称、规格、数量、含税销售单价、`line_gross_amount`、`allocated_discount_amount`、`allocated_freight_amount`、`paid_amount`、销项税率 | `mall_order_item`；商城必须完成订单级优惠和运费到明细的实际分配 |
| 商城成本 | 供应商单位成本、明细成本合计、成本含税标识、成本进项税率 | `mall_order_item` 的成本快照；含税标识为含税时进项税率必填，禁止用销项税率替代 |
| 支付来源 | 单内来源序号、来源类型、来源金额；卡券来源带稳定卡实例引用，微信来源带微信支付引用 | `mall_payment_source` |
| 来源 × 明细分摊 | 每个支付来源实际分摊到每条商品明细的金额 | `mall_item_funding_allocation` |

商城成本四项属于业务必填契约。缺值或含税成本缺进项税率时，不拒绝已经验签且金额守恒
的支付事实，但该商城成本不能记为 `ACTUAL`：先生成成本差异，按消费发生时点的有效
`supplier_offering_revision` 降级为 `STANDARD`，仍取不到则标记 `NONE`，禁止猜税率
或按零成本计算利润。

支付来源类型**只允许 `CARD` 或 `WECHAT`**。`CARD` 必须携带稳定卡实例引用，
`WECHAT` 必须携带微信支付引用；其他代码拒绝并进入接口差异，不保留“福利账户”或
“其他支付”兼容分支。

金额在人民币分精度完成守恒校验：

- 每条明细 `paid_amount = line_gross_amount - allocated_discount_amount +
  allocated_freight_amount`；
- 明细原价、明细优惠、明细运费、明细实付合计分别等于订单 `gross_amount`、
  `discount_amount`、`freight_amount`、`paid_amount`；
- 每条商品明细的来源分摊合计等于该明细实付，每个支付来源的明细分摊合计等于该来源
  金额，全部支付来源合计等于订单实付。

商城负责提供实际优惠、运费和支付分摊结果；ERP 不按订单金额比例推测任一矩阵。
基础资料、卡实例或成本暂时无法归集时，验签和基本守恒通过的事实仍先保存并标记待归集，
不得丢弃、拒收或生成第二份事实。

#### 10.4.4 取消、退款、完成与余额恢复

| 事实 | 除共同信封外的必填字段 | 规范目标与金额作用 |
| --- | --- | --- |
| `ORDER_CANCELED` | 商城售后请求 ID、原支付业务事实键、取消版本、整单或明细范围、实际取消数量、实际取消金额、原因 | `mall_order_cancel_fact`；只记录取消结果，不冲减消费、支付、成本或应付。发生资金退回时仍须独立 `REFUND_SUCCEEDED` |
| `REFUND_SUCCEEDED` | 商城售后请求 ID、原支付业务事实键、退款单号、退款版本、完成时间；每条分配含原商品明细、原支付来源、实际退款数量和金额 | `mall_refund` 与 `mall_refund_allocation`；按原商品和原支付来源追加消费反向记录 |
| `ORDER_COMPLETED` | 原支付业务事实键、完成版本、实际完成时间 | `mall_order_completion_fact`；不覆盖供应商履约结果 |
| `CARD_BALANCE_RESTORED` | 商城售后请求 ID、关联退款事实键、恢复单号、恢复版本、稳定卡实例引用、实际恢复金额和时间 | `mall_balance_restoration`；只证明原卡余额实际回补，不再次冲减消费、供应商成本或应付 |

每次部分退款和每次余额恢复分别形成不可变事实。商城退款、卡券余额恢复和供应商退款
是三个不同结果：`REFUND_SUCCEEDED` 是商城侧冲减消费和原支付分摊的唯一依据；
`CARD_BALANCE_RESTORED` 只记余额回补；只有供应商退款成功事实才能冲减供应侧成本
和应付。

#### 10.4.5 售后动作请求、实时与回填

`mall_after_sales_request` 不是成功事实。请求必填来源商城、稳定请求 ID、原订单和
明细、取消或退款类型、请求数量/金额、原因和请求时间。只有原支付实际发生时间不早于
`T`、履约链为 `ERP_AUTOMATED` 且已形成 ERP 供应商订单时，该请求才能驱动
Supplier Connector。`T` 前支付的旧单继续原人工售后，商城不向 ERP 提交供应商售后
动作请求；实际取消、退款、完成和余额恢复仍按上述结果事实回流。

历史回填使用 `mall_consumption_backfill_job` 和 `mall_consumption_backfill_item` 记录
批次及逐事实结果，业务时间范围固定为 `[range_start, T)`。回填报文与实时报文使用
完全相同的共同信封、专有字段、成本税字段、支付来源枚举、分摊矩阵和业务事实键；
`data_source` 是唯一传输差异。实时与回填重叠时只确认已存在事实，不重复形成消费、
成本或供应商订单。

### 10.5 第二期供应商补充数据

本节只定义第二期 API 供应商入站数据如何落入既有规范对象，不改变供应商商品库、
供应商 SKU 映射和多供应商供给在第一期已经建设并启用的阶段归属。

API 供应商原始目录、订单、动作结果、退款和账单必须逐项进入既有规范对象，不建立
供应商专用同义表：

| 数据集 | 必需内容 | 规范目标 |
| --- | --- | --- |
| 供应商商品身份与版本 | 供应商、来源类型、可选连接、供应商 SPU/SKU 编码、来源版本或摘要、名称、描述、品牌、规格属性、单位、条码、主图（SKU 1:1）/轮播/详情图、一件代发底价（含税运）、集采底价（含税）、集采起订量、可供数量/状态 | `supplier_catalog_product`、`supplier_catalog_product_revision`、`supplier_catalog_product_revision_media`、`supplier_catalog_sku`、`supplier_catalog_sku_revision`；批次结果另记 `supplier_catalog_intake_batch`、`supplier_catalog_intake_item` |
| 公司 SKU 映射 | 供应商目录 SKU、公司 SKU、确认人、依据和有效状态；供应商 SPU 只作上下文 | `supplier_product_mapping` |
| 固定供给版本 | 公司 SKU、供应商 SKU、一件代发供给价（含税/不含税）、集采供给价（含税/不含税）、进项税率、运费、服务费、区域、可供状态、商品能力、集采起订量和有效期；不设置 `supply_mode` | `supplier_offering`、`supplier_offering_revision` |
| 供应商子订单 | ERP 永久订单号、商城订单及明细、固定供应商和连接、固定供给版本、数量、地址快照、下单成本和进项税率 | `supplier_fulfillment_order`、`supplier_fulfillment_item` |
| 下单、查询、取消、退款动作 | 原供应商订单、动作类型、商城售后请求、稳定幂等键、供应商请求号、脱敏请求/响应摘要和结果 | `supplier_order_action`；结果未知先按原请求查询，重试沿用原幂等键 |
| 状态回调 | 外部事件 ID、供应商状态版本、原状态、新状态、发生时间和接收时间 | `supplier_order_status_history`；按版本和发生时间拒绝状态倒退 |
| 供应商退款成功 | 外部退款单号、供应商订单、实际退款时间；逐分配记录供应商订单明细、原 `cost_entry` / `cost_allocation`、原 `payable_entry`、已付款时的原 `payment_allocation`、退款数量、含税/不含税金额和税额 | `supplier_refund_fact`、`supplier_refund_allocation` |
| 周期账单与差异 | 账单号和版本、期间、供应商订单明细、订单金额、运费、服务费、退款、双方金额和处理结论 | `supplier_settlement_statement`、`supplier_settlement_item`、`supplier_settlement_difference` |
| 成本与应付 | 成本来源、`ACTUAL` / `STANDARD` / `NONE` 口径、含税金额、进项税率、税额、不含税金额及原业务依据 | `cost_entry`、`cost_allocation`、`payable_entry`；退款使用负向应付和 `payable_entry_offset` 冲减原正向分录，结算差额使用追加分录，均不覆盖原记录 |

供应商退款成功是冲减供应商成本和应付的唯一供应侧依据。每条退款分配与反向
`cost_entry`、反向 `cost_allocation`、负向 `payable_entry(entry_type = 供应商退款)`
及其 `payable_entry_offset` 在同一事务提交；原应付已付款时，还须在该事务追加原
`payment_allocation` 的 `REVERSE` 和实际现金 `supplier_refund` 事实。商城
`REFUND_SUCCEEDED` 不能替代供应商退款，供应商退款也不能替代商城消费冲减或卡券余额
恢复。

成本口径由 ERP 按事实来源确定，不接受供应商任意声明：供应商订单确认价、履约最终价、
人工确认价和已确认结算价记 `ACTUAL`；消费发生时点的供给版本回退值记 `STANDARD`；
没有可用成本时记 `NONE` 且不保存零成本金额。

多个供应商目录 SKU 可以多对一映射同一公司 SKU；这些来源记录不是公司商品副本。增加第二供应商只新增该来源 SKU 的映射和供给，不创建或修改公司商品/SKU 修订、销售可见价或市场价。来源媒体必须先归档为受控文件资产，才能作为公司商品新修订的候选；不得用供应商临时 URL 覆盖现有公司图文。

供应商商品库与公司商品在内容字段上同构（名称、描述、规格维度、分类/品牌/单位字典、条码、主图/轮播/详情等），但所有权与修订独立；UI 上 W21 中心页与 W14 商品详情分区及编辑控件对齐，便于对照目录。SKU 主图为 1:1；目录价格为代发底价（含税运）、集采底价（含税）与集采起订量。采购必须逐个供应商 SKU 操作：有同款公司 SKU 的正向路径只建立精确映射和该供应商的双价供给；没有同款时，“加入公司商品池”是反向创建公司商品/SKU 的业务名称，把语义相同字段自动预填为公司商品/SKU 草稿，采购可修改，并在确认销售可见价与市场价后，在同一事务创建公司商品/SKU 及其修订、精确映射和双价供给。两个价格只写公司 `sku_revision`。销售查询仅展示启用、具有销售可见价且有业务时点有效供给的公司 SKU。W14 也可从固定公司 SKU 创建供应商商品/SKU；该正向路径只创建映射和供给。SPU 只提供页面和批量选择上下文，不形成正式映射，也不隐式处理未选择的兄弟 SKU。

- 旧 `product_spu`、`product_sku` 身份映射继续保留，用于存量商城商品关联和对账，不反向成为 ERP 商品身份。

---

## 11. 导入与同步验收

### 11.1 上线前数据剖析

五张来源表必须输出并由责任部门确认：

- 总记录数、有效记录数、软删除记录数；
- 主键、业务键空值和重复数；
- 每个状态和布尔字段的取值分布；
- 金额、数量、税率的最小值、最大值、格式异常数和负数分布；
- 关联孤儿数量；
- JSON 和字符串集合解析失败数量；
- 来源更新时间为空、未来时间和同秒集中度；
- 敏感字段和附件字段清单；
- 所有不能从 DDL 证明的字段语义及其数据字典证据。

### 11.2 分表验收

| 对象 | 验收条件 |
| --- | --- |
| 卡券销售单 | 全部导入对象具有唯一来源销售单号；正式状态可证明；核心字段映射完成；金额公式通过；一单恰好一条卡券明细；P0 差异为零 |
| 客户 | 全部在用客户具有明确 ERP 身份或明确拒绝原因；跨客户/供应商主体候选经销售、采购和财务确认，未按税号自动合并；敏感字段受控 |
| 供应商 | 全部在用供应商具有明确 ERP 身份、至少一项经采购确认的能力及必要有效资质；余额累计未误入资金台账 |
| SPU | 全部被历史订单引用的对象均有可追溯身份；新启用商品另需显式确认独立 `product_kind`、通过分类兼容校验并拥有至少一个可用 SKU；旧商城商品性质只作候选，商城发布信息与 ERP 核心信息已拆分 |
| SKU | 历史引用无孤儿；新启用 SKU 的规格 JSON 全部可解析、规范规格签名不重复且基础单位齐全；供应关系冲突只阻断新供给，不删除历史身份 |
| 期初库存 | 只使用统一基准日仓库实盘，按 SKU 和仓库导入；不使用 SPU/SKU 来源库存字段替代 |

### 11.3 运行验收

- 同一来源快照重复处理不产生重复 ERP 对象或版本。
- 增量同步在 ERP 停机恢复后能从原游标补齐。
- 基线初始水位取拉取开始时间，重叠窗口和稳定分页在停机、超时与重复页场景不漏单。
- 同秒多条更新不丢失。
- 迟到快照直接丢弃；同一来源时间不同内容进入差异；A → B → A 保留第三次观察和业务版本。
- 每日全量清单和 ERP 当前商业指纹逐单一致。
- 单据差异可按来源单号重拉和重放。
- 映射失败只生成差异任务，不生成错误应收、库存和利润。
- 第二期重复发送同一 ERP 版本时，商城只应用一次。
- 发送超时、未知结果、业务拒绝和确认丢失均能重试、查询或转人工处理。
- 切换时点 T 记录一次（`mall_consumption_cutover.enabled_at`）；T 起商城停止创建
  B2B 销售单，全部 B2B 销售单统一由 ERP 服务；无批次、冻结或基线确认流程。
- 卡实例基线不要求商城传 ERP 版本、明细或类目；所有会被事实引用的有效、耗尽和到期实例均可追溯到原销售单。
- 五类商城事实均通过共同信封和专有字段契约校验；实时与回填按相同业务事实键去重，
  `[range_start, T)` 回填与实时重叠时不重复形成正式事实。
- 支付来源只接受 `CARD` 和 `WECHAT`；订单优惠、运费先由商城分配到明细，订单、明细、
  支付来源和来源 × 明细分摊总额逐层守恒。
- 实时和回填的商城成本快照均校验含税标识及进项税率；不得用销项税率替代。
- 商城退款、余额恢复和供应商退款分别幂等；供应商退款分配合计等于退款事实，
  反向成本和应付与退款事实同事务提交。
- 原始文件重复审计只使用 `source_file_hmac + hmac_key_version`；密钥轮换后能受控验证
  历史指纹，清洗包完整性值与原始文件 HMAC 相互独立。

### 11.4 迁移门禁与结果义务

生产导入必须同时满足以下门禁，否则禁止进入生产：

1. 字段字典、状态字典、单位规则和安全白名单必须已冻结；
2. 验证环境必须已完成来源快照导入与自动校验；
3. 销售必须已确认客户和卡券销售单映射；
4. 采购必须已确认供应商、能力、资质、商品和供应关系；
5. 运营必须已确认卡券类目、卡形态和商城发布关系；
6. 财务必须已确认税率、卡券期初票款处理和银行信息，并确认旧供应商累计余额不生成 ERP 期初事实；
7. 仓储必须已按统一基准日实盘确认 SKU 期初库存；
8. 必须输出成功、拒绝、差异和未覆盖清单；
9. 必须经业务负责人签字确认后方可执行生产导入；
10. 成功形成正式数据的白名单导入包、manifest、规则版本、成功结果和映射审计必须长期
    保留；失败合规包及行列明细必须保留 30 天，失败批次元数据、汇总计数、脱敏错误码和
    审计必须长期保留。原始 SQL 仅允许在隔离临时区使用；完成安全清洗后，仅当审计需要
    关联同一原始文件时保存 `source_file_hmac + hmac_key_version`，其后必须按批准期限销毁。

一旦导入结果已经形成正式销售单版本、应收、付款、库存或其他下游事实，禁止通过覆盖或删除回滚；必须使用业务变更或纠正事实处理。

---

## 12. 安全白名单

### 12.1 一般规则

- 所有同步接口使用字段允许列表，不使用 `SELECT *` 作为长期接口契约。
- 接口载荷、来源快照、导入文件、错误信息和应用日志执行同一敏感字段策略。
- 原始 SQL、商城原始导出和含禁止字段的文件不得进入长期对象存储；长期文件必须是
  重新生成的白名单导入包，并用带密钥的完整性校验值防篡改，不能以低熵敏感字段
  的普通摘要代替加密或脱敏。
- 若审计需要关联同一原始文件，只保存 `source_file_hmac + hmac_key_version`，不保存
  裸 SHA/MD5 等普通摘要。密钥轮换由密钥管理系统执行，历史密钥版本仅供受控校验；
  清洗包使用自己的 `file_asset.content_hmac + hmac_key_version`，不能复用原始文件
  HMAC。
- 手机、邮箱、税号、银行账户和证照附件按角色授权访问。
- 银行账户加密存储，列表默认遮罩，查看完整值记录访问审计。
- 附件进入受控对象存储，业务表只保存附件身份、类型、摘要和权限，不把长期可访问 URL 当正式数据。
- 数据导出设置有效期并记录导出人、用途和范围。
- 日志不记录完整联系方式、银行账户、证件内容、绑定验证信息、Token、签名密钥和来源数据库连接信息。

### 12.2 `pay_card_sell`

商业快照允许接收：

- 销售单号、正式业务状态、来源更新时间；
- 客户和结算主体候选身份及必要客户快照；
- 来源销售主管、项目名称和业务备注；
- 卡券类目候选、卡形态候选；
- 面额候选、数量、成交单价、成交金额；
- 履约期限；
- 税率和开票要求；
- 证明存量单正式性的审批引用和必要附件元数据。

明确禁止接收：

- `bind_verify_prefix`
- `bind_verify_suffix`
- `bind_verify_answer`
- 卡号、卡密、绑定手机号和卡实例秘密
- 数据库连接信息和商城接口密钥

补充约束：

- `pay_url` 不接收；`audit_url` 原始值和签名参数不持久化。只允许保存已安全归档、
  重新授权的附件身份，或保存 `process_instance_id` 等流程引用，不伪造成 ERP 审批记录；
- 商城玩法字段一律不接收，技术排查仍回到商城侧完成，不在 ERP 建立玩法诊断副本。

### 12.3 客户与供应商

- 联系方式只接收业务所需字段。
- 银行账户不进入普通来源快照；通过专用加密导入通道写入银行账户版本。
- 证照字段只接收结构化元数据和受控附件引用，不在日志打印原始 JSON 或文件内容。
- 旧余额、授信和累计金额只能进入财务专用核对区，不能在普通基础资料页面批量展示。

### 12.4 商品与 SKU

- 商品名称、来源商品类型候选、规格、分类、图片引用、说明、供应商商品 ID、价格和可供信息可以进入商品暂存；来源类型不得直接写入公司 `product_kind`。
- 图片 URL 必须校验协议、域名策略和内容类型，不由后端任意访问未知内网地址。
- 富文本说明在展示前净化。
- `properties` JSON 限制结构、深度、字段数量和字符串长度。
- 来源商品原始响应按供应商连接隔离，不包含连接密钥和签名原文。

---

## 13. 必须补齐的数据字典与样本

以下语义不能由当前 DDL 单独证明。生产导入前必须取得代码字典、真实样本和业务负责人确认：

| 来源表 | 必须确认的字段 |
| --- | --- |
| `pay_card_sell` | `sell_status`、`status`、`card_type_id`、`category_id`、`card_type`、`sell_card_type`、`order_type`、`card_location`、`sign_status`、`sell_price`、`total_sell_money`、`original_price`、`card_value`、卡形态来源、合同来源、结算主体来源、`tax_point`、`invoice_type`、履约期限优先规则 |
| `erp_customer` | `status`、`tax_percent` 单位及适用范围；`customer_types` 只确认解析格式并留白名单暂存，不形成正式分类 |
| `erp_supplier` | `type`、`status`、`submit_status`、`recycle_status`、结算周期代码、反向布尔值、累计金额单位、证照 JSON 结构 |
| `product_spu` | `status`、`delisting_type`、`category`、`supplier_type`、价格单位、库存口径、字符串 ID 集合格式 |
| `product_sku` | `status`、`properties` 结构、`supplier_type`、价格单位、`stock/total_stock` 口径、库存释放规则、`min_nums` 是供应商起订量还是商城购买限制、`stock_warning` 的仓库适用范围、重量体积单位 |

五张表之外还必须补齐以下依赖数据集；缺失时只允许暂存来源行，不能猜测外键：

| 依赖数据集 | 用途 |
| --- | --- |
| 商品分类、品牌及旧税收分类字典 | 解析 `category_id`、`brand_id`、`category_tax_rate_id` |
| SKU 属性、属性值及分类适用关系 | 解析 `properties` 并生成稳定规格签名 |
| 基础计量单位及每个 SKU 的单位来源 | 建立 `unit_of_measure` 与 `sku.base_unit_id` |
| 供应商连接、渠道和供应商商品身份 | 解释 `supplier_type`、`third_channel_id`、`supplier_spu_id`、`supplier_sku_id` |
| 客户合同、结算主体及其有效版本 | 形成销售单正式版本和应收归属 |
| 员工与组织身份 | 区分销售主管、负责销售和来源操作人 |
| 商城正式状态历史或正式单清单 | 证明已生效、作废和关闭，不从当前行状态反推历史 |
| 卡券类目与电子卡/实体卡形态字典 | 区分卡券类目、储值/次卡分类和卡形态 |

确认材料应记录来源代码版本、样本范围、确认人和生效时间。映射规则升级后保留旧版本，以便重现历史导入结果。
