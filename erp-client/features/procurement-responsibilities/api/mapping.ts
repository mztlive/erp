import {
    PRODUCT_KIND_VALUES,
    type ProductKind,
} from "@/features/master-data/types"
import type {
    ProcurementResponsibilityRule,
    ProcurementResponsibilityRuleType,
    SaveProcurementResponsibilityRuleInput,
} from "@/features/procurement-responsibilities/types"
import type { BackendProcurementResponsibilityRule } from "@/features/procurement-responsibilities/api/wire-types"

const RULE_TYPES = new Set<ProcurementResponsibilityRuleType>([
    "SKU",
    "CATEGORY_SERVICE_REGION",
    "CATEGORY",
    "PRODUCT_KIND",
    "DEFAULT_DISPATCHER",
])

function ruleType(value: string): ProcurementResponsibilityRuleType {
    return RULE_TYPES.has(value as ProcurementResponsibilityRuleType)
        ? (value as ProcurementResponsibilityRuleType)
        : "DEFAULT_DISPATCHER"
}

function productKind(value?: string | null): ProductKind | undefined {
    return PRODUCT_KIND_VALUES.includes(value as ProductKind)
        ? (value as ProductKind)
        : undefined
}

function isEnabledStatus(status: string): boolean {
    const normalized = status.trim().toUpperCase()
    return normalized === "ENABLED" || normalized === "ACTIVE"
}

export function mapProcurementResponsibilityRule(
    rule: BackendProcurementResponsibilityRule,
): ProcurementResponsibilityRule {
    const skuLabel = [rule.sku_no, rule.sku_name].filter(Boolean).join(" · ")
    return {
        ruleId: rule.id,
        ruleType: ruleType(rule.rule_type),
        skuId: rule.sku_id ?? undefined,
        skuLabel: skuLabel || undefined,
        categoryId: rule.category_id ?? undefined,
        categoryLabel: rule.category_name ?? undefined,
        serviceRegion: rule.service_region?.trim() || undefined,
        productKind: productKind(rule.product_kind),
        ownerUserId: rule.owner_user_id,
        ownerName: rule.owner_name?.trim() || "负责人待确认",
        enabled: isEnabledStatus(rule.status),
        version: rule.version ?? 1,
    }
}

export function mapSaveProcurementResponsibilityRule(
    input: SaveProcurementResponsibilityRuleInput,
) {
    return {
        rule_type: input.ruleType,
        sku_id: input.ruleType === "SKU" ? input.skuId : undefined,
        category_id:
            input.ruleType === "CATEGORY" ||
            input.ruleType === "CATEGORY_SERVICE_REGION"
                ? input.categoryId
                : undefined,
        service_region:
            input.ruleType === "CATEGORY_SERVICE_REGION"
                ? input.serviceRegion?.trim()
                : undefined,
        product_kind:
            input.ruleType === "PRODUCT_KIND" ? input.productKind : undefined,
        owner_user_id: input.ownerUserId,
        status: input.enabled ? "ENABLED" : "DISABLED",
        expected_version: input.expectedVersion,
    }
}
