/**
 * W14 资源专属字段 — 表单 / 回填 / 提交逻辑。
 * 新建 / 更新资料表单按 `RESOURCE_FIELDS` 渲染专属字段；
 * 提交命令携带强类型 `fields`，禁止退回通用 Record。
 * 字段声明表见 resource-fields-defs.ts；本文件统一再导出，保持既有导入路径不变。
 */

import { z } from "zod"

import { masterDataCopy } from "@/features/master-data/lib/copy"
import {
    RESOURCE_FIELDS,
    type ResourceFieldDef,
} from "@/features/master-data/lib/resource-fields-defs"
import type {
    MasterDataCenterView,
    MasterDataListItem,
    MasterDataResource,
    MasterDataResourceFields,
} from "@/features/master-data/types"

export {
    INVOICE_TYPE_OPTIONS,
    RESOURCE_FIELDS,
    SETTLEMENT_MODE_OPTIONS,
    SUPPLIER_CAPABILITY_OPTIONS,
    SUPPLIER_RATING_OPTIONS,
} from "@/features/master-data/lib/resource-fields-defs"
export type { ResourceFieldDef } from "@/features/master-data/lib/resource-fields-defs"

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
