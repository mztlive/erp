/** W14 基础资料 · 各资源专属可写字段（对齐 W14 §4.3 / §5.2 / §8.2）。 */

import type { ProductFields } from "@/features/master-data/api/types-product"

export type SellableItemFields = Readonly<{
    sku: string
    /** 采购发布给销售的公司商品池价格；不是任何一家供应商的成本。 */
    salesVisiblePriceGross: string
    /** 从 supplier_offering 聚合的可用供应商数量，只读投影。 */
    supplierCount?: string
    region?: string
    leadTime?: string
    fulfillmentModes?: string
}>

/**
 * 卡券类目可写字段：业务上一个卡券类目即一个 VOUCHER 类型的 SKU，
 * `voucherNo` 同时作为 `product_no` 与 `sku_no`（无需分别填写）。
 *
 * 分类 / 品牌 / 基础单位由后端默认补齐（共用卡券根分类、品牌「福尚云」、单位「张」），
 * 前端新建 Dialog 只提交编号与描述；可选字段保留给显式覆盖或兼容旧调用。
 */
export type VoucherCategoryFields = Readonly<{
    voucherNo: string
    description: string
    specification?: string
    /** 显式分类 ID；省略则用共用卡券根分类。 */
    categoryId?: string
    category?: string
    newCategoryCode?: string
    newCategoryName?: string
    newCategoryParentId?: string
    /** 显式品牌 ID；省略则用「福尚云」。 */
    brandId?: string
    brand?: string
    /** 显式基础单位 ID；省略则用「张」。 */
    baseUnitId?: string
    baseUnitCode?: string
    baseUnit?: string
    barcode?: string
    salesVisiblePriceGross?: string
    marketPrice?: string
}>

/** 商品分类字典：稳定代码 + 名称（name 在通用字段）+ 可选上级。 */
export type CategoryFields = Readonly<{
    code: string
    /** 上级分类稳定 ID；根分类为空。 */
    parentId?: string
    /** 兼容旧表单/展示：上级分类名称。 */
    parent?: string
    productKind?: string
}>

/** 品牌字典：稳定代码 + 名称（name 在通用字段）+ 可选 Logo。 */
export type BrandFields = Readonly<{
    code: string
    /** 品牌 Logo 展示文件名。 */
    logo?: string
    /** 品牌 Logo 已登记文件资产（D05）。 */
    logoAssetId?: string
    /** 品牌 Logo 可访问 URL（仅回显用）。 */
    logoPreviewUrl?: string
}>

/**
 * 计量单位字典：稳定代码 + 名称（name 在通用字段）+ 符号 + 数量小数位。
 * `unit_code` 创建后不可改；更新只改 name / symbol / quantity_scale / status。
 */
export type UnitOfMeasureFields = Readonly<{
    code: string
    symbol: string
    /** 允许数量小数位（0–6），表单以字符串承载。 */
    quantityScale: string
}>

export type SupplierFields = Readonly<{
    company: string
    creditCode?: string
    contactName?: string
    contactPhone?: string
    clearContact?: boolean
    address?: string
    clearAddress?: boolean
    settlement?: string
    capability?: string
    businessCategory?: string
    signingEntity?: string
    paymentEntity?: string
    qualification?: string
    contractNo?: string
    contractValidFrom?: string
    contractValidTo?: string
    contractFile?: string
    authorizationFile?: string
    authorizationValidFrom?: string
    authorizationValidTo?: string
    foodLicense?: string
    legalPersonIdCard?: string
    /** 资质字段 fileName → 已登记文件资产 id（D05）。 */
    qualificationFileAssetIds?: Readonly<Record<string, string>>
    contractFileAssetIds?: Readonly<Record<string, string>>
    authorizationFileAssetIds?: Readonly<Record<string, string>>
    foodLicenseFileAssetIds?: Readonly<Record<string, string>>
    legalPersonIdCardFileAssetIds?: Readonly<Record<string, string>>
    /** `资质类型::证书编号` → 适用能力代码；新资质默认适用当前勾选能力。 */
    qualificationCapabilityCodes?: Readonly<Record<string, readonly string[]>>
    taxNo?: string
    clearTaxProfile?: boolean
    bankName?: string
    bankAccount?: string
    clearBankAccount?: boolean
    invoiceType?: string
    invoiceTaxRate?: string
    initialScore?: string
    supplierRating?: string
    currentScore?: string
}>

type WarehouseFields = Readonly<Record<string, never>>

/**
 * 按资源强类型化的专属字段。
 * 正式提交不得退回通用 `Record<string, string>` 契约。
 */
export type MasterDataResourceFields = {
    "sellable-items": SellableItemFields
    products: ProductFields
    categories: CategoryFields
    brands: BrandFields
    "unit-of-measures": UnitOfMeasureFields
    "voucher-categories": VoucherCategoryFields
    suppliers: SupplierFields
    warehouses: WarehouseFields
}
