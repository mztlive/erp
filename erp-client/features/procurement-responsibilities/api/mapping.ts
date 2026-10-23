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
        productKind: rule.product_kind ?? undefined,
        ownerUserIds: rule.owner_user_ids,
        ownerNames: rule.owner_user_ids.map(
            (_, index) => rule.owner_names[index]?.trim() || "负责人待确认",
        ),
        strategy: rule.strategy,
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
            input.ruleType === "CATEGORY" ? input.categoryId : undefined,
        product_kind:
            input.ruleType === "PRODUCT_KIND" ? input.productKind : undefined,
        owner_user_ids: input.ownerUserIds,
        strategy: input.strategy,
        status: input.enabled ? "active" : "disabled",
        version: input.expectedVersion,
    }
}
