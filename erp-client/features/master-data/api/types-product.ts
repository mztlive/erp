/** W14 基础资料 · 商品（SPU）与 SKU 的可写字段及详情投影类型。 */

import type {
    LifecycleStatus,
    ProductKind,
} from "@/features/master-data/api/types-core"

/** 规格维度（如 颜色 / 规格），取值组合生成 SKU。 */
export type ProductSpecDimension = Readonly<{
    name: string
    values: readonly string[]
}>

/**
 * SKU 行：保存公司商品身份与媒体；`salePrice` 是商品池销售可见价的
 * 编辑投影，命令处理器必须写入商品池修订，不得把它当作供应商成本。
 * 规格取值由 SPU 规格维度组合得出。
 *
 * 供应商、供给模式、供货价、底价、进项税、费用、MOQ、区域、能力与
 * 有效期全部归 W21 `supplier_offering` / `supplier_offering_revision`；
 * 不得嵌入 SKU 修订形成第二份供给事实或草稿。
 */
export type ProductSkuFields = Readonly<{
    skuId?: string
    /** 当前 SKU 修订 ID；编辑既有行时作为乐观并发条件提交。 */
    skuRevisionId?: string
    /** 该行来自历史停用 SKU；再次启用时必须提交明确意图。 */
    requiresExplicitReenable?: boolean
    /**
     * 规范化规格签名（`specification_signature`）：系统按规格属性组合派生，
     * 创建后不可变；用于判断某行能否延续原 `sku_id`。业务 UI 不展示、不手填。
     */
    specificationSignature?: string
    /**
     * 产品编码 = `sku_no`。
     * 系统按规格组合默认生成，允许手动覆盖；仅为全局唯一业务编码，
     * 不能作为身份恢复或重绑键。
     */
    skuNo: string
    /**
     * SKU 名称 = `sku_revision.name`。
     * 与商品（SPU）名称独立维护；新建行可默认带入商品名称后由用户改写。
     */
    name: string
    /** 与 specs 顺序对齐的属性取值。 */
    attributeValues: readonly string[]
    /** 展示用规格文案，如「颜色：红 / 规格：大」。 */
    specLabel: string
    barcode?: string
    /** SKU 主图（单张，展示名）。 */
    mainImage: string
    /** SKU 主图可访问地址（仅回显用）。 */
    mainImagePreviewUrl?: string
    /** SKU 主图已登记文件资产（D05）。 */
    mainImageAssetId?: string
    /** 公司商品池销售可见价；销售可见，采购成本不可见。 */
    salePrice?: string
    /** 市场价（参考展示，非正式发布价）。 */
    marketPrice?: string
    baseUnit?: string
    /** 独立于启停状态的 SKU 上架状态。 */
    listingStatus?: "LISTED" | "UNLISTED"
    lifecycleStatus: LifecycleStatus
}>

/** 商品（SPU）可写字段：规格组合出 SKU；无「规格标识」手填字段。 */
export type ProductFields = Readonly<{
    /** 商品稳定身份当前启停状态；编辑不得隐式改为启用。 */
    lifecycleStatus: LifecycleStatus
    /** 商品编号（product_no）；创建必填，创建后不可变。 */
    productNo: string
    /** 公司审核后的商品描述；可由供应商来源资料预填，但保存后独立维护。 */
    description?: string
    /** 公司审核后的 SPU 规格/服务内容；未展示编辑时仍须原样保留。 */
    specification?: string
    /** `unit_of_measure` 稳定身份与代码；`baseUnit` 仅为显示快照。 */
    baseUnitId: string
    baseUnitCode: string
    baseUnit: string
    categoryId: string
    category: string
    brandId: string
    brand: string
    /**
     * 公司商品类型（`product.product_kind`）：独立必填稳定属性，
     * W14 正向创建必须显式提交，写入后不可变；分类仅校验兼容性。
     * 空字符串表示草稿未选择，提交前 fail-closed。
     */
    productKind: ProductKind | ""
    /** SPU 轮播图（多张，可空）。 */
    carouselImages: readonly string[]
    /** SPU 详情图（多张，可空）。 */
    detailImages: readonly string[]
    /** 轮播图 fileName → 可访问预览/来源 URL（回显与再次保存用） */
    carouselPreviewUrls: Readonly<Record<string, string>>
    detailPreviewUrls: Readonly<Record<string, string>>
    /** 轮播图 fileName → 已登记文件资产 id */
    carouselFileAssetIds: Readonly<Record<string, string>>
    detailFileAssetIds: Readonly<Record<string, string>>
    specs: readonly ProductSpecDimension[]
    skus: readonly ProductSkuFields[]
}>

/** 对象中心展示用的商品详情投影。 */
export type ProductDetailView = Readonly<{
    lifecycleStatus: LifecycleStatus
    productNo: string
    description?: string
    specification?: string
    baseUnitId: string
    baseUnitCode: string
    baseUnit: string
    categoryId: string
    category: string
    brandId: string
    brand: string
    carouselImages: readonly string[]
    detailImages: readonly string[]
    carouselPreviewUrls: Readonly<Record<string, string>>
    detailPreviewUrls: Readonly<Record<string, string>>
    carouselFileAssetIds: Readonly<Record<string, string>>
    detailFileAssetIds: Readonly<Record<string, string>>
    specs: readonly ProductSpecDimension[]
    skus: readonly ProductSkuFields[]
}>
