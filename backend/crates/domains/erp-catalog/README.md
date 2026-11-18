# erp-catalog：商品目录与规格

## 职责

`erp-catalog` 是公司商品主档的统一来源，回答「卖的是什么」「有哪些可独立识别的 SKU」「当前展示资料和销售参考价是什么」。例如，「保温杯」是一个商品（SPU），500 mL 和 750 mL 是同一商品下的两个 SKU；销售选品、供应商供给和库存都引用这些公司身份，不另建一套商品主档。

- 维护商品和 SKU 的稳定身份、不可变修订、启停状态及上下架；维护品牌、分类、计量单位、规格属性及属性值、卡券分类资料。
- 维护公司 SKU 的一件代发、集采和市场含税参考价，提供公司商品池的查询合同；参考价与供应商采购报价相互独立。
- 管理供应商门户新品提报的商品草稿和建档步骤，以及 API 选品后公司展示资料的唯一同步来源、字段人工维护责任、来源基线和差异确认回执。

## 目录结构

公开入口见 [src/lib.rs](src/lib.rs)，依赖见 [Cargo.toml](Cargo.toml)。术语与统一分工见[总索引](../../README.md#代码中常见名称的含义)。

| 入口 | 用途 |
| --- | --- |
| [src/entity/](src/entity/mod.rs) | 商品、SKU、修订、字典等实体及规格签名等确定性规则；商品模型入口为 [entity/catalog](src/entity/catalog/mod.rs)。 |
| [src/service/catalog/](src/service/catalog/mod.rs) | `CatalogService`：建档、规格编辑、上下架、字典和卡券等本域用例；[sellable.rs](src/service/catalog/sellable.rs) 定义公司商品池筛选和视图转换。 |
| [src/repository/](src/repository/mod.rs) | `CatalogExt` 集合访问器、MongoDB 仓储、筛选、分页及跨集合写入步骤。 |
| [src/indexes/](src/indexes/mod.rs) | 公开索引注册入口 `indexes::ensure`，包含目录、条码占用、交接回执、门户和来源同步索引。 |
| [src/authorization.rs](src/authorization.rs)、[access.rs](src/service/catalog/access.rs) | 商品对象事实、固定 Mapping、用例授权入口及编译查询凭证。 |
| [src/ports/](src/ports/mod.rs) | 商品消费的文件资产事实、待登记附件、审计及跨域供给查询合同。 |
| [src/dto/](src/dto/mod.rs) | 创建、编辑、查询、导入等请求与视图合同。 |
| [src/source_sync/](src/source_sync/mod.rs)、[service/catalog/source_sync.rs](src/service/catalog/source_sync.rs) | 前者保存来源基线、字段责任和差异决定规则；后者在事务中核验权限及版本、写入实际商品修订并保存确认回执。 |
| [src/portal/](src/portal/mod.rs) | 门户新品提报、分类映射和审核通过后的商品/SKU 建档步骤；正式写入加入调用方事务。 |

## 架构设计

### 建档与修订：改名称不换 SKU，改规格组合要区分身份

[Product](src/entity/catalog/product.rs) 保存商品编号、业务类型、维护归属和当前修订指针；商品名称、介绍、分类、品牌见 [ProductRevision](src/entity/catalog/product_revision.rs)，轮播图和详情图归商品修订。[Sku](src/entity/catalog/sku.rs) 保存 SKU 编号、所属商品、基础单位、规格签名和当前修订指针；销售名称、主图、条码和公司销售参考价见 [SkuRevision](src/entity/catalog/sku_revision.rs)。展示资料随修订追加，不覆盖旧快照。

500 mL SKU 的 `容量=500 mL` 由[规格签名值对象](src/entity/catalog/specification.rs)用 SPU 内的规格名和值规范化生成，不依赖全局字典编码；规格项的展示顺序不改变身份。固定空签名保证同一商品最多一个无规格 SKU。用[完整规格编辑](src/service/catalog/product_update.rs)给 500 mL SKU 改销售名称时，签名不变，保留 `sku_id` 并追加修订；首次加入 750 mL 则创建新的 SKU。移出规格只停用 SKU、保留历史；重新加入已停用规格必须明确要求重新启用、填写原因并复用原身份。

[建档](src/service/catalog/product_create.rs)和编辑用例先校验输入与临时附件引用，再在写入快照中核实字典、权限及适用的版本，写入稳定记录、修订、媒体和审计。仓储通过 `CatalogExt` 读取集合，用调用方的 `persistence_core::Executor` [写入商品修订与媒体](src/repository/catalog/product.rs)、[写入 SKU 与修订](src/repository/catalog/sku_lookup.rs)并[核对条码归属](src/repository/catalog/barcode_claim.rs)，不自行开启事务。[目录索引](src/indexes/catalog.rs)保证商品编号、SKU 编号及 `(product_id, specification_signature)` 唯一，停用不释放规格身份。单域原子写入由 Service 开启事务；跨域写入由 Process 持有事务并调用本域写入步骤。图片字节在事务外上传，资产登记与审计由 Port 注入并使用原执行器。

### 事实与权限：归谁维护，与是否有权编辑是两件事

打开保温杯档案或编辑 500 mL SKU 时，商品的 `maintainer_user_id`、`business_org_unit_id` 和记录版本是可读取的对象事实，创建人不是维护人的替代值。建档以即将保存的真实商品事实做候选授权；修改已有 SKU 时，对其所属商品用固定动作做对象授权，并核对相应版本，不能以查到商品或通过按钮权限检查代替。事实和固定 Mapping 由[应用授权装配](../../../apps/erp-server/src/wiring/authorization/master.rs)接入；权限定义归 `erp-permissions`，[`erp-identity::AuthorizationService`](../erp-identity/README.md#业务授权-api)解释策略。本域不解释角色条款、不组合权限范围，也不构造授权结果。

[公司商品列表及详情](../../read-models/erp-read-models/src/catalog_center/scope.rs)使用本次读取阶段的编译凭证，在分页和计数前应用授权条件；用户筛选只能缩小结果。[详情下的 SKU 与修订](src/service/catalog/product_detail.rs)还须核对对子资源所属商品的访问权限。读取前后检查凭证与当前阶段是否匹配，交付前由身份服务复验：同一快照内核对执行边界和有效期，非事务读取还须重读身份、权限范围及动态事实依赖；凭证不得跨请求、事务或阶段复用。公司商品池是另一个读取入口，[校验固定操作权限](../../read-models/erp-read-models/src/catalog_center/mod.rs)，不替代档案对象授权。

### 上架、可售与供给：上架不等于能卖

[上下架用例](src/service/catalog/listing.rs)在 SKU 上保存上架状态，SPU 只按启用 SKU 汇总上架结果；例如，仅上架 500 mL 时，两个 SKU 均启用的保温杯显示部分上架。新 SKU 默认未上架；旧数据缺少上架字段时按已上架读取，但仍须满足可售条件。即使 500 mL 已上架，在资格业务日也必须满足商品与 SKU 均启用、当前修订启用且已生效、已维护公司一件代发参考价，并有至少一条启用、条款有效且当前可供的供给，才能进入[公司商品池](../../read-models/erp-read-models/src/catalog_supply/repository/sellable.rs)。商品池是只读投影，不产生独立商品身份，也不暴露供应商采购成本和税率。

[erp-supply](../erp-supply/README.md) 维护「某供应商供应 500 mL SKU」的供给关系、采购价格、税率和可供状态，以及供应商侧履约和结算；[erp-inventory](../erp-inventory/README.md) 维护仓内数量与预占。商品域通过 [CatalogSupplyQueryPort](src/ports/supply.rs)声明商品列表、供给覆盖、可售分页与精确资格复验的合同，由[应用装配](../../../apps/erp-server/src/wiring/catalog_supply_query/mod.rs)接到跨域 ReadModel。销售提交按所选 `sku_id` 和 `sku_revision_id` [实时复验](../../orchestration/erp-order-to-cash/src/order_to_cash/adapters/catalog.rs)；之前出现在列表里，不代表下架、断供或换修订后还能沿用原选择。

### API 选品与来源同步：同一个商品可以多家供货，展示资料只有一个来源

同步供应商品库不等于新建公司商品。[选品 Process](../../orchestration/erp-supplier-catalog/src/supplier_catalog_adoption/confirm.rs)在采购明确确认后创建或关联公司商品/SKU，并在同一行事务中登记供给和选品结果。符合条件的新建商品可建立[唯一资料来源](../../orchestration/erp-supplier-catalog/src/product_source_sync.rs)；同源商品可以绑定多个 SKU，其他供应商可以供货但不接管显示资料，旧商品不会因匹配或同步自动取得来源关系。

例如，供应商更换 500 mL 的主图时，系统按[字段责任规则](src/source_sync/rules.rs)比较公司当前值、供应商最新值和已处理基线。商品名称、介绍、轮播图和详情图按 SPU 跟踪；销售名称和主图按 SKU 跟踪。未人工维护且可采用的变化可写入新修订；涉及人工维护字段或图片尚未核验归档的差异留待确认，来源未提供的字段不视为清空。同步不改公司分类、规格、单位、销售价格或主动停用、下架状态。

确认差异可采用来源值、保留公司值或编辑后确认。[确认命令](src/source_sync/command.rs)先授权，再查幂等回执；首次执行核对商品、来源及涉及 SKU 的版本，将字段决定、必要的修订、审计和回执同事务写入。同一操作号重试同一请求返回原结果，载荷不同或首次提交的版本过期则拒绝。[来源索引](src/indexes/source_sync.rs)通过 `indexes::ensure` 增量登记；回滚保留来源基线、商品修订和确认回执。

原引用《供应商 API 商品与自动履约执行合同》已从当前版本移除，可在 Git 历史的 `f4de1dff6:docs/supplier-api-product-contract.md` 查阅。当前实现的入口见[供应目录与供给编排](../../orchestration/erp-supplier-catalog/README.md)及[供应商订单编排](../../orchestration/erp-supplier-orders/README.md)；业务背景见 [API 接入供应商供给与 ERP 商品发布](../../../../docs/erp-phase-2.md#7-api-接入供应商供给与-erp-商品发布)。

## 修改与验证

- 仅依赖 `erp-identity` 的公开身份与授权接口；其余常规、构建和测试依赖不得指向其他业务领域或组合层。跨域能力由 Process 或应用装配 Port；跨域读取由 ReadModel 组装。
- 无 I/O 的校验、迁移及不变式放实体或值对象。新增或调整仓储查询时同步评估索引；应用复用公开索引入口，保持逐集合注册顺序。HTTP 优先复用本域 DTO；扩展公开合同时检查 Process、ReadModel 和应用调用方。
- 业务代码变更须补充本域库单元测试，覆盖成功、失败、边界及相关幂等或版本冲突路径。在 `backend/` 执行 `env -u ERP_TEST_MONGO_URI cargo test -p erp-catalog --lib --locked` 并遵循[统一质量门禁](../../README.md#质量门禁)；不执行集成测试或真实外部服务测试。仅修改文档时只执行 `git diff --check`。
