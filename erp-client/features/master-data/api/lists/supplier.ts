/** 供应商列表适配：能力 / 资质 / 资质健康度筛选。 */

import type { SupplierDto } from "@/features/master-data/api/contracts"
import { mapSupplierRow } from "@/features/master-data/api/list-mappers"
import type {
    MasterDataListItem,
    MasterDataListQuery,
} from "@/features/master-data/types"
import { fetchAllPages } from "./fetch-all"

export async function listSuppliers(
    query: MasterDataListQuery,
): Promise<MasterDataListItem[]> {
    const status =
        query.lifecycleStatus === "enabled"
            ? "active"
            : query.lifecycleStatus === "disabled"
              ? "disabled"
              : undefined
    const suppliers = await fetchAllPages<SupplierDto>("/admin/suppliers", {
        status,
        keyword: query.q || undefined,
        capability_codes: joinFilterCodes(query.supplierCapabilityCodes),
        qualification_types: joinFilterCodes(query.supplierQualificationTypes),
        qualification_health: query.supplierQualificationHealth,
    })
    return suppliers.map((supplier) => mapSupplierRow(supplier))
}

/** 规范化多选条件，供后端以逗号分隔的稳定查询参数接收。 */
export function joinFilterCodes(
    values: readonly string[] | undefined,
): string | undefined {
    if (!values?.length) return undefined
    return [...new Set(values.map((value) => value.trim()).filter(Boolean))]
        .sort()
        .join(",")
}
