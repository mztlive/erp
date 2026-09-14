import { apiGet } from "@/lib/api"
import type { Page } from "@/lib/api"
import type {
    CustomerDirectoryQuery,
    CustomerDirectoryResult,
} from "@/features/customers/types"
import { mapDirectoryItem } from "./mappers"
import type { BackendCustomerView } from "./wire-types"

/**
 * 查询客户目录。范围、过滤、排序和分页均由服务端执行。
 */
export async function fetchCustomerDirectory(
    query: CustomerDirectoryQuery,
): Promise<CustomerDirectoryResult> {
    const status = query.status === "all" ? undefined : query.status
    const path =
        query.scope === "all_authorized"
            ? "/admin/customers/all-authorized"
            : "/admin/customers"
    const page = await apiGet<
        Page<BackendCustomerView> & {
            owner_options: { value: string; label: string }[]
            empty_reason?: string | null
            scope_version: string
            policy_version: number
            organization_version: number
            scope_summary: string
            as_of: string
            ownership_basis: string
        }
    >(path, {
        scope: query.scope === "all_authorized" ? undefined : query.scope,
        scope_version: query.scopeVersion,
        owner_user_ids: query.ownerUserIds || undefined,
        org_unit_ids: query.orgUnitIds || undefined,
        include_descendants:
            query.orgUnitIds && query.includeDescendants ? true : undefined,
        keyword: query.query?.trim() || undefined,
        status,
        page: query.page,
        page_size: query.pageSize,
        sort_by: "updated_at",
        sort_dir: query.sortDir === "asc" ? "asc" : "desc",
    })
    return {
        hasCustomerScope: page.empty_reason !== "no_scope",
        emptyReason: page.empty_reason,
        scopeVersion: page.scope_version,
        policyVersion: page.policy_version,
        organizationVersion: page.organization_version,
        scopeSummary: page.scope_summary,
        asOf: page.as_of,
        ownershipBasis: page.ownership_basis,
        ownerOptions: page.owner_options ?? [],
        items: page.items.map(mapDirectoryItem),
        totalInScope: page.total,
        page: page.page,
        pageSize: page.page_size,
        queriedAt: page.as_of,
    }
}
