import type { CustomerDirectoryQuery } from "../types"
import { transferCsv } from "@/lib/data-transfer"
export const exportCustomerDirectory = async (
    query: CustomerDirectoryQuery,
) => {
    const result = await transferCsv("customers", {
        scope: query.scope,
        scope_version: query.scopeVersion,
        owner_user_ids: query.ownerUserIds || undefined,
        org_unit_ids: query.orgUnitIds || undefined,
        include_descendants:
            query.orgUnitIds && query.includeDescendants ? true : undefined,
        keyword: query.query?.trim() || undefined,
        status: query.status === "all" ? undefined : query.status,
        sort_by: "updated_at",
        sort_dir: query.sortDir === "asc" ? "asc" : "desc",
    })
    return result.content
}
