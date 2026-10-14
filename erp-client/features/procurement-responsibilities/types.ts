import type { ProductKind } from "@/features/master-data/types"

export type ProcurementResponsibilityRuleType =
    | "SKU"
    | "CATEGORY"
    | "PRODUCT_KIND"
    | "DEFAULT_DISPATCHER"

export type ProcurementResponsibilityRule = Readonly<{
    ruleId: string
    ruleType: ProcurementResponsibilityRuleType
    skuId?: string
    skuLabel?: string
    categoryId?: string
    categoryLabel?: string
    productKind?: ProductKind
    ownerUserId: string
    ownerName: string
    enabled: boolean
    version: number
}>

export type SaveProcurementResponsibilityRuleInput = {
    ruleId?: string
    ruleType: ProcurementResponsibilityRuleType
    skuId?: string
    categoryId?: string
    productKind?: ProductKind
    ownerUserId: string
    enabled: boolean
    expectedVersion?: number
}

export const PROCUREMENT_RESPONSIBILITY_RULE_TYPE_LABEL: Readonly<
    Record<ProcurementResponsibilityRuleType, string>
> = {
    SKU: "SKU",
    CATEGORY: "分类",
    PRODUCT_KIND: "商品类型",
    DEFAULT_DISPATCHER: "默认调度人",
}
