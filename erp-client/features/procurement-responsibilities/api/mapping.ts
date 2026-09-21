import type {
    ProcurementResponsibilityRule,
    SaveProcurementResponsibilityRuleInput,
} from "@/features/procurement-responsibilities/types"
import type { BackendProcurementResponsibilityRule } from "@/features/procurement-responsibilities/api/wire-types"

export function mapProcurementResponsibilityRule(
    rule: BackendProcurementResponsibilityRule,
): ProcurementResponsibilityRule {
    const skuLabel = [rule.sku_no, rule.sku_name].filter(Boolean).join(" · ")
    return {
        ruleId: rule.id,
        ruleType: rule.rule_type,
        skuId: rule.sku_id ?? undefined,
        skuLabel: skuLabel || undefined,
        categoryId: rule.category_id ?? undefined,
        categoryLabel: rule.category_name ?? undefined,
        serviceRegion: rule.service_region?.trim() || undefined,
        productKind: rule.product_kind ?? undefined,
        ownerUserId: rule.owner_user_id,
        ownerName: rule.owner_name?.trim() || "负责人待确认",
        enabled: rule.status === "active",
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
        status: input.enabled ? "active" : "disabled",
        version: input.expectedVersion,
    }
}
