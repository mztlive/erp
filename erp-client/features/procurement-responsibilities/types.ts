import type { AssignmentStrategy } from "@/components/business/responsibility-pool/model"
import type { ProductKind } from "@/features/master-data/types"

export type ProcurementResponsibilityRuleType =
    | "INQUIRY"
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
    ownerUserIds: readonly string[]
    ownerNames: readonly string[]
    strategy: AssignmentStrategy
    enabled: boolean
    version: number
}>

export type SaveProcurementResponsibilityRuleInput = {
    ruleId?: string
    ruleType: ProcurementResponsibilityRuleType
    skuId?: string
    categoryId?: string
    productKind?: ProductKind
    ownerUserIds: string[]
    strategy: AssignmentStrategy
    enabled: boolean
    expectedVersion?: number
}

export const PROCUREMENT_RESPONSIBILITY_RULE_TYPE_LABEL: Readonly<
    Record<ProcurementResponsibilityRuleType, string>
> = {
    INQUIRY: "询价",
    SKU: "SKU",
    CATEGORY: "分类",
    PRODUCT_KIND: "商品类型",
    DEFAULT_DISPATCHER: "默认负责人池",
}
