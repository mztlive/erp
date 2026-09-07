# 阶段 16 Catalog / Supply 跨域查询语义核销合同

## 1. 判定与绑定

当前所核源码未发现业务语义漂移。验收依据为原生产函数、真实 Mongo 执行器、实际 Port 实现、实际销售消费链、HTTP 入口和错误转换的逐段核销；不得将本报告解释为已执行 Rust 测试或真实数据库验证。

- 输入提交：`ed8015e28d36e5b9323b87e1a1fd5f63c122eb70`。
- 审核工作树：`/private/tmp/erp-domain-crate-16-supply`。
- 原件：`/private/tmp/supply16-catalog-before.json` 的 8 个文件内容及 SHA-256 全部逐字节匹配输入提交对应 Git blob。
- 源码终态提交：`72a0c79a2261d33699b869329376e534edfb1ef4`；34 个已审源文件 SHA-256 全部匹配该提交实际 Git blob，工作树对应文件也与 blob 相同，已封存。
- 逐符号、完整声明、文件 hash、错误链证据：`/private/tmp/supply16-catalog-semantic-review.json`。
- 复核脚本：`/private/tmp/review-supply16-catalog.py` 与 `/private/tmp/complete-supply16-catalog-review.py`。脚本只读取工作树并输出 `/private/tmp`；未执行迁移脚本。

| 核销项 | 结果 |
| --- | --- |
| 原 8 文件函数定义 | 77 个：64 个生产函数、12 个原测试、1 个测试 helper |
| 原生产函数正文 | 60 个 token 相同；2 个列表入口按原查询点拆分后前后片段相同；2 个影子状态方法改用 owning Supply enum 的原实际分支 |
| 迁往实际 provider 的 Mongo / pipeline 函数 | 21 个正文 token 全部相同 |
| 原测试 | 12 个全部保留；4 个迁往实际 provider，8 个留在 Catalog |
| 完整 row / filter / facet / DTO 声明 | 11 个全部相同，包含字段顺序、类型、derive、serde 与 validator 属性 |
| Port / RM / HTTP 调用 | 3 个 Port 实现、2 个 RM 入口、2 个 HTTP handler 已核对实际生产正文 |
| 错误转换 | Catalog→services、Catalog→HTTP、services→HTTP 的原实际 From 正文均相同 |

计数以 JSON 逐符号记录为准。字符比较保留字面量，只排除注释和空白；不将不明业务差异归为“命名变化”。

## 2. 真实符号归属与公开边界

| 原拥有位置 | 终态拥有位置 / 合同 |
| --- | --- |
| `erp_catalog::repository::catalog::product::{CatalogRepository::search_products, product_page, aggregate_products}` | `erp_processes::adapters::catalog_supply_query::repository::product::CatalogSupplyRepository`，三个正文均保留 |
| `erp_catalog::repository::catalog::product_pipeline::*` | Process `catalog_supply_query/repository/product_pipeline.rs`，10 个原生产 builder / helper 全部迁入 |
| `erp_catalog::repository::catalog::sellable` 的 8 个 Mongo / pipeline 函数 | Process `catalog_supply_query/repository/sellable.rs` |
| `ProductFilter` / `ProductRow` / `SellableSkuFilter` / `SellableSkuRow` | 留在 Catalog，窄重导出于 `erp_catalog::repository`；`SellableSkuFilter::as_of` 正文不变 |
| `ProductFacet` / `ProductTotal` / `SellableSkuFacet` / `SellableSkuTotal` | 随实际聚合移入 Process 私有 repository 叶 |
| `CatalogService::product_list` | Catalog `prepare_product_list` → RM `CatalogCenterReadService::product_list` → Catalog `product_page_view` |
| `CatalogService::sellable_sku_list` | Catalog `prepare_sellable_sku_list` → RM `CatalogCenterReadService::sellable_sku_list` → Catalog `sellable_sku_page_view` |
| 假 Supply 集合常量与状态副本 | 删除；实际 provider 引用 `SupplierOfferingExt` 和 Supply 实际枚举 |
| `listing::sku_is_listed_expr` | 保留唯一 Catalog 实现，仅改窄公开导出；原上架汇总和新商品列表同时消费 |

`MongoCatalogSupplyQuery` 为公开提供方；`repository` 与四个叶模块保持私有，`CatalogSupplyRepository` 为 `pub(super)`。三个 Port 方法只接收 Catalog 自有 filter / row、`BusinessDate`、调用方 `&mut dyn Executor`，返回 `persistence_core::Result`。Catalog 不再通过假 Port 引用 Supply 集合名或复制状态变体。

## 3. 实际查询与事务合同

### 3.1 精确引用

`find_sellable_sku_refs` 在 `refs.is_empty()` 时立即返回空结果，发生于构造 match、构造 filter、构造 pipeline 和访问 session 之前。非空引用保留输入顺序的 `$or`，每个分支必须同时匹配 `id` 与 `current_revision_id`；不得仅按 SKU ID 查询。

精确引用使用原 `SellableSkuFilter::as_of(date)`，原无业务筛选状态与分页占位值均保留。`paging=None` 保留按 `sku_no`、`id` 排序及 `$skip:0`，不追加 `$limit`。分页接口继续施加页码和页大小。

### 3.2 Executor 与聚合输出

商品和可售聚合均保留原分支：`executor.session()` 为 Some 时，aggregate 与 cursor stream 使用同一传入 session；为 None 时执行无 session 的对应聚合。Port、provider 和 repository 不新建事务、不替换 Executor、不增加重试或第二次查询。

两个 RM 列表入口仍由调用方明确传入 `NoTransaction`。销售资格入口仍将原销售流程传入的 Executor 经 Sales Port → `CatalogQualificationAdapter` → `MongoCatalogSupplyQuery` → repository 原样传到底层。

两条聚合保持只消费第一个 facet 文档；无 facet 时返回空 items / total，计数取第一个 total 行或 0。typed aggregation、cursor 收集和反序列化的错误传播位置均不变。

### 3.3 商品列表 pipeline

执行顺序固定为原主表 match → 当前商品修订 lookup / 保留空修订 unwind → 分类 / 品牌筛选 → 当前启用 SKU、当前 SKU 修订与供给 lookup → SKU 各计数 → 继承上架状态 → 关键字 / 上架 / 供应商 / 覆盖 / 价格筛选 → facet。

商品供给覆盖仍仅使用原“供给启用且 current_revision_id 非空”的谓词；本迁移没有增加可售池的日期或 availability 资格约束。价格上下界保持 Decimal128 与闭区间，并由同一个 SKU 的 `$elemMatch` 满足区间。分页前完成全部业务筛选；排序字段白名单、方向、skip / limit / project 次序与 total 所处阶段均不变。

### 3.4 可售 SKU pipeline

稳定 SKU 保持启用、`listing_status in [listed,null]`、当前修订非空。当前 SKU 修订保持启用、销售可见价非空和 `[effective_from,effective_to)`；稳定商品和当前商品修订保持原启用与日期条件。单位关联允许缺失。

供给依次约束稳定供给启用与当前指针、当前供给修订 `[valid_from,valid_to)`、availability 为 `AVAILABLE` 且 quantity 缺失 / null 或大于 Decimal128 零。至少一条合格供给之后，才按原 `$setUnion` 计算供应商去重与供给区域并集。

可选筛选顺序保持商品类型 → 分类 → 品牌 → 供应商 → 区域 → 最大供应商数量 → 价格 → 关键字。关键字继续 `regex::escape`，价格仍 Decimal128，统计仍处于全部资格和筛选之后。行投影不增加供应商身份、采购价格或税率。

## 4. 准备、日期与投影合同

商品列表保留 `params.validate()` → `params.normalized()` → filter。`normalized()` 原函数文件逐字节未变，内部先 `normalize_sort`，再销售价区间校验，再文本归一化与分页默认值。因此排序错误与金额错误的先后关系不变。

可售列表保留 `params.validate()` → 销售价区间校验 → page 默认 1 → page_size 默认 20 → 显式资格日期或惰性 `BusinessDate::today()` → 按原字段顺序归一化文本 → filter。金额区间规则仍先拒绝负端点，再拒绝下限高于上限；不提前取日期，不移至 provider 重新取日期。

两条原列表函数均已按原实际查询语句切成前缀 / 后缀，与新 prepare / projection 完整正文逐 token 重构相等。仅有返回 filter 的 `Ok` 包装和保存既有 page / page_size / date 的局部别名变化。

商品行投影与顺序、分页总数及 page / page_size 相同。可售行保留原逐行遍历和 Catalog 规格签名解析器；历史非法签名继续返回空属性，不使整页失败。响应 `eligibility_as_of` 等于实际查询日期。

销售 `ensure_sellable_refs` 及 Sales Port 源文件逐字节未变：空引用先返回；非空引用在原 `qualified_refs(refs, BusinessDate::today(), executor)` 参数求值位置取今天；实际返回 `(sku_id,sku_revision_id)` 映射和下游集合差异、SKU 排序及 fail-closed 文案不变。

## 5. wire / 字段 / 错误出口

| 实际绑定 | 原与新值 |
| --- | --- |
| `SupplierOfferingExt::SUPPLIER_OFFERINGS` | `supplier_offerings` |
| `SupplierOfferingExt::SUPPLIER_OFFERING_REVISIONS` | `supplier_offering_revisions` |
| `SupplierOfferingExt::SUPPLIER_OFFERING_AVAILABILITIES` | `supplier_offering_availabilities` |
| `OfferingStatus::Active.as_str()` | `ACTIVE` |
| `AvailabilityStatus::Available.as_str()` | `AVAILABLE` |

上述值已展开 owning domain 实际 trait 常量和枚举匹配分支核对，不仅检查测试字符串。Catalog 自有集合常量继续由 `CatalogExt` 提供。

完整声明核对覆盖 `ProductRow` 15 字段、`ProductFilter` 15 字段、`SellableSkuFilter` 12 字段、`SellableSkuRow` 22 字段、四个 facet / total 类型、`SellableSkuListParams` 12 字段、`SellableSkuView` 23 字段与规格属性 view 2 字段。`SellableSkuRow::supply_regions` 的 `#[serde(default)]` 保留。商品请求 / 响应 DTO 与共用价格校验文件均逐字节匹配输入。

页面原错误链为 Mongo → persistence → Catalog Error → services Error → HTTP Error。新链通过 RM 显式 `.map_err(erp_catalog::Error::from)` 保持同一 Catalog 转换，再进入当前 RM 错误别名 `services::Error` 和同一 HTTP From。验证错误、业务错误和 core 错误按原 Catalog From 进入 HTTP；没有直接将 persistence 错误交给 services 的另一个转换分支。

销售资格仍直接由 persistence → Sales Error，不经过 Catalog 或 RM 错误。DuplicateKey 的通用冲突提示、乐观锁刷新提示、TransientTransaction / OutcomeUnknown 的 typed source，以及其他 RepositoryError source 均保持。相关错误源文件或原转换生产正文已核对相同。

## 6. 原测试与实际接线

必须保留的 4 个迁移测试：

1. `product_list_pipeline_applies_keyword_and_sku_coverage_filters` → Process `repository/product_pipeline.rs`。
2. `sellable_sku_pipeline_is_fail_closed_and_cost_safe` → Process `repository/sellable.rs`。
3. `sellable_sku_reference_filter_matches_exact_pair` → Process `repository/sellable.rs`。
4. `supply_status_wire_values_stay_uppercase` → Process `repository/shared.rs`；前 3 个正文完全相同，第 4 个只把两个原集合常量改为真实 `SupplierOfferingExt` 常量，断言值及顺序相同。

其余 Catalog 原 8 测试保留：shared 排序 1 个、listing 汇总 4 个、sellable DTO / 规格投影 3 个，正文全部相同。原 listing 的 fixture helper 也保留。

HTTP `product_list` 与 `sellable_sku_list` 的请求参数、DTO、ApiResponse 包装和原函数正文，除 `state.catalog_service()` 改为 `state.catalog_center()` 外均相同。AppState 保存真实 `Arc<dyn CatalogSupplyQueryPort>`，用 `MongoCatalogSupplyQuery::new(db.clone())` 初始化；构造函数仅绑定 Database。商品页面复用该 Arc，销售通过 Arc<dyn CatalogSupplyQueryPort> 保存同一具体实现并使用原 Database，不要求两个消费者持有同一个对象实例。

`sku_is_listed_expr` 全仓只有一个定义；已有 Catalog 上架汇总和迁出的商品列表使用同一公开函数，未复制上架政策。最初生产 helper 位于测试模块之后的问题已在当前源码修正，不影响本报告语义判定。

## 7. 新增 7 个测试核销

新增测试源码均已完整阅读。5 个位于 `erp-read-models/src/catalog_center/tests.rs`，2 个位于 `erp-processes/src/order_to_cash/adapters/catalog/tests.rs`，全部调用实际生产入口，不另写替代业务序列。

| 实际测试 | 核销合同 |
| --- | --- |
| `invalid_product_page_stops_before_any_query` | 实际 product_list 的分页校验错误先于任何 Port 调用 |
| `product_projection_preserves_optional_fields_counts_and_page` | 实际 prepare 归一化商品编号，Port 验证分页与 NoTransaction，实际投影保留缺失字段、4 个计数、版本、时间和分页总数，单次调用 |
| `product_repository_failure_keeps_original_catalog_error_mapping` | 实际 RM 接收仓储乐观锁错误，经 Catalog 转换得到原 ConflictError 文案，单次调用 |
| `sellable_validation_precedes_price_validation_and_query` | 实际可售入口先拒绝非法页码，再在页码有效时按原文案拒绝倒置价格；两个失败均零查询 |
| `sellable_query_keeps_normalized_filters_explicit_date_and_default_page` | 实际归一化 q / 区域 / 空供应商，保持显式日期、默认分页与 total，Port 验证 NoTransaction |
| `exact_qualification_keeps_refs_date_and_nonzero_executor` | 实际 Sales adapter 传递引用顺序、日期和非零大小 Executor 的原对象指针，实际 row→pair 映射保序；若调用分页查询测试立即失败 |
| `exact_qualification_keeps_persistence_failure_without_retry` | 实际 Sales adapter 保留 Executor 指针和精确引用，仓储错误仍映射为原 Sales ConflictError 文案，恰好一次查询 |

Sales adapter 唯一生产增量为字段改成 `Arc<dyn CatalogSupplyQueryPort>`，默认 `new(Database)` 仍构造 `Arc::new(MongoCatalogSupplyQuery::new(db))`，无读库、取时、ID 或其它 I/O。`qualified_refs` 正文与先前审核一致。

这 7 个测试验证真实 RM / Sales adapter 经过记录 Port 的调用路径，不执行 Mongo 聚合或真实 ClientSession。新增可售成功用例返回空行并传显式日期，不声称它单独运行覆盖惰性 today 或非空可售投影；后两项仍由原生产前后片段相同与原投影 helper 测试保留证据支持。审核 hash 集合已从原 32 个文件扩展为 34 个，加入两个新增测试源。

## 8. 验证边界与终态交接

本审核未修改仓库源码、历史 `tests/**`、raw、allowlist 或 Cargo 配置，未执行 Cargo / Rust 测试 / MongoDB。环境变量 `ERP_TEST_MONGO_URI` 当前为 unset。root 新增测试的执行结果与全局门禁必须由 root 的真实日志提供，不得由本报告静态结论替代。

最终交接已完成源码封存：JSON 的 34 个源文件 hash 和 Git blob OID 已绑定 `72a0c79a2261d33699b869329376e534edfb1ef4`，12 个原测试与 7 个新增测试正文直接核对该提交 blob 一致。`source_commit_binding` 已为 sealed；原审核之后没有生产或测试正文变化。源码静态结论为未发现漂移；root 统一运行门禁仍由对应实际日志负责，本报告不追加未执行的运行验证声明。


## 9. raw money 三项显式核销

原始报告 `/private/tmp/erp-supply16-contract-sealed/missing-drift-report.json` 的 `/foundation_comparison/money/changed_symbols` 精确包含 `fn::as_of`、`struct::SellableSkuFilter`、`struct::SellableSkuRow`。raw 文件及其 `needs_review` 状态未修改；本节与 JSON `raw_foundation_money_disposition` 为对应逐项处置。

这三个实际定义仍在 `backend/crates/erp-catalog/src/repository/catalog/sellable.rs`。raw after money 集合选择了迁出的 Mongo provider 文件，未选择保留 filter / row 的 Catalog 叶；未进入该采集集合不等于生产符号删除。

| raw 名称 | 真实定义与处置 |
| --- | --- |
| `fn::as_of` | `erp_catalog::repository::catalog::sellable::SellableSkuFilter::as_of` 原正文相同。所有可选筛选（包括两个 Option<Amount> 价格端点）仍为 None；传入 BusinessDate 原样写入，page / page_size 仍为 1。无取时、ID、I/O 或金额转换 |
| `struct::SellableSkuFilter` | `erp_catalog::repository::catalog::sellable::SellableSkuFilter`，公开出口 `erp_catalog::repository::SellableSkuFilter`。12 个字段与 derive 完全相同；价格端点仍为 Option<erp_core::money::Amount>，资格日期仍为 BusinessDate，未增加默认金额、浮点转换或日期求值 |
| `struct::SellableSkuRow` | `erp_catalog::repository::catalog::sellable::SellableSkuRow`，公开出口 `erp_catalog::repository::SellableSkuRow`。22 字段、顺序和 Serialize / Deserialize / PartialEq / Eq 相同；sales_visible_price_gross 为 Amount、market_price 为 Option<Amount>、日期字段类型相同，supply_regions 的 serde(default) 保留。未增加自定义金额 serializer 或精度转换 |

实际金额筛选 builder 仍以 Decimal128 生成闭区间条件，供给数量零仍为 Decimal128，相关迁出生产函数已逐正文核对相同。页入口取业务日期和金额首错顺序按第 4 节；`as_of` 本身不取今天。以上三项绑定同一终态提交 `72a0c79a2261d33699b869329376e534edfb1ef4`，raw 文件 SHA-256、原 raw token hash、实际定义行号以及现有完整声明 / 函数证据均在 JSON 明确记录，可由 C 直接引用闭合。
