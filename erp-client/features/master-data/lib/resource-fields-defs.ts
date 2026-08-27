/**
 * W14 资源专属字段声明表 — 按资源强类型化。
 * 本文件为纯声明式配置表（资源 → 字段定义与选项常量），故允许保持较长；
 * 表单/回填逻辑见 resource-fields.ts，均经 `@/features/master-data/lib/resource-fields` 统一出口。
 */

import { masterDataCopy } from "@/features/master-data/lib/copy"
import type { MasterDataResource } from "@/features/master-data/types"
import { SUPPLIER_PAYMENT_TERM_OPTIONS } from "@/lib/business-options"

const REGION_OPTIONS = [
    "华东",
    "华南",
    "华北",
    "西南",
    "华中",
    "西北",
    "全国",
] as const

/** 供应商结算方式（稳定值对齐 supplier_commercial_profile_revision.settlement_mode）。 */
export const SETTLEMENT_MODE_OPTIONS = [
    { value: "prepayment", label: "预付款" },
    { value: "pay_after_use", label: "先用后付" },
    { value: "cash_settlement", label: "现结" },
] as const

export { SUPPLIER_PAYMENT_TERM_OPTIONS }

/** 供应商发票类型。 */
export const INVOICE_TYPE_OPTIONS = [
    "增值税专用发票",
    "增值税普通发票",
    "电子发票",
] as const

/** 供应商评级。 */
export const SUPPLIER_RATING_OPTIONS = ["A 级", "B 级", "C 级", "D 级"] as const

/** 供应商能力（多选，对齐 erp-phase-1 §4.5）。 */
export const SUPPLIER_CAPABILITY_OPTIONS = [
    "实物商品",
    "虚拟商品",
    "线下服务",
    "API",
    "印刷",
] as const

const PRODUCT_KIND_OPTIONS = ["实物", "虚拟", "服务", "卡券"] as const

/** 计量单位允许的数量小数位（对齐 unit_of_measure.quantity_scale 0–6）。 */
const QUANTITY_SCALE_OPTIONS = ["0", "1", "2", "3", "4", "5", "6"] as const

type ResourceFieldKind =
    | "text"
    | "textarea"
    | "select"
    | "checkbox-group"
    | "category-parent"
    | "media"
    | "media-list"

export type ResourceFieldDef = Readonly<{
    key: string
    label: string
    kind: ResourceFieldKind
    options?: readonly string[]
    required?: boolean
    /** 是否进入列表关键信息（keyFacts）。 */
    listFact?: boolean
    /** 列表/历史版本中的其他同义标签，用于编辑回填。 */
    aliases?: readonly string[]
    /** 宽表单分区（仅商品 SKU 等复杂资源使用）。 */
    section?: "identity" | "catalog" | "media" | "default"
    /** media / media-list 字段说明。 */
    hint?: string
}>

/** 按资源声明可维护专属字段；仓库在写门禁未确认前无维护字段。 */
export const RESOURCE_FIELDS: Readonly<
    Record<MasterDataResource, readonly ResourceFieldDef[]>
> = {
    "sellable-items": [
        {
            key: "sku",
            label: masterDataCopy.fSku,
            kind: "text",
            required: true,
            listFact: true,
        },
        {
            key: "salesVisiblePriceGross",
            label: masterDataCopy.fSalesVisiblePrice,
            kind: "text",
            required: true,
            listFact: true,
        },
        {
            key: "supplierCount",
            label: masterDataCopy.fSupplierCount,
            kind: "text",
            listFact: true,
        },
        {
            key: "region",
            label: masterDataCopy.fRegion,
            kind: "select",
            options: REGION_OPTIONS,
            listFact: true,
        },
        { key: "leadTime", label: masterDataCopy.fLeadTime, kind: "text" },
        {
            key: "fulfillmentModes",
            label: masterDataCopy.fFulfillmentModes,
            kind: "text",
        },
    ],
    /**
     * 商品（SPU）列表/概览事实标签。
     * 完整维护在商品详情页（规格组合 → SKU，主图在 SKU，轮播/详情图在 SPU），
     * 不再使用宽对话框 + 规格标识手填模型。
     */
    products: [
        {
            key: "baseUnit",
            label: masterDataCopy.fBaseUnit,
            kind: "select",
            required: true,
            listFact: true,
            section: "identity",
        },
        {
            key: "category",
            label: masterDataCopy.fCategory,
            kind: "select",
            required: true,
            listFact: true,
            section: "catalog",
        },
        {
            key: "brand",
            label: masterDataCopy.fBrand,
            kind: "select",
            required: true,
            listFact: true,
            section: "catalog",
        },
    ],
    categories: [
        {
            key: "code",
            label: masterDataCopy.fCategoryCode,
            kind: "text",
            required: true,
            listFact: true,
        },
        {
            key: "parentId",
            label: masterDataCopy.fParentCategory,
            kind: "category-parent",
            listFact: true,
            aliases: ["上级分类"],
        },
        {
            key: "productKind",
            label: masterDataCopy.fProductKind,
            kind: "select",
            options: PRODUCT_KIND_OPTIONS,
            listFact: true,
        },
    ],
    brands: [
        {
            key: "code",
            label: masterDataCopy.fBrandCode,
            kind: "text",
            required: true,
            listFact: true,
        },
        {
            key: "logo",
            label: masterDataCopy.fBrandLogo,
            kind: "media",
            listFact: true,
            hint: masterDataCopy.brandLogoHint,
            aliases: ["Logo", "品牌 Logo"],
        },
    ],
    "unit-of-measures": [
        {
            key: "code",
            label: masterDataCopy.fUnitCode,
            kind: "text",
            required: true,
            listFact: true,
            aliases: ["单位代码", "unit_code"],
        },
        {
            key: "symbol",
            label: masterDataCopy.fUnitSymbol,
            kind: "text",
            required: true,
            listFact: true,
            aliases: ["符号", "单位符号"],
        },
        {
            key: "quantityScale",
            label: masterDataCopy.fQuantityScale,
            kind: "select",
            options: QUANTITY_SCALE_OPTIONS,
            required: true,
            listFact: true,
            aliases: ["小数位", "数量小数位"],
        },
    ],
    "voucher-categories": [
        {
            key: "sku",
            label: masterDataCopy.fSku,
            kind: "text",
            required: true,
            listFact: true,
            aliases: ["卡券 SKU"],
        },
        {
            key: "description",
            label: masterDataCopy.fDescription,
            kind: "textarea",
            listFact: true,
            aliases: ["说明"],
        },
    ],
    suppliers: [
        {
            key: "company",
            label: masterDataCopy.fCompany,
            kind: "text",
            required: true,
            listFact: true,
        },
        {
            key: "contactName",
            label: masterDataCopy.fContactName,
            kind: "text",
            listFact: true,
        },
        {
            key: "creditCode",
            label: masterDataCopy.fCreditCode,
            kind: "text",
        },
        {
            key: "contactPhone",
            label: masterDataCopy.fContactPhone,
            kind: "text",
            listFact: true,
        },
        {
            key: "address",
            label: masterDataCopy.fAddress,
            kind: "text",
        },
        {
            key: "settlement",
            label: masterDataCopy.fSettlement,
            kind: "select",
            options: SETTLEMENT_MODE_OPTIONS.map((option) => option.value),
            required: true,
            listFact: true,
            aliases: ["商务结算", "商务结算版本"],
        },
        {
            key: "paymentTerm",
            label: masterDataCopy.fPaymentTerm,
            kind: "select",
            options: SUPPLIER_PAYMENT_TERM_OPTIONS.map(
                (option) => option.value,
            ),
            required: true,
            listFact: true,
        },
        {
            key: "capability",
            label: masterDataCopy.fCapability,
            kind: "checkbox-group",
            options: SUPPLIER_CAPABILITY_OPTIONS,
            listFact: true,
            aliases: ["能力版本"],
        },
        {
            key: "businessCategory",
            label: masterDataCopy.fBusinessCategory,
            kind: "text",
        },
        {
            key: "signingEntity",
            label: masterDataCopy.fSigningEntity,
            kind: "text",
        },
        {
            key: "paymentEntity",
            label: masterDataCopy.fPaymentEntity,
            kind: "text",
        },
        {
            key: "qualification",
            label: masterDataCopy.fQualification,
            kind: "media-list",
            hint: masterDataCopy.supplierQualificationHint,
        },
        {
            key: "contractNo",
            label: masterDataCopy.fContractNo,
            kind: "text",
        },
        {
            key: "contractValidFrom",
            label: masterDataCopy.fContractValidFrom,
            kind: "text",
        },
        {
            key: "contractValidTo",
            label: masterDataCopy.fContractValidTo,
            kind: "text",
        },
        {
            key: "contractFile",
            label: masterDataCopy.fContractFile,
            kind: "media-list",
            hint: masterDataCopy.supplierQualificationHint,
        },
        {
            key: "authorizationFile",
            label: masterDataCopy.fAuthorizationFile,
            kind: "media-list",
            hint: masterDataCopy.supplierQualificationHint,
        },
        {
            key: "authorizationValidFrom",
            label: masterDataCopy.fAuthorizationValidFrom,
            kind: "text",
        },
        {
            key: "authorizationValidTo",
            label: masterDataCopy.fAuthorizationValidTo,
            kind: "text",
        },
        {
            key: "foodLicense",
            label: masterDataCopy.fFoodLicense,
            kind: "media-list",
            hint: masterDataCopy.supplierQualificationHint,
        },
        {
            key: "legalPersonIdCard",
            label: masterDataCopy.fLegalPersonIdCard,
            kind: "media-list",
            hint: masterDataCopy.supplierQualificationHint,
        },
        { key: "taxNo", label: masterDataCopy.fTaxNo, kind: "text" },
        { key: "bankName", label: masterDataCopy.fBankName, kind: "text" },
        {
            key: "bankAccount",
            label: masterDataCopy.fBankAccount,
            kind: "text",
        },
        {
            key: "invoiceType",
            label: masterDataCopy.fInvoiceType,
            kind: "select",
            options: INVOICE_TYPE_OPTIONS,
            listFact: true,
        },
        {
            key: "invoiceTaxRate",
            label: masterDataCopy.fInvoiceTaxRate,
            kind: "text",
        },
        {
            key: "initialScore",
            label: masterDataCopy.fInitialScore,
            kind: "text",
        },
        {
            key: "supplierRating",
            label: masterDataCopy.fSupplierRating,
            kind: "select",
            options: SUPPLIER_RATING_OPTIONS,
            listFact: true,
        },
        {
            key: "currentScore",
            label: masterDataCopy.fCurrentScore,
            kind: "text",
        },
    ],
    warehouses: [],
}
