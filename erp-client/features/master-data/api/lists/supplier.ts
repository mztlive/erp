/** 供应商列表适配：能力 / 资质 / 资质健康度筛选。 */

import { fetchCompleteList } from "@/lib/collect-pages"
import type { SupplierDto } from "@/features/master-data/api/contracts"
import { mapSupplierRow } from "@/features/master-data/api/list-mappers"
import type {
    MasterDataListQuery,
    MasterDataListResult,
} from "@/features/master-data/types"

export async function listSuppliers(
    query: MasterDataListQuery,
): Promise<Pick<MasterDataListResult, "rows" | "emptyReason" | "ownerOptions" | "capabilityOwnerOptions">> {
    const status =
        query.lifecycleStatus === "enabled"
            ? "active"
            : query.lifecycleStatus === "disabled"
              ? "disabled"
              : undefined
    const result = await fetchCompleteList<SupplierDto>(
        "/admin/suppliers",
        {
            status,
            keyword: query.q || undefined,
            capability_codes: joinFilterCodes(query.supplierCapabilityCodes),
            qualification_types: joinFilterCodes(query.supplierQualificationTypes),
            qualification_health: query.supplierQualificationHealth,
            owner_user_ids: query.owner_user_ids || undefined,
            capability_owner_user_ids: query.capability_owner_user_ids || undefined,
            org_unit_ids: query.org_unit_ids || undefined,
            include_descendants: query.include_descendants || undefined,
        },
        (item) => item.id,
    )
    return {
        rows: result.items.map((supplier) => mapSupplierRow(supplier)),
        emptyReason: result.empty_reason === "no_scope" ? "no_scope" : null,
        ownerOptions: mapOptions(result.owner_options),
        capabilityOwnerOptions: mapOptions(result.capability_owner_options),
    }
}

function mapOptions(
    options: readonly { value: string; label: string }[] | undefined,
): { value: string; label: string }[] {
    return (options ?? []).map((option) => ({
        value: option.value,
        label: option.label,
    }))
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
