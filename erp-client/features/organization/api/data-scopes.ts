import { fetchCompleteList } from "@/lib/collect-pages"
import { apiDelete, apiPost } from "@/lib/api"
import type {
    CreateDataScopeInput,
    DataScopeListView,
    DataScopeRecord,
    DataScopeUrlState,
} from "@/features/organization/types"

type BackendDataScope = {
    id: string
    subject_type: DataScopeRecord["subjectType"]
    subject_id: string
    scope_type: DataScopeRecord["scopeType"]
    scope_targets: string[]
    schema_version: number
    resource: string
    actions: string[]
    target_dimension: DataScopeRecord["targetDimension"]
    target_mode?: DataScopeRecord["targetMode"] | null
    include_descendants?: boolean | null
    enabled: boolean
    version: number
    created_at: number
}

function mapRecord(row: BackendDataScope): DataScopeRecord {
    return {
        id: row.id,
        subjectType: row.subject_type,
        subjectId: row.subject_id,
        scopeType: row.scope_type,
        scopeTargets: row.scope_targets,
        schemaVersion: row.schema_version,
        resource: row.resource,
        actions: row.actions,
        targetDimension: row.target_dimension,
        targetMode: row.target_mode ?? null,
        includeDescendants: row.include_descendants ?? null,
        enabled: row.enabled,
        version: row.version,
        createdAt: row.created_at,
    }
}

export function dataScopeListQuery(url: DataScopeUrlState) {
    return {
        resource: url.resource,
        action: url.action,
        subject_type: url.subjectType === "all" ? undefined : url.subjectType,
        subject_id:
            url.subjectType === "all" || !url.subjectId
                ? undefined
                : url.subjectId,
        scope_type: url.scopeType === "all" ? undefined : url.scopeType,
    }
}

export async function fetchDataScopes(
    url: DataScopeUrlState,
): Promise<DataScopeListView> {
    const page = await fetchCompleteList<BackendDataScope>(
        "/admin/data-scopes",
        dataScopeListQuery(url),
    )
    const keyword = url.q?.trim().toLowerCase()
    const items = page.items.map(mapRecord).filter((row) => {
        if (!keyword) return true
        return [
            row.resource,
            row.actions.join(" "),
            row.subjectId,
            row.scopeType,
        ]
            .join(" ")
            .toLowerCase()
            .includes(keyword)
    })
    return {
        items,
        total: items.length,
        emptyReason: page.empty_reason === "no_scope" ? "no_scope" : null,
        scopeVersion: page.scope_version,
        policyVersion: page.policy_version,
        organizationVersion: page.organization_version,
        asOf: page.as_of,
        scopeSummary: page.scope_summary,
        ownershipBasis: page.ownership_basis,
    }
}

export async function createDataScope(input: CreateDataScopeInput) {
    return apiPost<BackendDataScope>("/admin/data-scopes", {
        schema_version: 2,
        resource: input.resource,
        actions: input.actions,
        target_dimension: input.targetDimension,
        target_mode: input.targetMode,
        include_descendants: input.includeDescendants,
        enabled: true,
        subject_type: input.subjectType,
        subject_id: input.subjectId,
        scope_type: input.scopeType,
        scope_targets: input.scopeTargets,
    })
}

export async function deleteDataScope(id: string) {
    await apiDelete<void>(`/admin/data-scopes/${id}`)
}
