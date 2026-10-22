import type { MasterDataListQuery } from "../types"
import { sellableListParams } from "./lists/sellable"
import { joinFilterCodes } from "./lists/supplier"
export function masterTransferQuery(query: MasterDataListQuery) {
    const status =
        query.lifecycleStatus === "enabled"
            ? "active"
            : query.lifecycleStatus === "disabled"
              ? "disabled"
              : undefined
    const filters = {
        revision_timing: query.revisionTiming,
        metric: query.metricKey,
    }
    if (query.resource === "sellable-items")
        return {
            resource: "sellable",
            ...sellableListParams(query),
            max_supplier_count:
                query.sellableSupplyPreset === "single-supplier"
                    ? 1
                    : undefined,
            _filters: filters,
        }
    if (query.resource === "products")
        return {
            resource: "product",
            status,
            keyword: query.q || undefined,
            product_kind: query.productKind,
            category_id: query.productCategoryId,
            brand_id: query.productBrandId,
            supplier_id: query.productSupplierId,
            listing_status: query.productListingStatus,
            supply_coverage: query.productSupplyCoverage,
            sales_price_min: query.productSalesPriceMin,
            sales_price_max: query.productSalesPriceMax,
            owner_user_ids: query.ownerUserIds || undefined,
            procurement_owner_user_ids:
                query.procurementOwnerUserIds || undefined,
            _filters: filters,
        }
    if (query.resource === "suppliers")
        return {
            resource: "supplier",
            status,
            keyword: query.q || undefined,
            capability_codes: joinFilterCodes(query.supplierCapabilityCodes),
            qualification_types: joinFilterCodes(
                query.supplierQualificationTypes,
            ),
            qualification_health: query.supplierQualificationHealth,
            owner_user_ids: query.owner_user_ids || undefined,
            capability_owner_user_ids:
                query.capability_owner_user_ids || undefined,
            org_unit_ids: query.org_unit_ids || undefined,
            include_descendants: query.include_descendants || undefined,
            _filters: filters,
        }
    return null
}
