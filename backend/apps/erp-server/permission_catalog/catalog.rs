//! catalog 的接口权限目录。
use super::{PermissionDef, groups};

/// 查询商品收货地区配送提示。
pub const DELIVERY_COVERAGE_DELIVERY_COVERAGE: PermissionDef =
    PRODUCT_SELLABLE_SKU_LIST.described(groups::CATALOG, "查询商品收货地区配送提示");

/// 导入商品报价表。
pub const IMPORT_PRODUCT_IMPORT_SUBMIT: PermissionDef =
    PRODUCT_PRODUCT_CREATE.described(groups::CATALOG, "导入商品报价表");

/// 查询商品导入任务。
pub const IMPORT_PRODUCT_IMPORT_JOB_LIST: PermissionDef =
    PRODUCT_PRODUCT_LIST.described(groups::CATALOG, "查询商品导入任务");

/// 查询商品导入任务详情。
pub const IMPORT_PRODUCT_IMPORT_JOB_DETAIL: PermissionDef =
    PRODUCT_PRODUCT_LIST.described(groups::CATALOG, "查询商品导入任务详情");

/// 查询商品导入逐项结果。
pub const IMPORT_PRODUCT_IMPORT_JOB_ITEMS: PermissionDef =
    PRODUCT_PRODUCT_LIST.described(groups::CATALOG, "查询商品导入逐项结果");

/// 初始化商品直传。
pub const IMPORT_PRODUCT_IMPORT_DIRECT_UPLOAD_INIT: PermissionDef =
    PRODUCT_PRODUCT_CREATE.described(groups::CATALOG, "初始化商品直传");

/// 签发商品直传分片地址。
pub const IMPORT_PRODUCT_IMPORT_DIRECT_UPLOAD_PART_URL: PermissionDef =
    PRODUCT_PRODUCT_CREATE.described(groups::CATALOG, "签发商品直传分片地址");

/// 合并商品直传并登记导入任务。
pub const IMPORT_PRODUCT_IMPORT_DIRECT_UPLOAD_COMPLETE: PermissionDef =
    PRODUCT_PRODUCT_CREATE.described(groups::CATALOG, "合并商品直传并登记导入任务");

/// 取消商品直传。
pub const IMPORT_PRODUCT_IMPORT_DIRECT_UPLOAD_ABORT: PermissionDef =
    PRODUCT_PRODUCT_CREATE.described(groups::CATALOG, "取消商品直传");

/// 查询商品分类列表。
pub const PRODUCT_CATEGORY_LIST: PermissionDef =
    PermissionDef {
        code: "product_category:list", group: groups::CATALOG, description: "查询商品分类列表"
    };

/// 创建商品分类。
pub const PRODUCT_CATEGORY_CREATE: PermissionDef =
    PermissionDef {
        code: "product_category:create", group: groups::CATALOG, description: "创建商品分类"
    };

/// 更新商品分类。
pub const PRODUCT_CATEGORY_UPDATE: PermissionDef =
    PermissionDef {
        code: "product_category:update", group: groups::CATALOG, description: "更新商品分类"
    };

/// 移动商品分类。
pub const PRODUCT_CATEGORY_MOVE: PermissionDef =
    PRODUCT_CATEGORY_UPDATE.described(groups::CATALOG, "移动商品分类");

/// 删除商品分类。
pub const PRODUCT_CATEGORY_DELETE: PermissionDef =
    PermissionDef {
        code: "product_category:delete", group: groups::CATALOG, description: "删除商品分类"
    };

/// 查询商品品牌列表。
pub const PRODUCT_BRAND_LIST: PermissionDef =
    PermissionDef {
        code: "product_brand:list", group: groups::CATALOG, description: "查询商品品牌列表"
    };

/// 创建商品品牌。
pub const PRODUCT_BRAND_CREATE: PermissionDef =
    PermissionDef { code: "product_brand:create", group: groups::CATALOG, description: "创建商品品牌" };

/// 批量导入品牌，复用品牌创建权限。
pub const PRODUCT_BRAND_IMPORT: PermissionDef =
    PRODUCT_BRAND_CREATE.described(groups::CATALOG, "提交品牌导入任务");

/// 下载品牌导入任务待处理行，仍需原提交人及当前创建权限。
pub const PRODUCT_BRAND_IMPORT_FAILURES: PermissionDef =
    PRODUCT_BRAND_CREATE.described(groups::CATALOG, "下载品牌导入待处理行");

/// 一次创建商品品牌及Logo。
pub const PRODUCT_BRAND_CREATE_WITH_ASSETS: PermissionDef =
    PRODUCT_BRAND_CREATE.described(groups::CATALOG, "一次创建商品品牌及Logo");

/// 更新商品品牌。
pub const PRODUCT_BRAND_UPDATE: PermissionDef =
    PermissionDef { code: "product_brand:update", group: groups::CATALOG, description: "更新商品品牌" };

/// 一次更新商品品牌及Logo。
pub const PRODUCT_BRAND_UPDATE_WITH_ASSETS: PermissionDef =
    PRODUCT_BRAND_UPDATE.described(groups::CATALOG, "一次更新商品品牌及Logo");

/// 删除商品品牌。
pub const PRODUCT_BRAND_DELETE: PermissionDef =
    PermissionDef { code: "product_brand:delete", group: groups::CATALOG, description: "删除商品品牌" };

/// 查询计量单位列表。
pub const UNIT_OF_MEASURE_LIST: PermissionDef =
    PermissionDef {
        code: "unit_of_measure:list", group: groups::CATALOG, description: "查询计量单位列表"
    };

/// 创建计量单位。
pub const UNIT_OF_MEASURE_CREATE: PermissionDef =
    PermissionDef {
        code: "unit_of_measure:create", group: groups::CATALOG, description: "创建计量单位"
    };

/// 更新计量单位。
pub const UNIT_OF_MEASURE_UPDATE: PermissionDef =
    PermissionDef {
        code: "unit_of_measure:update", group: groups::CATALOG, description: "更新计量单位"
    };

/// 删除计量单位。
pub const UNIT_OF_MEASURE_DELETE: PermissionDef =
    PermissionDef {
        code: "unit_of_measure:delete", group: groups::CATALOG, description: "删除计量单位"
    };

/// 查询规格属性列表。
pub const SKU_ATTRIBUTE_LIST: PermissionDef =
    PermissionDef {
        code: "sku_attribute:list", group: groups::CATALOG, description: "查询规格属性列表"
    };

/// 创建规格属性。
pub const SKU_ATTRIBUTE_CREATE: PermissionDef =
    PermissionDef { code: "sku_attribute:create", group: groups::CATALOG, description: "创建规格属性" };

/// 更新规格属性。
pub const SKU_ATTRIBUTE_UPDATE: PermissionDef =
    PermissionDef { code: "sku_attribute:update", group: groups::CATALOG, description: "更新规格属性" };

/// 删除规格属性。
pub const SKU_ATTRIBUTE_DELETE: PermissionDef =
    PermissionDef { code: "sku_attribute:delete", group: groups::CATALOG, description: "删除规格属性" };

/// 查询规格属性值列表。
pub const SKU_ATTRIBUTE_VALUE_LIST: PermissionDef = PermissionDef {
    code: "sku_attribute_value:list",
    group: groups::CATALOG,
    description: "查询规格属性值列表",
};

/// 创建规格属性值。
pub const SKU_ATTRIBUTE_VALUE_CREATE: PermissionDef = PermissionDef {
    code: "sku_attribute_value:create",
    group: groups::CATALOG,
    description: "创建规格属性值",
};

/// 更新规格属性值。
pub const SKU_ATTRIBUTE_VALUE_UPDATE: PermissionDef = PermissionDef {
    code: "sku_attribute_value:update",
    group: groups::CATALOG,
    description: "更新规格属性值",
};

/// 删除规格属性值。
pub const SKU_ATTRIBUTE_VALUE_DELETE: PermissionDef = PermissionDef {
    code: "sku_attribute_value:delete",
    group: groups::CATALOG,
    description: "删除规格属性值",
};

/// 查询公司商品池。
pub const PRODUCT_SELLABLE_SKU_LIST: PermissionDef =
    PermissionDef { code: "sellable_sku:list", group: groups::CATALOG, description: "查询公司商品池" };

/// 查询商品列表。
pub const PRODUCT_PRODUCT_LIST: PermissionDef =
    PermissionDef { code: "product:list", group: groups::CATALOG, description: "查询商品列表" };

/// 创建商品。
pub const PRODUCT_PRODUCT_CREATE: PermissionDef =
    PermissionDef { code: "product:create", group: groups::CATALOG, description: "创建商品" };

/// 一次创建商品及媒体。
pub const PRODUCT_PRODUCT_CREATE_WITH_ASSETS: PermissionDef =
    PRODUCT_PRODUCT_CREATE.described(groups::CATALOG, "一次创建商品及媒体");

/// 规格编辑商品。
pub const PRODUCT_PRODUCT_UPDATE: PermissionDef =
    PermissionDef { code: "product:update", group: groups::CATALOG, description: "规格编辑商品" };

/// 一次修订商品及媒体。
pub const PRODUCT_PRODUCT_UPDATE_WITH_ASSETS: PermissionDef =
    PRODUCT_PRODUCT_UPDATE.described(groups::CATALOG, "一次修订商品及媒体");

/// 停用商品。
pub const PRODUCT_PRODUCT_DISABLE: PermissionDef =
    PRODUCT_PRODUCT_UPDATE.described(groups::CATALOG, "停用商品");

/// 整组上下架商品SKU。
pub const PRODUCT_PRODUCT_LISTING_UPDATE: PermissionDef =
    PRODUCT_PRODUCT_UPDATE.described(groups::CATALOG, "整组上下架商品SKU");

/// 查询商品修订列表。
pub const PRODUCT_PRODUCT_REVISION_LIST: PermissionDef =
    PermissionDef {
        code: "product_revision:list", group: groups::CATALOG, description: "查询商品修订列表"
    };

/// 查询SKU列表。
pub const PRODUCT_SKU_LIST: PermissionDef =
    PermissionDef { code: "sku:list", group: groups::CATALOG, description: "查询SKU列表" };

/// 上下架单个SKU。
pub const PRODUCT_SKU_LISTING_UPDATE: PermissionDef =
    PRODUCT_PRODUCT_UPDATE.described(groups::CATALOG, "上下架单个SKU");

/// 查询SKU修订列表。
pub const PRODUCT_SKU_REVISION_LIST: PermissionDef =
    PermissionDef { code: "sku_revision:list", group: groups::CATALOG, description: "查询SKU修订列表" };

/// 查询卡券类目列表。
pub const PRODUCT_VOUCHER_CATEGORY_PROFILE_LIST: PermissionDef = PermissionDef {
    code: "voucher_category_profile:list",
    group: groups::CATALOG,
    description: "查询卡券类目列表",
};

/// 创建卡券类目。
pub const PRODUCT_VOUCHER_CATEGORY_CREATE: PermissionDef = PermissionDef {
    code: "voucher_category_profile:create",
    group: groups::CATALOG,
    description: "创建卡券类目",
};

/// 更新卡券类目。
pub const PRODUCT_VOUCHER_CATEGORY_UPDATE: PermissionDef = PermissionDef {
    code: "voucher_category_profile:update",
    group: groups::CATALOG,
    description: "更新卡券类目",
};

/// 查询商品详情。
pub const PRODUCT_PRODUCT_DETAIL: PermissionDef =
    PermissionDef { code: "product:detail", group: groups::CATALOG, description: "查询商品详情" };

/// 交接商品维护人。
pub const PRODUCT_PRODUCT_HANDOVER: PermissionDef =
    PRODUCT_PRODUCT_UPDATE.described(groups::CATALOG, "交接商品维护人");

/// 查询商品交接候选。
pub const PRODUCT_PRODUCT_HANDOVER_CANDIDATES: PermissionDef =
    PRODUCT_PRODUCT_UPDATE.described(groups::CATALOG, "查询商品交接候选");

/// 查询商品修订详情。
pub const PRODUCT_PRODUCT_DETAIL_REVISIONS: PermissionDef =
    PRODUCT_PRODUCT_DETAIL.described(groups::CATALOG, "查询商品修订详情");

/// 查询商品SKU详情。
pub const PRODUCT_PRODUCT_DETAIL_SKUS: PermissionDef =
    PRODUCT_PRODUCT_DETAIL.described(groups::CATALOG, "查询商品SKU详情");

/// 查询商品SKU修订详情。
pub const PRODUCT_PRODUCT_DETAIL_SKU_REVISIONS: PermissionDef =
    PRODUCT_PRODUCT_DETAIL.described(groups::CATALOG, "查询商品SKU修订详情");

/// 查看商品来源资料差异。
pub const SOURCE_DIFF_PRODUCT_SOURCE_DIFF: PermissionDef =
    PRODUCT_PRODUCT_DETAIL.described(groups::CATALOG, "查看商品来源资料差异");

/// 确认商品来源资料差异。
pub const SOURCE_DIFF_RESOLVE_PRODUCT_SOURCE_DIFF: PermissionDef =
    PRODUCT_PRODUCT_UPDATE.described(groups::CATALOG, "确认商品来源资料差异");

/// 本模块可供路由引用的显式权限定义。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    ("DELIVERY_COVERAGE_DELIVERY_COVERAGE", DELIVERY_COVERAGE_DELIVERY_COVERAGE),
    ("IMPORT_PRODUCT_IMPORT_SUBMIT", IMPORT_PRODUCT_IMPORT_SUBMIT),
    ("IMPORT_PRODUCT_IMPORT_JOB_LIST", IMPORT_PRODUCT_IMPORT_JOB_LIST),
    ("IMPORT_PRODUCT_IMPORT_JOB_DETAIL", IMPORT_PRODUCT_IMPORT_JOB_DETAIL),
    ("IMPORT_PRODUCT_IMPORT_JOB_ITEMS", IMPORT_PRODUCT_IMPORT_JOB_ITEMS),
    ("IMPORT_PRODUCT_IMPORT_DIRECT_UPLOAD_INIT", IMPORT_PRODUCT_IMPORT_DIRECT_UPLOAD_INIT),
    ("IMPORT_PRODUCT_IMPORT_DIRECT_UPLOAD_PART_URL", IMPORT_PRODUCT_IMPORT_DIRECT_UPLOAD_PART_URL),
    ("IMPORT_PRODUCT_IMPORT_DIRECT_UPLOAD_COMPLETE", IMPORT_PRODUCT_IMPORT_DIRECT_UPLOAD_COMPLETE),
    ("IMPORT_PRODUCT_IMPORT_DIRECT_UPLOAD_ABORT", IMPORT_PRODUCT_IMPORT_DIRECT_UPLOAD_ABORT),
    ("PRODUCT_CATEGORY_LIST", PRODUCT_CATEGORY_LIST),
    ("PRODUCT_CATEGORY_CREATE", PRODUCT_CATEGORY_CREATE),
    ("PRODUCT_CATEGORY_UPDATE", PRODUCT_CATEGORY_UPDATE),
    ("PRODUCT_CATEGORY_MOVE", PRODUCT_CATEGORY_MOVE),
    ("PRODUCT_CATEGORY_DELETE", PRODUCT_CATEGORY_DELETE),
    ("PRODUCT_BRAND_LIST", PRODUCT_BRAND_LIST),
    ("PRODUCT_BRAND_CREATE", PRODUCT_BRAND_CREATE),
    ("PRODUCT_BRAND_IMPORT", PRODUCT_BRAND_IMPORT),
    ("PRODUCT_BRAND_IMPORT_FAILURES", PRODUCT_BRAND_IMPORT_FAILURES),
    ("PRODUCT_BRAND_CREATE_WITH_ASSETS", PRODUCT_BRAND_CREATE_WITH_ASSETS),
    ("PRODUCT_BRAND_UPDATE", PRODUCT_BRAND_UPDATE),
    ("PRODUCT_BRAND_UPDATE_WITH_ASSETS", PRODUCT_BRAND_UPDATE_WITH_ASSETS),
    ("PRODUCT_BRAND_DELETE", PRODUCT_BRAND_DELETE),
    ("UNIT_OF_MEASURE_LIST", UNIT_OF_MEASURE_LIST),
    ("UNIT_OF_MEASURE_CREATE", UNIT_OF_MEASURE_CREATE),
    ("UNIT_OF_MEASURE_UPDATE", UNIT_OF_MEASURE_UPDATE),
    ("UNIT_OF_MEASURE_DELETE", UNIT_OF_MEASURE_DELETE),
    ("SKU_ATTRIBUTE_LIST", SKU_ATTRIBUTE_LIST),
    ("SKU_ATTRIBUTE_CREATE", SKU_ATTRIBUTE_CREATE),
    ("SKU_ATTRIBUTE_UPDATE", SKU_ATTRIBUTE_UPDATE),
    ("SKU_ATTRIBUTE_DELETE", SKU_ATTRIBUTE_DELETE),
    ("SKU_ATTRIBUTE_VALUE_LIST", SKU_ATTRIBUTE_VALUE_LIST),
    ("SKU_ATTRIBUTE_VALUE_CREATE", SKU_ATTRIBUTE_VALUE_CREATE),
    ("SKU_ATTRIBUTE_VALUE_UPDATE", SKU_ATTRIBUTE_VALUE_UPDATE),
    ("SKU_ATTRIBUTE_VALUE_DELETE", SKU_ATTRIBUTE_VALUE_DELETE),
    ("PRODUCT_SELLABLE_SKU_LIST", PRODUCT_SELLABLE_SKU_LIST),
    ("PRODUCT_PRODUCT_LIST", PRODUCT_PRODUCT_LIST),
    ("PRODUCT_PRODUCT_CREATE", PRODUCT_PRODUCT_CREATE),
    ("PRODUCT_PRODUCT_CREATE_WITH_ASSETS", PRODUCT_PRODUCT_CREATE_WITH_ASSETS),
    ("PRODUCT_PRODUCT_UPDATE", PRODUCT_PRODUCT_UPDATE),
    ("PRODUCT_PRODUCT_UPDATE_WITH_ASSETS", PRODUCT_PRODUCT_UPDATE_WITH_ASSETS),
    ("PRODUCT_PRODUCT_DISABLE", PRODUCT_PRODUCT_DISABLE),
    ("PRODUCT_PRODUCT_LISTING_UPDATE", PRODUCT_PRODUCT_LISTING_UPDATE),
    ("PRODUCT_PRODUCT_REVISION_LIST", PRODUCT_PRODUCT_REVISION_LIST),
    ("PRODUCT_SKU_LIST", PRODUCT_SKU_LIST),
    ("PRODUCT_SKU_LISTING_UPDATE", PRODUCT_SKU_LISTING_UPDATE),
    ("PRODUCT_SKU_REVISION_LIST", PRODUCT_SKU_REVISION_LIST),
    ("PRODUCT_VOUCHER_CATEGORY_PROFILE_LIST", PRODUCT_VOUCHER_CATEGORY_PROFILE_LIST),
    ("PRODUCT_VOUCHER_CATEGORY_CREATE", PRODUCT_VOUCHER_CATEGORY_CREATE),
    ("PRODUCT_VOUCHER_CATEGORY_UPDATE", PRODUCT_VOUCHER_CATEGORY_UPDATE),
    ("PRODUCT_PRODUCT_DETAIL", PRODUCT_PRODUCT_DETAIL),
    ("PRODUCT_PRODUCT_HANDOVER", PRODUCT_PRODUCT_HANDOVER),
    ("PRODUCT_PRODUCT_HANDOVER_CANDIDATES", PRODUCT_PRODUCT_HANDOVER_CANDIDATES),
    ("PRODUCT_PRODUCT_DETAIL_REVISIONS", PRODUCT_PRODUCT_DETAIL_REVISIONS),
    ("PRODUCT_PRODUCT_DETAIL_SKUS", PRODUCT_PRODUCT_DETAIL_SKUS),
    ("PRODUCT_PRODUCT_DETAIL_SKU_REVISIONS", PRODUCT_PRODUCT_DETAIL_SKU_REVISIONS),
    ("SOURCE_DIFF_PRODUCT_SOURCE_DIFF", SOURCE_DIFF_PRODUCT_SOURCE_DIFF),
    ("SOURCE_DIFF_RESOLVE_PRODUCT_SOURCE_DIFF", SOURCE_DIFF_RESOLVE_PRODUCT_SOURCE_DIFF),
];
