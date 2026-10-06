import { z } from "zod"
import type {
    NewProductInput,
    PortalApplication,
    PortalDictionary,
    RawDictionary,
} from "@/features/supplier-portal/types"

const hierarchyNode = z.object({
    id: z.string().min(1),
    version: z.number().int().positive(),
    name: z.string(),
    parent_id: z.string().nullable(),
    product_kind: z.string(),
})
const mappingSchema = z.object({
    brandId: z.string().min(1),
    brandVersion: z.number().int().positive(),
    categoryId: z.string().min(1),
    categoryVersion: z.number().int().positive(),
    categoryHierarchy: z.array(hierarchyNode).min(1),
    mappings: z
        .array(
            z.object({
                rowId: z.string().min(1),
                unitId: z.string().min(1),
                unitVersion: z.number().int().positive(),
                unitSynonymConfirmed: z.boolean(),
                unitSynonymReason: z.string(),
            }),
        )
        .min(1),
})
export type ReviewMappingSelection = z.infer<typeof mappingSchema>

const planSchema = z.object({
    schemaVersion: z.literal(1),
    actorId: z.string().min(1),
    supplierId: z.string().min(1),
    sourceRequestId: z.string().min(1),
    sourceVersion: z.number().int().positive(),
    sourceName: z.string(),
    targetRequestId: z.string().min(1),
    targetVersion: z.number().int().positive(),
    targetInputIdentity: z.string().min(1),
    mapping: mappingSchema,
})
export type ReviewMappingPlan = z.infer<typeof planSchema>
export type ReviewDictionaries = {
    brands: PortalDictionary[]
    categories: PortalDictionary[]
    units: PortalDictionary[]
}

type Sku = NewProductInput["skus"][number]
const rawDictionarySchema = z.object({ raw_name: z.string() }).passthrough()
const productSchema = z
    .object({
        name: z.string(),
        product_kind: z.string(),
        brand: rawDictionarySchema,
        category: rawDictionarySchema,
        skus: z
            .array(
                z
                    .object({
                        row_id: z.string().min(1),
                        name: z.string(),
                        ordering_code: z.string(),
                        unit: rawDictionarySchema,
                        quote_basis: z.string().nullish(),
                        packaging: z
                            .object({
                                original_unit: z.string(),
                                base_unit: z.string(),
                                units_per_package: z.string(),
                                original_unit_price: z.string(),
                                conversion_confirmed_by_supplier: z.boolean(),
                            })
                            .passthrough()
                            .nullish(),
                    })
                    .passthrough(),
            )
            .min(1),
    })
    .passthrough()

export function reviewProduct(
    application: PortalApplication,
): NewProductInput | null {
    if (application.kind !== "new_product") return null
    const parsed = productSchema.safeParse(
        application.submitted_snapshot ?? application.input,
    )
    return parsed.success ? (parsed.data as unknown as NewProductInput) : null
}

function selectedAllows(
    raw: RawDictionary,
    id: string,
    version: number,
): boolean {
    return (
        (!raw.selected_id || raw.selected_id === id) &&
        (raw.expected_version == null || raw.expected_version === version)
    )
}

function canonical(value: unknown): string {
    if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`
    if (value && typeof value === "object") {
        return `{${Object.entries(value)
            .sort(([left], [right]) => left.localeCompare(right))
            .map(([key, item]) => `${JSON.stringify(key)}:${canonical(item)}`)
            .join(",")}}`
    }
    return JSON.stringify(value) ?? "null"
}

/** 对完整冻结原稿取摘要；计划只保存摘要，避免把大量原稿写入浏览器存储。 */
export async function reviewInputIdentity(
    input: NewProductInput,
): Promise<string> {
    const digest = await crypto.subtle.digest(
        "SHA-256",
        new TextEncoder().encode(canonical(input)),
    )
    return Array.from(new Uint8Array(digest), (byte) =>
        byte.toString(16).padStart(2, "0"),
    ).join("")
}

export function confirmedMappingIssue(
    input: NewProductInput,
    mapping: ReviewMappingSelection,
    dictionaries: ReviewDictionaries,
): string | null {
    if (
        !input.brand.raw_name.trim() ||
        !input.category.raw_name.trim() ||
        input.skus.some((sku) => !sku.unit.raw_name.trim())
    )
        return "原始品牌、分类或单位缺失，请先单独核对。"
    if (!mappingSchema.safeParse(mapping).success)
        return "请先逐项确认品牌、完整分类路径和各规格单位。"
    const brand = dictionaries.brands.find(
        (item) => item.id === mapping.brandId,
    )
    const category = dictionaries.categories.find(
        (item) => item.id === mapping.categoryId,
    )
    if (
        !brand ||
        brand.version !== mapping.brandVersion ||
        !selectedAllows(input.brand, brand.id, brand.version)
    )
        return "品牌版本或供应商已选品牌已变化，请重新核对。"
    if (
        !category ||
        category.version !== mapping.categoryVersion ||
        category.product_kind !== input.product_kind ||
        !selectedAllows(input.category, category.id, category.version) ||
        canonical(category.hierarchy) !== canonical(mapping.categoryHierarchy)
    )
        return "完整分类路径、商品类型或分类版本已变化，请重新核对。"
    const last = mapping.categoryHierarchy.at(-1)
    if (
        !last ||
        last.id !== category.id ||
        last.version !== category.version ||
        mapping.categoryHierarchy.some(
            (node, index) =>
                node.product_kind !== input.product_kind ||
                node.parent_id !==
                    (index === 0
                        ? null
                        : mapping.categoryHierarchy[index - 1]?.id),
        )
    )
        return "请先明确确认包含目标分类的完整路径。"
    if (
        mapping.mappings.length !== input.skus.length ||
        new Set(mapping.mappings.map((row) => row.rowId)).size !==
            input.skus.length
    )
        return "请先确认本申请全部规格的单位。"
    for (const sku of input.skus) {
        const row = mapping.mappings.find((item) => item.rowId === sku.row_id)
        const unit = dictionaries.units.find((item) => item.id === row?.unitId)
        if (
            !row ||
            !unit ||
            unit.version !== row.unitVersion ||
            !selectedAllows(sku.unit, unit.id, unit.version)
        )
            return "规格单位版本或供应商已选单位已变化，请重新核对。"
        if (
            sku.packaging &&
            (!sku.packaging.conversion_confirmed_by_supplier ||
                sku.packaging.base_unit.trim() !== sku.unit.raw_name.trim())
        )
            return "包装未经供应商确认或基础单位口径不同，不能应用相同映射。"
        if (
            ![unit.name, unit.code].some(
                (name) => name?.trim() === sku.unit.raw_name.trim(),
            ) &&
            (!row.unitSynonymConfirmed || !row.unitSynonymReason.trim())
        )
            return "单位同义映射需要逐行确认同义含义并填写说明。"
    }
    return null
}

function sameUnitMeaning(source: Sku, target: Sku): boolean {
    return (
        !!source.unit.raw_name.trim() &&
        source.unit.raw_name.trim() === target.unit.raw_name.trim() &&
        (source.quote_basis?.trim() ?? "") ===
            (target.quote_basis?.trim() ?? "") &&
        canonical(source.packaging ?? null) ===
            canonical(target.packaging ?? null)
    )
}

/** 目标每一行须匹配同口径来源；不同含义或冲突引用的申请整个排除。 */
export function applicableMapping(
    sourceApplication: PortalApplication,
    source: NewProductInput,
    mapping: ReviewMappingSelection,
    targetApplication: PortalApplication,
): { mapping: ReviewMappingSelection | null; issue: string | null } {
    const target = reviewProduct(targetApplication)
    if (
        targetApplication.status !== "pending" ||
        !target ||
        !sourceApplication.supplier_id ||
        targetApplication.supplier_id !== sourceApplication.supplier_id ||
        target.product_kind !== source.product_kind ||
        !source.brand.raw_name.trim() ||
        !source.category.raw_name.trim() ||
        source.brand.raw_name.trim() !== target.brand.raw_name.trim() ||
        source.category.raw_name.trim() !== target.category.raw_name.trim()
    )
        return {
            mapping: null,
            issue: "供应商、商品类型、原始品牌或完整分类路径不同。",
        }
    if (
        !selectedAllows(target.brand, mapping.brandId, mapping.brandVersion) ||
        !selectedAllows(
            target.category,
            mapping.categoryId,
            mapping.categoryVersion,
        )
    )
        return {
            mapping: null,
            issue: "供应商已选品牌或分类与本次映射不同，需单独核对。",
        }
    const rows: ReviewMappingSelection["mappings"] = []
    for (const sku of target.skus) {
        const options = source.skus
            .filter((row) => sameUnitMeaning(row, sku))
            .map((row) => {
                const selected = mapping.mappings.find(
                    (item) => item.rowId === row.row_id,
                )
                return selected
                    ? {
                          rowId: selected.rowId,
                          unitId: selected.unitId,
                          unitVersion: selected.unitVersion,
                          unitSynonymConfirmed: selected.unitSynonymConfirmed,
                          unitSynonymReason: selected.unitSynonymReason,
                      }
                    : undefined
            })
            .filter(
                (row): row is ReviewMappingSelection["mappings"][number] =>
                    !!row,
            )
        const option = options[0]
        if (
            !option ||
            options.some(
                (row) =>
                    canonical({ ...row, rowId: "" }) !==
                    canonical({ ...option, rowId: "" }),
            ) ||
            !selectedAllows(sku.unit, option.unitId, option.unitVersion)
        )
            return {
                mapping: null,
                issue: "有规格单位原文、包装、报价口径不同或已选单位冲突，需单独核对。",
            }
        rows.push({ ...option, rowId: sku.row_id })
    }
    return {
        mapping: {
            ...mapping,
            categoryHierarchy: mapping.categoryHierarchy.map((row) => ({
                ...row,
            })),
            mappings: rows,
        },
        issue: null,
    }
}

export function mappingConfirmationIdentity(
    application: PortalApplication,
    mapping: ReviewMappingSelection,
): string {
    return canonical({
        id: application.id,
        version: application.version,
        mapping,
    })
}

function planKey(actorId: string, requestId: string): string {
    return `supplier-portal-review-mapping:v1:${encodeURIComponent(actorId)}:${encodeURIComponent(requestId)}`
}

export function readMappingPlan(
    actorId: string,
    requestId: string,
): { plan: ReviewMappingPlan | null; issue: string | null } {
    if (!actorId || !requestId || typeof window === "undefined")
        return { plan: null, issue: null }
    try {
        const text = sessionStorage.getItem(planKey(actorId, requestId))
        if (!text) return { plan: null, issue: null }
        const result = planSchema.safeParse(JSON.parse(text))
        if (
            !result.success ||
            result.data.actorId !== actorId ||
            result.data.targetRequestId !== requestId
        )
            return {
                plan: null,
                issue: "已保存的映射方案格式或账号不匹配，请重新核对。",
            }
        return { plan: result.data, issue: null }
    } catch {
        return { plan: null, issue: "无法读取已保存的映射方案，请重新核对。" }
    }
}

export function saveMappingPlan(plan: ReviewMappingPlan): void {
    sessionStorage.setItem(
        planKey(plan.actorId, plan.targetRequestId),
        JSON.stringify(planSchema.parse(plan)),
    )
}

export function discardMappingPlan(actorId: string, requestId: string): void {
    sessionStorage.removeItem(planKey(actorId, requestId))
}

export async function mappingPlanIssue(
    plan: ReviewMappingPlan,
    actorId: string,
    application: PortalApplication,
    dictionaries: ReviewDictionaries,
): Promise<string | null> {
    const input = reviewProduct(application)
    if (
        !input ||
        application.status !== "pending" ||
        plan.actorId !== actorId ||
        plan.targetRequestId !== application.id ||
        plan.targetVersion !== application.version ||
        plan.supplierId !== application.supplier_id ||
        plan.targetInputIdentity !== (await reviewInputIdentity(input))
    )
        return "申请或供应商原稿已变化，已保存的映射不可采用，请逐项重新核对。"
    return confirmedMappingIssue(input, plan.mapping, dictionaries)
}
