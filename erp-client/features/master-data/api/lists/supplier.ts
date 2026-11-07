/** 供应商列表适配：能力 / 资质 / 资质健康度筛选。 */

import { fetchCompleteList } from "@/lib/collect-pages"
import type { SupplierDto } from "@/features/master-data/api/contracts"
import { mapSupplierRow } from "@/features/master-data/api/list-mappers"
import type {
    MasterDataListQuery,
    MasterDataListResult,
} from "@/features/master-data/types"
import { supplierListParams } from "./query-params"

export { joinFilterCodes } from "./query-params"

export async function listSuppliers(
    query: MasterDataListQuery,
): Promise<Pick<MasterDataListResult, "rows" | "emptyReason">> {
    const result = await fetchCompleteList<SupplierDto>(
        "/admin/suppliers",
        supplierListParams(query),
        (item) => item.id,
    )
    return {
        rows: result.items.map((supplier) => mapSupplierRow(supplier)),
        emptyReason: result.empty_reason === "no_scope" ? "no_scope" : null,
    }
}
