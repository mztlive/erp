import { z } from "zod"
import type { PortalDuplicate } from "../../api"
import type {
    NewProductInput,
    PortalApplication,
    PortalDictionary,
} from "@/features/supplier-portal/types"
import { toAutomationIdSegment } from "@/lib/automation-id"

export type ReviewSkuMappingRow = {
    rowId: string
    unitId: string
    unitVersion: number
    target: string
    unitSynonymConfirmed: boolean
    unitSynonymReason: string
}

export type ReviewFormValues = {
    internalNote: string
    returnReason: string
    maintainerUserId: string
    productTarget: string
    brandId: string
    categoryId: string
    brandVersion: number
    categoryVersion: number
    categoryHierarchy: NonNullable<PortalDictionary["hierarchy"]>
    existingConfirmed: string[]
    mappings: ReviewSkuMappingRow[]
}

export type ReviewSectionId =
    | "basic"
    | "brand-category"
    | "sku-mapping"
    | "decision"

export type ReviewSectionIssue = {
    sectionId: ReviewSectionId
    fieldId: string
    message: string
}

export type ReviewSectionSummary = {
    id: ReviewSectionId
    step: number
    title: string
    shortTitle: string
    subtitle: string
    completedCount: number
    totalCount: number
    issues: ReviewSectionIssue[]
    isComplete: boolean
}

export type CreateDictionaryRequest = {
    kind: "brand" | "category" | "unit"
    rawName: string
    rowId?: string
}

export type SkuUnitPreviousState = {
    unitId: string
    unitVersion: number
    unitSynonymConfirmed: boolean
    unitSynonymReason: string
}

export type BatchUnitRowRecord = {
    unitId: string
    unitName: string
    overwritten: boolean
    previous: SkuUnitPreviousState
}

export type BatchUnitApplySnapshot = {
    unitId: string
    unitName: string
    scope: "unmapped" | "all"
    appliedRowIds: string[]
    overwrittenRowIds: string[]
    synonymRequiredRowIds: string[]
    skippedLockedCount: number
    previousRows: Record<string, SkuUnitPreviousState>
}

export type ExistingOfferingCandidate = {
    row_id: string
    offering_id: string
    expected_offering_version: number
    expected_revision_no: number
    sku_id?: string
    supplier_sku_code?: string
}

export const PRODUCT_KIND_LABELS: Record<string, string> = {
    PHYSICAL: "实物",
    VIRTUAL: "虚拟商品",
    OFFLINE_SERVICE: "线下服务",
    VOUCHER: "卡券",
}

export const reviewFormSchema = z.object({
    internalNote: z.string().max(500, "内部核对备注最多500字"),
    returnReason: z.string().max(500, "退回原因最多500字"),
    maintainerUserId: z.string(),
    productTarget: z.string(),
    brandId: z.string(),
    categoryId: z.string(),
    brandVersion: z.number().int(),
    categoryVersion: z.number().int(),
    categoryHierarchy: z.custom<NonNullable<PortalDictionary["hierarchy"]>>(),
    existingConfirmed: z.array(z.string()),
    mappings: z.array(
        z.object({
            rowId: z.string(),
            unitId: z.string(),
            unitVersion: z.number().int(),
            target: z.string(),
            unitSynonymConfirmed: z.boolean(),
            unitSynonymReason: z.string(),
        }),
    ),
})

export const emptyReviewFormValues: ReviewFormValues = {
    internalNote: "",
    returnReason: "",
    maintainerUserId: "",
    productTarget: "unconfirmed",
    brandId: "",
    categoryId: "",
    brandVersion: 0,
    categoryVersion: 0,
    categoryHierarchy: [],
    existingConfirmed: [],
    mappings: [],
}

export const reviewFormBaseOptions = {
    defaultValues: emptyReviewFormValues,
    validators: {
        onSubmit: reviewFormSchema,
    },
}

export function createReviewDefaultValues(
    rawProduct: NewProductInput | null,
): ReviewFormValues {
    return {
        internalNote: "",
        returnReason: "",
        maintainerUserId: "",
        productTarget: "unconfirmed",
        brandId: rawProduct?.brand.selected_id ?? "",
        categoryId: rawProduct?.category.selected_id ?? "",
        brandVersion: rawProduct?.brand.expected_version ?? 0,
        categoryVersion: rawProduct?.category.expected_version ?? 0,
        categoryHierarchy: [],
        existingConfirmed: [],
        mappings:
            rawProduct?.skus.map((sku) => ({
                rowId: sku.row_id,
                unitId: sku.unit.selected_id ?? "",
                unitVersion: sku.unit.expected_version ?? 0,
                target: "unconfirmed",
                unitSynonymConfirmed: false,
                unitSynonymReason: "",
            })) ?? [],
    }
}

export function dictionaryOptions(
    rows: { id: string; name: string; path?: string }[] | undefined,
) {
    return (rows ?? []).map((item) => ({
        value: item.id,
        label: item.path ?? item.name,
    }))
}

export function fieldHighlightClass(
    highlightedFieldId: string | null,
    fieldId: string,
) {
    return highlightedFieldId === fieldId
        ? "rounded-md ring-2 ring-destructive/65 ring-offset-2 p-1.5 -m-1.5 transition-shadow"
        : undefined
}

export function fieldIdToSectionId(fieldId: string): ReviewSectionId {
    if (
        fieldId === "supplier-portal-review-product-target" ||
        fieldId === "supplier-portal-review-duplicate-query" ||
        fieldId === "supplier-portal-review-duplicate-search"
    ) {
        return "basic"
    }
    if (
        fieldId === "supplier-portal-review-brand" ||
        fieldId === "supplier-portal-review-category" ||
        fieldId === "supplier-portal-review-category-path-confirm" ||
        fieldId === "supplier-portal-review-create-brand" ||
        fieldId === "supplier-portal-review-create-category"
    ) {
        return "brand-category"
    }
    if (
        fieldId.startsWith("supplier-portal-review-sku-") ||
        fieldId.startsWith("supplier-portal-review-existing-") ||
        fieldId.startsWith("supplier-portal-review-default-unit") ||
        fieldId.startsWith("supplier-portal-review-unit-bulk")
    ) {
        return "sku-mapping"
    }
    return "decision"
}

export function evaluateSkuRowStatus({
    sku,
    row,
    units,
    selectedProduct,
    existingCandidates,
}: {
    sku: NewProductInput["skus"][number]
    row: ReviewSkuMappingRow | undefined
    units: PortalDictionary[] | undefined
    selectedProduct: PortalDuplicate | undefined
    existingCandidates: readonly ExistingOfferingCandidate[]
}): {
    ready: boolean
    issues: ReviewSectionIssue[]
    needsSynonymConfirmation: boolean
    matchedUnit: PortalDictionary | undefined
} {
    const prefix = `supplier-portal-review-sku-${toAutomationIdSegment(sku.row_id)}`
    const issues: ReviewSectionIssue[] = []
    const unit = row ? units?.find((item) => item.id === row.unitId) : undefined
    const needsSynonymConfirmation = Boolean(
        unit && sku.unit.raw_name.trim() !== unit.name.trim(),
    )

    if (!row || !unit) {
        issues.push({
            sectionId: "sku-mapping",
            fieldId: `${prefix}-unit`,
            message: `${sku.name}：请完成有效基础单位匹配`,
        })
    } else if (row.unitSynonymConfirmed && !row.unitSynonymReason.trim()) {
        issues.push({
            sectionId: "sku-mapping",
            fieldId: `${prefix}-unit-synonym-reason`,
            message: `${sku.name}：确认单位同义映射时必须填写逐行核对依据`,
        })
    } else if (needsSynonymConfirmation && !row.unitSynonymConfirmed) {
        issues.push({
            sectionId: "sku-mapping",
            fieldId: `${prefix}-unit-synonym-confirmed`,
            message: `${sku.name}：单位原文与标准单位不同，须确认同义含义或退回供应商`,
        })
    }

    if (!row || row.target === "unconfirmed") {
        issues.push({
            sectionId: "sku-mapping",
            fieldId: `${prefix}-target`,
            message: `${sku.name}：请选择新建或复用规格`,
        })
    } else {
        const matchedSku = selectedProduct?.skus.find(
            (item) => item.sku_id === row.target,
        )
        if (row.target !== "new" && !matchedSku) {
            issues.push({
                sectionId: "sku-mapping",
                fieldId: `${prefix}-target`,
                message: `${sku.name}：所选规格必须属于明确匹配的商品`,
            })
        } else {
            const conflictingCandidate = existingCandidates.find(
                (candidate) =>
                    candidate.row_id === sku.row_id &&
                    candidate.sku_id &&
                    row.target !== candidate.sku_id,
            )
            if (conflictingCandidate) {
                issues.push({
                    sectionId: "sku-mapping",
                    fieldId: `${prefix}-target`,
                    message: `${sku.name}：相同订货编码已关联已有规格，请复用该精确规格`,
                })
            }
        }
    }

    return {
        ready: issues.length === 0,
        issues,
        needsSynonymConfirmation,
        matchedUnit: unit,
    }
}

export function evaluateReviewSections({
    values,
    rawProduct,
    brands,
    categories,
    units,
    duplicates,
    existingCandidates,
    initialAvailability,
    reportedConfirmed,
    canListProcurementPeople,
}: {
    values: ReviewFormValues
    rawProduct: NewProductInput | null
    brands: PortalDictionary[] | undefined
    categories: PortalDictionary[] | undefined
    units: PortalDictionary[] | undefined
    duplicates: PortalDuplicate[] | undefined
    existingCandidates: readonly ExistingOfferingCandidate[]
    initialAvailability: boolean
    reportedConfirmed: boolean
    canListProcurementPeople: boolean
}): ReviewSectionSummary[] {
    const sections: ReviewSectionSummary[] = []
    const selectedProduct = duplicates?.find(
        (item) => item.product_id === values.productTarget,
    )

    // 1. 基本信息与查重
    if (rawProduct) {
        const basicIssues: ReviewSectionIssue[] = []
        let basicCompleted = 0
        if (values.productTarget === "unconfirmed") {
            basicIssues.push({
                sectionId: "basic",
                fieldId: "supplier-portal-review-product-target",
                message: "请明确选择新建或复用公司商品",
            })
        } else if (values.productTarget !== "new" && !selectedProduct) {
            basicIssues.push({
                sectionId: "basic",
                fieldId: "supplier-portal-review-product-target",
                message: "所选商品候选已不可用，请重新核对",
            })
        } else {
            basicCompleted = 1
        }
        sections.push({
            id: "basic",
            step: 1,
            title: "基本信息与查重",
            shortTitle: "基本与查重",
            subtitle: "核对提报概览并决定新建或复用公司商品",
            completedCount: basicCompleted,
            totalCount: 1,
            issues: basicIssues,
            isComplete: basicIssues.length === 0,
        })

        // 2. 品牌与类目归属
        const brandCategoryIssues: ReviewSectionIssue[] = []
        let brandCategoryCompleted = 0
        const brand = brands?.find((item) => item.id === values.brandId)
        if (!brand) {
            brandCategoryIssues.push({
                sectionId: "brand-category",
                fieldId: "supplier-portal-review-brand",
                message: "品牌尚未完成有效匹配，不能通过",
            })
        } else {
            brandCategoryCompleted += 1
        }

        const category = categories?.find(
            (item) => item.id === values.categoryId,
        )
        if (!category) {
            brandCategoryIssues.push({
                sectionId: "brand-category",
                fieldId: "supplier-portal-review-category",
                message: "分类尚未完成有效匹配，不能通过",
            })
        } else {
            brandCategoryCompleted += 1
        }

        const hierarchyConfirmed = Boolean(
            category &&
            values.categoryHierarchy.length > 0 &&
            JSON.stringify(values.categoryHierarchy) ===
                JSON.stringify(category.hierarchy),
        )
        if (!hierarchyConfirmed) {
            brandCategoryIssues.push({
                sectionId: "brand-category",
                fieldId: "supplier-portal-review-category-path-confirm",
                message: "请读取并明确确认当前完整分类路径后再通过",
            })
        } else {
            brandCategoryCompleted += 1
        }

        sections.push({
            id: "brand-category",
            step: 2,
            title: "品牌与类目归属",
            shortTitle: "品牌与类目",
            subtitle: "匹配标准品牌、分类并确认完整类目层级",
            completedCount: brandCategoryCompleted,
            totalCount: 3,
            issues: brandCategoryIssues,
            isComplete: brandCategoryIssues.length === 0,
        })

        // 3. SKU 单位与规格映射
        const skuIssues: ReviewSectionIssue[] = []
        let skuCompleted = 0
        for (const sku of rawProduct.skus) {
            const row = values.mappings.find(
                (item) => item.rowId === sku.row_id,
            )
            const status = evaluateSkuRowStatus({
                sku,
                row,
                units,
                selectedProduct,
                existingCandidates,
            })
            if (status.ready) {
                skuCompleted += 1
            } else {
                skuIssues.push(...status.issues)
            }
        }
        for (const candidate of existingCandidates) {
            if (values.existingConfirmed.includes(candidate.offering_id)) {
                skuCompleted += 1
            } else {
                skuIssues.push({
                    sectionId: "sku-mapping",
                    fieldId: `supplier-portal-review-existing-${toAutomationIdSegment(candidate.offering_id)}`,
                    message: `订货编码 ${candidate.supplier_sku_code ?? "本次订货编码"}：请核对并确认既有供给修订`,
                })
            }
        }
        const skuTotal = rawProduct.skus.length + existingCandidates.length
        sections.push({
            id: "sku-mapping",
            step: 3,
            title: "SKU 单位与规格映射",
            shortTitle: "SKU 映射",
            subtitle: "逐行或批量匹配基础单位、同义说明与规格建档决定",
            completedCount: skuCompleted,
            totalCount: skuTotal,
            issues: skuIssues,
            isComplete: skuIssues.length === 0,
        })
    } else {
        sections.push({
            id: "basic",
            step: 1,
            title: "申请概览与核对说明",
            shortTitle: "申请概览",
            subtitle: "核对供应商提报概览与当前处理责任人",
            completedCount: 0,
            totalCount: 0,
            issues: [],
            isComplete: true,
        })
    }

    // 4. 审核结论与提交
    const decisionIssues: ReviewSectionIssue[] = []
    let decisionCompleted = 0
    let decisionTotal = 0

    if (initialAvailability) {
        decisionTotal += 1
        if (reportedConfirmed) {
            decisionCompleted += 1
        } else {
            decisionIssues.push({
                sectionId: "decision",
                fieldId: "supplier-portal-review-reported-confirmed",
                message: "请明确核对供应商实际报送时间后再通过",
            })
        }
    }

    decisionTotal += 1
    if (values.maintainerUserId) {
        decisionCompleted += 1
    } else {
        decisionIssues.push({
            sectionId: "decision",
            fieldId: canListProcurementPeople
                ? "supplier-portal-review-maintainer"
                : "supplier-portal-review-maintainer-self",
            message: "请明确选择建档或供给的内部维护人",
        })
    }

    if (rawProduct) {
        decisionTotal += 1
        if (!values.internalNote.trim()) {
            decisionIssues.push({
                sectionId: "decision",
                fieldId: "supplier-portal-review-comment",
                message: "请填写内部核对备注，保留本次复用或新建理由",
            })
        } else if (values.internalNote.length > 500) {
            decisionIssues.push({
                sectionId: "decision",
                fieldId: "supplier-portal-review-comment",
                message: "内部核对备注最多500字",
            })
        } else {
            decisionCompleted += 1
        }
    } else if (values.internalNote.length > 500) {
        decisionIssues.push({
            sectionId: "decision",
            fieldId: "supplier-portal-review-comment",
            message: "内部核对备注最多500字",
        })
    }

    if (values.returnReason.length > 500) {
        decisionIssues.push({
            sectionId: "decision",
            fieldId: "supplier-portal-review-return-reason",
            message: "退回原因最多500字",
        })
    }

    sections.push({
        id: "decision",
        step: rawProduct ? 4 : 2,
        title: "审核结论与提交",
        shortTitle: "审核结论",
        subtitle: "确认内部维护人、核对备注并提交通过或退回决定",
        completedCount: decisionCompleted,
        totalCount: decisionTotal,
        issues: decisionIssues,
        isComplete: decisionIssues.length === 0,
    })

    return sections
}

export function buildApproveReviewPayload({
    value,
    canProcess,
    canListProcurementPeople,
    application,
    taskId,
    taskVersion,
    rawProduct,
    brands,
    categories,
    units,
    duplicates,
    existingCandidates,
    initialAvailability,
    reportedConfirmed,
}: {
    value: ReviewFormValues
    canProcess: boolean
    canListProcurementPeople: boolean
    application: PortalApplication
    taskId: string
    taskVersion: string
    rawProduct: NewProductInput | null
    brands: PortalDictionary[] | undefined
    categories: PortalDictionary[] | undefined
    units: PortalDictionary[] | undefined
    duplicates: PortalDuplicate[] | undefined
    existingCandidates: readonly ExistingOfferingCandidate[]
    initialAvailability: boolean
    reportedConfirmed: boolean
}):
    | { ok: true; body: Record<string, unknown> }
    | { ok: false; message: string; fieldId: string } {
    if (!canProcess) {
        return {
            ok: false,
            message: "当前账号或任务不具备处理资格，请重新核对任务",
            fieldId: "supplier-portal-review-section",
        }
    }
    let normalizedProduct: Record<string, unknown> | null = null
    let existingProduct: Record<string, unknown> | null = null
    if (rawProduct) {
        const brand = brands?.find((item) => item.id === value.brandId)
        if (!brand) {
            return {
                ok: false,
                message: "品牌尚未完成有效匹配，不能通过",
                fieldId: "supplier-portal-review-brand",
            }
        }
        const category = categories?.find(
            (item) => item.id === value.categoryId,
        )
        if (!category) {
            return {
                ok: false,
                message: "分类尚未完成有效匹配，不能通过",
                fieldId: "supplier-portal-review-category",
            }
        }
        if (
            !value.categoryHierarchy.length ||
            JSON.stringify(value.categoryHierarchy) !==
                JSON.stringify(category.hierarchy)
        ) {
            return {
                ok: false,
                message: "请读取并明确确认当前完整分类路径后再通过",
                fieldId: "supplier-portal-review-category-path-confirm",
            }
        }
        if (value.productTarget === "unconfirmed") {
            return {
                ok: false,
                message: "请明确选择新建或复用公司商品",
                fieldId: "supplier-portal-review-product-target",
            }
        }
        const selectedProduct = duplicates?.find(
            (item) => item.product_id === value.productTarget,
        )
        if (value.productTarget !== "new" && !selectedProduct) {
            return {
                ok: false,
                message: "所选商品候选已不可用，请重新核对",
                fieldId: "supplier-portal-review-product-target",
            }
        }
        existingProduct = selectedProduct
            ? {
                  product_id: selectedProduct.product_id,
                  version: selectedProduct.version,
                  revision_id: selectedProduct.revision_id,
              }
            : null
        const mappings: Record<string, unknown>[] = []
        for (const row of value.mappings) {
            const prefix = `supplier-portal-review-sku-${toAutomationIdSegment(row.rowId)}`
            const sku = rawProduct.skus.find(
                (item) => item.row_id === row.rowId,
            )
            const unit = units?.find((item) => item.id === row.unitId)
            if (!sku || !unit) {
                return {
                    ok: false,
                    message: "每个规格必须完成有效基础单位匹配",
                    fieldId: `${prefix}-unit`,
                }
            }
            if (row.unitSynonymConfirmed && !row.unitSynonymReason.trim()) {
                return {
                    ok: false,
                    message: "确认单位同义映射时必须填写逐行核对依据",
                    fieldId: `${prefix}-unit-synonym-reason`,
                }
            }
            if (
                sku.unit.raw_name.trim() !== unit.name.trim() &&
                !row.unitSynonymConfirmed
            ) {
                return {
                    ok: false,
                    message:
                        "单位原文与标准单位不同，须明确确认同义含义或退回供应商",
                    fieldId: `${prefix}-unit-synonym-confirmed`,
                }
            }
            if (row.target === "unconfirmed") {
                return {
                    ok: false,
                    message: "请逐项明确选择新建或复用规格",
                    fieldId: `${prefix}-target`,
                }
            }
            const matchedSku = selectedProduct?.skus.find(
                (item) => item.sku_id === row.target,
            )
            if (row.target !== "new" && !matchedSku) {
                return {
                    ok: false,
                    message: "所选规格必须属于明确匹配的商品，请重新核对",
                    fieldId: `${prefix}-target`,
                }
            }
            mappings.push({
                row_id: row.rowId,
                name: sku.name.trim(),
                unit_id: unit.id,
                unit_version: sku.unit.selected_id
                    ? sku.unit.expected_version
                    : row.unitVersion,
                target_sku: matchedSku
                    ? {
                          sku_id: matchedSku.sku_id,
                          version: matchedSku.version,
                          revision_id: matchedSku.revision_id,
                      }
                    : null,
                unit_synonym_confirmation: row.unitSynonymConfirmed
                    ? {
                          original_unit: sku.unit.raw_name,
                          same_unit_meaning_confirmed: true,
                          reason: row.unitSynonymReason.trim(),
                      }
                    : null,
            })
        }
        for (const candidate of existingCandidates) {
            if (!value.existingConfirmed.includes(candidate.offering_id)) {
                return {
                    ok: false,
                    message: "请核对相同订货编码的既有供给，并明确确认本次修订",
                    fieldId: `supplier-portal-review-existing-${toAutomationIdSegment(candidate.offering_id)}`,
                }
            }
            const mapping = value.mappings.find(
                (row) => row.rowId === candidate.row_id,
            )
            if (mapping?.target !== candidate.sku_id) {
                return {
                    ok: false,
                    message:
                        "相同订货编码已关联已有规格，请复用该精确规格；含义变化时退回供应商核对",
                    fieldId: `supplier-portal-review-sku-${toAutomationIdSegment(candidate.row_id)}-target`,
                }
            }
        }
        normalizedProduct = {
            name: rawProduct.name.trim(),
            brand_id: brand.id,
            brand_version: rawProduct.brand.selected_id
                ? rawProduct.brand.expected_version
                : value.brandVersion,
            category_id: category.id,
            category_version: rawProduct.category.selected_id
                ? rawProduct.category.expected_version
                : value.categoryVersion,
            category_hierarchy: value.categoryHierarchy,
            sku_mappings: mappings,
        }
    }
    if (initialAvailability && !reportedConfirmed) {
        return {
            ok: false,
            message: "请明确核对供应商实际报送时间后再通过",
            fieldId: "supplier-portal-review-reported-confirmed",
        }
    }
    if (!value.maintainerUserId) {
        return {
            ok: false,
            message: "请明确选择建档或供给的内部维护人",
            fieldId: canListProcurementPeople
                ? "supplier-portal-review-maintainer"
                : "supplier-portal-review-maintainer-self",
        }
    }
    if (rawProduct && !value.internalNote.trim()) {
        return {
            ok: false,
            message: "请填写内部核对备注，保留本次复用或新建理由",
            fieldId: "supplier-portal-review-comment",
        }
    }
    if (value.internalNote.length > 500) {
        return {
            ok: false,
            message: "内部核对备注最多500字",
            fieldId: "supplier-portal-review-comment",
        }
    }
    return {
        ok: true,
        body: {
            availability_reported_at_confirmed:
                initialAvailability && reportedConfirmed,
            expected_version: application.version,
            work_item_id: taskId,
            work_item_version: Number(taskVersion),
            decision: "approve",
            comment: value.internalNote.trim() || null,
            maintainer_user_id: value.maintainerUserId || null,
            normalized_product: normalizedProduct,
            existing_product: existingProduct,
            existing_offerings:
                existingCandidates
                    .filter((item) =>
                        value.existingConfirmed.includes(item.offering_id),
                    )
                    .map((item) => ({
                        row_id: item.row_id,
                        offering_id: item.offering_id,
                        expected_offering_version:
                            item.expected_offering_version,
                        expected_revision_no: item.expected_revision_no,
                    })) ?? [],
        },
    }
}
