import type { ProductKind } from "@/features/master-data/types"
import type { ProcurementResponsibilityRuleType } from "@/features/procurement-responsibilities/types"

export type BackendProcurementResponsibilityRule = {
    id: string
    rule_type: ProcurementResponsibilityRuleType | string
    sku_id?: string | null
    sku_no?: string | null
    sku_name?: string | null
    category_id?: string | null
    category_name?: string | null
    service_region?: string | null
    product_kind?: ProductKind | string | null
    owner_user_id: string
    owner_name?: string | null
    status: "active" | "disabled" | string
    version?: number
}

export type BackendProcurementResponsibilityRuleList =
    | BackendProcurementResponsibilityRule[]
    | {
          items: BackendProcurementResponsibilityRule[]
          total: number
          page: number
          page_size: number
      }

export type BackendSaveProcurementResponsibilityRule =
    BackendProcurementResponsibilityRule
