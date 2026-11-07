import type { MasterDataListQuery } from "../types"
import { sellableListParams } from "./lists/sellable"
import { productListParams, supplierListParams } from "./lists/query-params"

export function masterTransferQuery(query: MasterDataListQuery) {
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
            ...productListParams(query),
            _filters: filters,
        }
    if (query.resource === "suppliers")
        return {
            resource: "supplier",
            ...supplierListParams(query),
            _filters: filters,
        }
    return null
}
