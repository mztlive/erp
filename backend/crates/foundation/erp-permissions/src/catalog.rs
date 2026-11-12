//! 商品目录的内置权限。
use crate::{Permission, PermissionDef, groups, resources};

/// 查询商品收货地区配送提示。
pub const DELIVERY_COVERAGE_DELIVERY_COVERAGE: Permission = PRODUCT_SELLABLE_SKU_LIST;

/// 导入商品报价表。
pub const IMPORT_PRODUCT_IMPORT_SUBMIT: Permission = PRODUCT_PRODUCT_CREATE;

/// 查询商品导入任务。
pub const IMPORT_PRODUCT_IMPORT_JOB_LIST: Permission = PRODUCT_PRODUCT_LIST;

/// 查询商品导入任务详情。
pub const IMPORT_PRODUCT_IMPORT_JOB_DETAIL: Permission = PRODUCT_PRODUCT_LIST;

/// 查询商品导入逐项结果。
pub const IMPORT_PRODUCT_IMPORT_JOB_ITEMS: Permission = PRODUCT_PRODUCT_LIST;

/// 初始化商品直传。
pub const IMPORT_PRODUCT_IMPORT_DIRECT_UPLOAD_INIT: Permission = PRODUCT_PRODUCT_CREATE;

/// 签发商品直传分片地址。
pub const IMPORT_PRODUCT_IMPORT_DIRECT_UPLOAD_PART_URL: Permission = PRODUCT_PRODUCT_CREATE;

/// 合并商品直传并登记导入任务。
pub const IMPORT_PRODUCT_IMPORT_DIRECT_UPLOAD_COMPLETE: Permission = PRODUCT_PRODUCT_CREATE;

/// 取消商品直传。
pub const IMPORT_PRODUCT_IMPORT_DIRECT_UPLOAD_ABORT: Permission = PRODUCT_PRODUCT_CREATE;

/// 查询商品分类列表。
pub const PRODUCT_CATEGORY_LIST: Permission = Permission::new(resources::PRODUCT_CATEGORY, "list");

/// 创建商品分类。
pub const PRODUCT_CATEGORY_CREATE: Permission = Permission::new(resources::PRODUCT_CATEGORY, "create");

/// 更新商品分类。
pub const PRODUCT_CATEGORY_UPDATE: Permission = Permission::new(resources::PRODUCT_CATEGORY, "update");

/// 移动商品分类。
pub const PRODUCT_CATEGORY_MOVE: Permission = PRODUCT_CATEGORY_UPDATE;

/// 删除商品分类。
pub const PRODUCT_CATEGORY_DELETE: Permission = Permission::new(resources::PRODUCT_CATEGORY, "delete");

/// 查询商品品牌列表。
pub const PRODUCT_BRAND_LIST: Permission = Permission::new(resources::PRODUCT_BRAND, "list");

/// 创建商品品牌。
pub const PRODUCT_BRAND_CREATE: Permission = Permission::new(resources::PRODUCT_BRAND, "create");

/// 批量导入品牌，复用品牌创建权限。
pub const PRODUCT_BRAND_IMPORT: Permission = PRODUCT_BRAND_CREATE;

/// 下载品牌导入任务待处理行，仍需原提交人及当前创建权限。
pub const PRODUCT_BRAND_IMPORT_FAILURES: Permission = PRODUCT_BRAND_CREATE;

/// 一次创建商品品牌及Logo。
pub const PRODUCT_BRAND_CREATE_WITH_ASSETS: Permission = PRODUCT_BRAND_CREATE;

/// 更新商品品牌。
pub const PRODUCT_BRAND_UPDATE: Permission = Permission::new(resources::PRODUCT_BRAND, "update");

/// 一次更新商品品牌及Logo。
pub const PRODUCT_BRAND_UPDATE_WITH_ASSETS: Permission = PRODUCT_BRAND_UPDATE;

/// 删除商品品牌。
pub const PRODUCT_BRAND_DELETE: Permission = Permission::new(resources::PRODUCT_BRAND, "delete");

/// 查询计量单位列表。
pub const UNIT_OF_MEASURE_LIST: Permission = Permission::new(resources::UNIT_OF_MEASURE, "list");

/// 创建计量单位。
pub const UNIT_OF_MEASURE_CREATE: Permission = Permission::new(resources::UNIT_OF_MEASURE, "create");

/// 更新计量单位。
pub const UNIT_OF_MEASURE_UPDATE: Permission = Permission::new(resources::UNIT_OF_MEASURE, "update");

/// 删除计量单位。
pub const UNIT_OF_MEASURE_DELETE: Permission = Permission::new(resources::UNIT_OF_MEASURE, "delete");

/// 查询规格属性列表。
pub const SKU_ATTRIBUTE_LIST: Permission = Permission::new(resources::SKU_ATTRIBUTE, "list");

/// 创建规格属性。
pub const SKU_ATTRIBUTE_CREATE: Permission = Permission::new(resources::SKU_ATTRIBUTE, "create");

/// 更新规格属性。
pub const SKU_ATTRIBUTE_UPDATE: Permission = Permission::new(resources::SKU_ATTRIBUTE, "update");

/// 删除规格属性。
pub const SKU_ATTRIBUTE_DELETE: Permission = Permission::new(resources::SKU_ATTRIBUTE, "delete");

/// 查询规格属性值列表。
pub const SKU_ATTRIBUTE_VALUE_LIST: Permission = Permission::new(resources::SKU_ATTRIBUTE_VALUE, "list");

/// 创建规格属性值。
pub const SKU_ATTRIBUTE_VALUE_CREATE: Permission = Permission::new(resources::SKU_ATTRIBUTE_VALUE, "create");

/// 更新规格属性值。
pub const SKU_ATTRIBUTE_VALUE_UPDATE: Permission = Permission::new(resources::SKU_ATTRIBUTE_VALUE, "update");

/// 删除规格属性值。
pub const SKU_ATTRIBUTE_VALUE_DELETE: Permission = Permission::new(resources::SKU_ATTRIBUTE_VALUE, "delete");

/// 查询公司商品池。
pub const PRODUCT_SELLABLE_SKU_LIST: Permission = Permission::new(resources::SELLABLE_SKU, "list");

/// 查询商品列表。
pub const PRODUCT_PRODUCT_LIST: Permission = Permission::new(resources::PRODUCT, "list");

/// 创建商品。
pub const PRODUCT_PRODUCT_CREATE: Permission = Permission::new(resources::PRODUCT, "create");

/// 一次创建商品及媒体。
pub const PRODUCT_PRODUCT_CREATE_WITH_ASSETS: Permission = PRODUCT_PRODUCT_CREATE;

/// 规格编辑商品。
pub const PRODUCT_PRODUCT_UPDATE: Permission = Permission::new(resources::PRODUCT, "update");

/// 一次修订商品及媒体。
pub const PRODUCT_PRODUCT_UPDATE_WITH_ASSETS: Permission = PRODUCT_PRODUCT_UPDATE;

/// 停用商品。
pub const PRODUCT_PRODUCT_DISABLE: Permission = PRODUCT_PRODUCT_UPDATE;

/// 整组上下架商品SKU。
pub const PRODUCT_PRODUCT_LISTING_UPDATE: Permission = PRODUCT_PRODUCT_UPDATE;

/// 查询商品修订列表。
pub const PRODUCT_PRODUCT_REVISION_LIST: Permission = Permission::new(resources::PRODUCT_REVISION, "list");

/// 查询SKU列表。
pub const PRODUCT_SKU_LIST: Permission = Permission::new(resources::SKU, "list");

/// 上下架单个SKU。
pub const PRODUCT_SKU_LISTING_UPDATE: Permission = PRODUCT_PRODUCT_UPDATE;

/// 查询SKU修订列表。
pub const PRODUCT_SKU_REVISION_LIST: Permission = Permission::new(resources::SKU_REVISION, "list");

/// 查询卡券类目列表。
pub const PRODUCT_VOUCHER_CATEGORY_PROFILE_LIST: Permission =
    Permission::new(resources::VOUCHER_CATEGORY_PROFILE, "list");

/// 创建卡券类目。
pub const PRODUCT_VOUCHER_CATEGORY_CREATE: Permission =
    Permission::new(resources::VOUCHER_CATEGORY_PROFILE, "create");

/// 更新卡券类目。
pub const PRODUCT_VOUCHER_CATEGORY_UPDATE: Permission =
    Permission::new(resources::VOUCHER_CATEGORY_PROFILE, "update");

/// 查询商品详情。
pub const PRODUCT_PRODUCT_DETAIL: Permission = Permission::new(resources::PRODUCT, "detail");

/// 交接商品维护人。
pub const PRODUCT_PRODUCT_HANDOVER: Permission = PRODUCT_PRODUCT_UPDATE;

/// 查询商品交接候选。
pub const PRODUCT_PRODUCT_HANDOVER_CANDIDATES: Permission = PRODUCT_PRODUCT_UPDATE;

/// 查询商品修订详情。
pub const PRODUCT_PRODUCT_DETAIL_REVISIONS: Permission = PRODUCT_PRODUCT_DETAIL;

/// 查询商品SKU详情。
pub const PRODUCT_PRODUCT_DETAIL_SKUS: Permission = PRODUCT_PRODUCT_DETAIL;

/// 查询商品SKU修订详情。
pub const PRODUCT_PRODUCT_DETAIL_SKU_REVISIONS: Permission = PRODUCT_PRODUCT_DETAIL;

/// 查看商品来源资料差异。
pub const SOURCE_DIFF_PRODUCT_SOURCE_DIFF: Permission = PRODUCT_PRODUCT_DETAIL;

/// 确认商品来源资料差异。
pub const SOURCE_DIFF_RESOLVE_PRODUCT_SOURCE_DIFF: Permission = PRODUCT_PRODUCT_UPDATE;

/// 商品目录权限及可复用说明。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    (
        "PRODUCT_CATEGORY_LIST",
        PermissionDef {
            permission: &PRODUCT_CATEGORY_LIST,
            group: groups::CATALOG,
            description: "查询商品分类列表",
        },
    ),
    (
        "PRODUCT_CATEGORY_CREATE",
        PermissionDef {
            permission: &PRODUCT_CATEGORY_CREATE,
            group: groups::CATALOG,
            description: "创建商品分类",
        },
    ),
    (
        "PRODUCT_CATEGORY_UPDATE",
        PermissionDef {
            permission: &PRODUCT_CATEGORY_UPDATE,
            group: groups::CATALOG,
            description: "更新商品分类",
        },
    ),
    (
        "PRODUCT_CATEGORY_DELETE",
        PermissionDef {
            permission: &PRODUCT_CATEGORY_DELETE,
            group: groups::CATALOG,
            description: "删除商品分类",
        },
    ),
    (
        "PRODUCT_BRAND_LIST",
        PermissionDef {
            permission: &PRODUCT_BRAND_LIST,
            group: groups::CATALOG,
            description: "查询商品品牌列表",
        },
    ),
    (
        "PRODUCT_BRAND_CREATE",
        PermissionDef {
            permission: &PRODUCT_BRAND_CREATE,
            group: groups::CATALOG,
            description: "创建商品品牌",
        },
    ),
    (
        "PRODUCT_BRAND_UPDATE",
        PermissionDef {
            permission: &PRODUCT_BRAND_UPDATE,
            group: groups::CATALOG,
            description: "更新商品品牌",
        },
    ),
    (
        "PRODUCT_BRAND_DELETE",
        PermissionDef {
            permission: &PRODUCT_BRAND_DELETE,
            group: groups::CATALOG,
            description: "删除商品品牌",
        },
    ),
    (
        "UNIT_OF_MEASURE_LIST",
        PermissionDef {
            permission: &UNIT_OF_MEASURE_LIST,
            group: groups::CATALOG,
            description: "查询计量单位列表",
        },
    ),
    (
        "UNIT_OF_MEASURE_CREATE",
        PermissionDef {
            permission: &UNIT_OF_MEASURE_CREATE,
            group: groups::CATALOG,
            description: "创建计量单位",
        },
    ),
    (
        "UNIT_OF_MEASURE_UPDATE",
        PermissionDef {
            permission: &UNIT_OF_MEASURE_UPDATE,
            group: groups::CATALOG,
            description: "更新计量单位",
        },
    ),
    (
        "UNIT_OF_MEASURE_DELETE",
        PermissionDef {
            permission: &UNIT_OF_MEASURE_DELETE,
            group: groups::CATALOG,
            description: "删除计量单位",
        },
    ),
    (
        "SKU_ATTRIBUTE_LIST",
        PermissionDef {
            permission: &SKU_ATTRIBUTE_LIST,
            group: groups::CATALOG,
            description: "查询规格属性列表",
        },
    ),
    (
        "SKU_ATTRIBUTE_CREATE",
        PermissionDef {
            permission: &SKU_ATTRIBUTE_CREATE,
            group: groups::CATALOG,
            description: "创建规格属性",
        },
    ),
    (
        "SKU_ATTRIBUTE_UPDATE",
        PermissionDef {
            permission: &SKU_ATTRIBUTE_UPDATE,
            group: groups::CATALOG,
            description: "更新规格属性",
        },
    ),
    (
        "SKU_ATTRIBUTE_DELETE",
        PermissionDef {
            permission: &SKU_ATTRIBUTE_DELETE,
            group: groups::CATALOG,
            description: "删除规格属性",
        },
    ),
    (
        "SKU_ATTRIBUTE_VALUE_LIST",
        PermissionDef {
            permission: &SKU_ATTRIBUTE_VALUE_LIST,
            group: groups::CATALOG,
            description: "查询规格属性值列表",
        },
    ),
    (
        "SKU_ATTRIBUTE_VALUE_CREATE",
        PermissionDef {
            permission: &SKU_ATTRIBUTE_VALUE_CREATE,
            group: groups::CATALOG,
            description: "创建规格属性值",
        },
    ),
    (
        "SKU_ATTRIBUTE_VALUE_UPDATE",
        PermissionDef {
            permission: &SKU_ATTRIBUTE_VALUE_UPDATE,
            group: groups::CATALOG,
            description: "更新规格属性值",
        },
    ),
    (
        "SKU_ATTRIBUTE_VALUE_DELETE",
        PermissionDef {
            permission: &SKU_ATTRIBUTE_VALUE_DELETE,
            group: groups::CATALOG,
            description: "删除规格属性值",
        },
    ),
    (
        "PRODUCT_SELLABLE_SKU_LIST",
        PermissionDef {
            permission: &PRODUCT_SELLABLE_SKU_LIST,
            group: groups::CATALOG,
            description: "查询公司商品池",
        },
    ),
    (
        "PRODUCT_PRODUCT_LIST",
        PermissionDef {
            permission: &PRODUCT_PRODUCT_LIST,
            group: groups::CATALOG,
            description: "查询商品列表",
        },
    ),
    (
        "PRODUCT_PRODUCT_CREATE",
        PermissionDef {
            permission: &PRODUCT_PRODUCT_CREATE,
            group: groups::CATALOG,
            description: "创建商品",
        },
    ),
    (
        "PRODUCT_PRODUCT_UPDATE",
        PermissionDef {
            permission: &PRODUCT_PRODUCT_UPDATE,
            group: groups::CATALOG,
            description: "规格编辑商品",
        },
    ),
    (
        "PRODUCT_PRODUCT_REVISION_LIST",
        PermissionDef {
            permission: &PRODUCT_PRODUCT_REVISION_LIST,
            group: groups::CATALOG,
            description: "查询商品修订列表",
        },
    ),
    (
        "PRODUCT_SKU_LIST",
        PermissionDef {
            permission: &PRODUCT_SKU_LIST, group: groups::CATALOG, description: "查询SKU列表"
        },
    ),
    (
        "PRODUCT_SKU_REVISION_LIST",
        PermissionDef {
            permission: &PRODUCT_SKU_REVISION_LIST,
            group: groups::CATALOG,
            description: "查询SKU修订列表",
        },
    ),
    (
        "PRODUCT_VOUCHER_CATEGORY_PROFILE_LIST",
        PermissionDef {
            permission: &PRODUCT_VOUCHER_CATEGORY_PROFILE_LIST,
            group: groups::CATALOG,
            description: "查询卡券类目列表",
        },
    ),
    (
        "PRODUCT_VOUCHER_CATEGORY_CREATE",
        PermissionDef {
            permission: &PRODUCT_VOUCHER_CATEGORY_CREATE,
            group: groups::CATALOG,
            description: "创建卡券类目",
        },
    ),
    (
        "PRODUCT_VOUCHER_CATEGORY_UPDATE",
        PermissionDef {
            permission: &PRODUCT_VOUCHER_CATEGORY_UPDATE,
            group: groups::CATALOG,
            description: "更新卡券类目",
        },
    ),
    (
        "PRODUCT_PRODUCT_DETAIL",
        PermissionDef {
            permission: &PRODUCT_PRODUCT_DETAIL,
            group: groups::CATALOG,
            description: "查询商品详情",
        },
    ),
];
