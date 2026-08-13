/**
 * W14 资源专属字段 — 按资源强类型化（对齐 docs/ui-workspaces/w14-basic-data.md §4.3/§5.2/§8.2）。
 * 新建 / 更新资料表单按 `RESOURCE_FIELDS` 渲染专属字段；
 * 提交命令携带强类型 `fields`，禁止退回通用 Record。
 */

import { z } from "zod"

import { masterDataCopy } from "@/features/master-data/lib/copy"
import type {
    MasterDataCenterView,
    MasterDataListItem,
    MasterDataResource,
    MasterDataResourceFields,
} from "@/features/master-data/types"

const REGION_OPTIONS = [
    "华东",
    "华南",
    "华北",
    "西南",
    "华中",
    "西北",
    "全国",
] as const

/** 供应商结算方式（对齐 supplier_commercial_profile_revision.settlement_mode）。 */
export const SETTLEMENT_MODE_OPTIONS = ["预付款", "先用后付", "现结"] as const

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
            options: SETTLEMENT_MODE_OPTIONS,
            listFact: true,
            aliases: ["商务结算", "商务结算版本"],
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

export type ResourceFormValues = {
    name: string
    effectiveFrom: string
    effectiveTo: string
    changeReason: string
    [field: string]: string
}

/**
 * 品牌 / 商品分类 / 计量单位为即时字典，表单不收集生效期间；
 * 提交时由服务端/会话层默认「立即生效」。
 */
export function usesEffectivePeriod(resource: MasterDataResource): boolean {
    return (
        resource !== "brands" &&
        resource !== "categories" &&
        resource !== "unit-of-measures"
    )
}

const DATE_FORMAT = /^\d{4}-\d{2}-\d{2}$/

function isValidDate(value: string): boolean {
    if (!DATE_FORMAT.test(value)) return false
    const [year, month, day] = value.split("-").map(Number)
    const date = new Date(Date.UTC(year, month - 1, day))
    return (
        date.getUTCFullYear() === year &&
        date.getUTCMonth() === month - 1 &&
        date.getUTCDate() === day
    )
}

/** 生效开始 / 结束字段共用：格式 + 非空。 */
function effectiveDateField(label: string): z.ZodString {
    return z
        .string()
        .min(1, `请填写${label}`)
        .refine(isValidDate, `${label}格式不正确，请使用 YYYY-MM-DD`)
}

/** 通用基础字段 + 当前资源专属字段（必填规则），供新建 / 更新表单共用。 */
export function buildResourceSchema(
    resource: MasterDataResource,
    defs: readonly ResourceFieldDef[],
) {
    const dynamic: Record<string, z.ZodString> = {}
    for (const def of defs) {
        if (def.kind === "media" && def.required) {
            dynamic[def.key] = z
                .string()
                .trim()
                .min(1, masterDataCopy.mediaMainRequired)
        } else if (def.required) {
            dynamic[def.key] = z.string().trim().min(1, `请填写${def.label}`)
        } else {
            dynamic[def.key] = z.string()
        }
    }
    // 计量单位名称常为单字（张/件/个），其余资源仍要求至少 2 字。
    const nameSchema =
        resource === "unit-of-measures"
            ? z.string().trim().min(1, "请填写名称")
            : z.string().trim().min(2, "请填写名称")

    return z
        .object({
            name: nameSchema,
            effectiveFrom: usesEffectivePeriod(resource)
                ? effectiveDateField("生效开始")
                : z.string(),
            effectiveTo: z
                .string()
                .refine(
                    (value) => value === "" || isValidDate(value),
                    "生效结束格式不正确，请使用 YYYY-MM-DD",
                ),
            changeReason: z.string().trim().min(2, "请填写变更原因"),
            ...dynamic,
        })
        .refine(
            (value) =>
                !usesEffectivePeriod(resource) ||
                value.effectiveTo === "" ||
                value.effectiveTo >= value.effectiveFrom,
            {
                message: "生效结束不能早于生效开始",
                path: ["effectiveTo"],
            },
        )
}

/** 字典类资源默认立即生效的业务日。 */
export function defaultImmediateEffectiveFrom(): string {
    return new Date().toISOString().slice(0, 10)
}

export function emptyResourceFieldValues(
    resource: MasterDataResource,
): Record<string, string> {
    const defaults = Object.fromEntries(
        RESOURCE_FIELDS[resource].map((def) => [def.key, ""]),
    )
    // 计量单位默认整数数量（小数位 0），减少新建时必填遗漏。
    if (resource === "unit-of-measures") {
        defaults.quantityScale = "0"
    }
    return defaults
}

export type ResourceFieldValues = Record<string, string>

/**
 * 从编辑目标（列表行或对象中心）回填当前值。
 * 以展示标签匹配；`aliases` 覆盖列表与中心用词差异。
 */
export function currentResourceFieldValues(
    target: MasterDataListItem | MasterDataCenterView,
): ResourceFieldValues {
    const resource: MasterDataResource =
        "currentRevision" in target ? target.resource : target.objectType
    const facts =
        "currentRevision" in target
            ? target.currentRevision.fields
            : target.keyFacts
    const byLabel = new Map(facts.map((fact) => [fact.label, fact.value]))
    const out: ResourceFieldValues = {}
    for (const def of RESOURCE_FIELDS[resource]) {
        // 分类上级存稳定 ID；展示事实是名称，优先从列表行回填
        if (def.key === "parentId" && !("currentRevision" in target)) {
            out.parentId = target.parentStableId ?? ""
            continue
        }
        if (
            def.key === "code" &&
            !("currentRevision" in target) &&
            target.dictionaryCode
        ) {
            out.code = target.dictionaryCode
            continue
        }
        if (
            def.key === "productKind" &&
            !("currentRevision" in target) &&
            target.productKind
        ) {
            out.productKind = target.productKind
            continue
        }
        const matched =
            byLabel.get(def.label) ??
            def.aliases
                ?.map((alias) => byLabel.get(alias))
                .find((value) => value !== undefined && value !== "")
        // 占位符 / 空值不回填，避免表单被「—」「****」污染
        if (
            matched === undefined ||
            matched === "" ||
            matched === "—" ||
            matched === "****" ||
            matched === "（敏感字段，需授权查看）" ||
            matched === "（请从财务上下文查看）" ||
            /^\d+\s*项$/.test(matched)
        ) {
            continue
        }
        // 展示事实可能带「（N 张）」摘要后缀，回填表单时去掉
        if (def.kind === "media-list") {
            out[def.key] = matched.replace(/（\d+\s*张）\s*$/, "").trim()
        } else if (def.key === "parentId") {
            out.parentId = matched === "（根分类）" ? "" : matched
        } else {
            out[def.key] = matched
        }
    }
    if (
        resource === "categories" &&
        !("currentRevision" in target) &&
        target.parentStableId !== undefined
    ) {
        out.parentId = target.parentStableId
    }
    return out
}

function pickField(
    values: ResourceFieldValues,
    key: string,
): string | undefined {
    const value = values[key]
    return value?.trim() ? value.trim() : undefined
}

/** 表单值 → 当前资源的强类型字段对象。 */
export function buildResourceFields(
    resource: MasterDataResource,
    values: ResourceFieldValues,
): MasterDataResourceFields[MasterDataResource] {
    switch (resource) {
        case "sellable-items":
            return {
                sku: pickField(values, "sku") ?? "",
                salesVisiblePriceGross:
                    pickField(values, "salesVisiblePriceGross") ?? "",
                supplierCount: pickField(values, "supplierCount"),
                region: pickField(values, "region"),
                leadTime: pickField(values, "leadTime"),
                fulfillmentModes: pickField(values, "fulfillmentModes"),
            }
        case "products":
            // 商品完整字段由商品表单页直接组装 ProductFields；此处仅兜底空结构。
            return {
                lifecycleStatus: "ENABLED",
                productNo: "",
                specification: "",
                baseUnitId: "",
                baseUnitCode: "",
                baseUnit: pickField(values, "baseUnit") ?? "",
                categoryId: "",
                category: pickField(values, "category") ?? "",
                brandId: "",
                brand: pickField(values, "brand") ?? "",
                productKind: "",
                carouselImages: [],
                detailImages: [],
                carouselPreviewUrls: {},
                detailPreviewUrls: {},
                carouselFileAssetIds: {},
                detailFileAssetIds: {},
                specs: [],
                skus: [],
            }
        case "categories":
            return {
                code: pickField(values, "code") ?? "",
                parentId: pickField(values, "parentId") || undefined,
                parent: pickField(values, "parent"),
                productKind: pickField(values, "productKind"),
            }
        case "brands":
            return {
                code: pickField(values, "code") ?? "",
                logo: pickField(values, "logo"),
                logoAssetId: pickField(values, "logoAssetId"),
                logoPreviewUrl: pickField(values, "logoPreviewUrl"),
            }
        case "unit-of-measures":
            return {
                code: pickField(values, "code") ?? "",
                symbol: pickField(values, "symbol") ?? "",
                quantityScale: pickField(values, "quantityScale") ?? "0",
            }
        case "voucher-categories":
            // 卡券类目创建走 VoucherCategoryFormDialog；此处仅占位满足强类型契约。
            // 分类 / 品牌 / 单位由后端默认（卡券根分类、福尚云、张）。
            return {
                voucherNo: pickField(values, "sku") ?? "",
                description: pickField(values, "description") ?? "",
            }
        case "suppliers":
            return {
                company: pickField(values, "company") ?? "",
                creditCode: pickField(values, "creditCode"),
                contactName: pickField(values, "contactName"),
                contactPhone: pickField(values, "contactPhone"),
                address: pickField(values, "address"),
                settlement: pickField(values, "settlement"),
                capability: pickField(values, "capability"),
                businessCategory: pickField(values, "businessCategory"),
                signingEntity: pickField(values, "signingEntity"),
                paymentEntity: pickField(values, "paymentEntity"),
                qualification: pickField(values, "qualification"),
                contractNo: pickField(values, "contractNo"),
                contractValidFrom: pickField(values, "contractValidFrom"),
                contractValidTo: pickField(values, "contractValidTo"),
                contractFile: pickField(values, "contractFile"),
                authorizationFile: pickField(values, "authorizationFile"),
                authorizationValidFrom: pickField(
                    values,
                    "authorizationValidFrom",
                ),
                authorizationValidTo: pickField(values, "authorizationValidTo"),
                foodLicense: pickField(values, "foodLicense"),
                legalPersonIdCard: pickField(values, "legalPersonIdCard"),
                taxNo: pickField(values, "taxNo"),
                bankName: pickField(values, "bankName"),
                bankAccount: pickField(values, "bankAccount"),
                invoiceType: pickField(values, "invoiceType"),
                invoiceTaxRate: pickField(values, "invoiceTaxRate"),
                initialScore: pickField(values, "initialScore"),
                supplierRating: pickField(values, "supplierRating"),
                currentScore: pickField(values, "currentScore"),
            }
        case "warehouses":
            return {}
    }
}

/** 解析 media-list 逗号分隔文件名。 */
export function parseMediaList(value: string | undefined): string[] {
    if (!value?.trim()) return []
    return value
        .split(",")
        .map((s) => s.trim())
        .filter(Boolean)
}

/** 序列化 media-list。 */
export function joinMediaList(names: readonly string[]): string {
    return names.filter(Boolean).join(", ")
}

