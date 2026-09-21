import { apiGet, type Page } from "@/lib/api"

import {
    baseActions,
    isExpiringWithin30Days,
    tsToIso,
} from "@/features/contracts/api/helpers"
import type {
    BackendContractRevision,
    BackendContractView,
} from "@/features/contracts/api/wire-types"
import type { ContractListRow } from "@/features/contracts/types"
import {
    CONTRACT_STATUS_LABEL,
    CONTRACT_STATUS_TONE,
} from "@/features/contracts/types"

function mapListRow(row: BackendContractView): ContractListRow {
    const revision: BackendContractRevision | null =
        row.current_revision ?? null
    const status = row.status
    const actions = baseActions(status)
    const validFrom =
        revision?.valid_from ?? tsToIso(row.created_at).slice(0, 10)
    const validTo = revision?.valid_to ?? "9999-12-31"

    return {
        contractId: row.id,
        contractNo: row.contract_no,
        customer: {
            customerId: row.customer_id,
            customerNo: row.customer_no ?? row.customer_id,
            displayName: revision?.customer_name ?? row.customer_id,
        },
        settlementParty: {
            partyId: row.settlement_party_id,
            displayName:
                revision?.settlement_party_name ?? row.settlement_party_id,
        },
        status,
        statusLabel: CONTRACT_STATUS_LABEL[status],
        statusTone: CONTRACT_STATUS_TONE[status],
        revisionNo: revision?.revision_no ?? 1,
        signedAt: revision?.signed_at,
        validFrom,
        validTo,
        expiringWithin30Days: isExpiringWithin30Days(
            status,
            revision?.valid_to,
        ),
        salesOrderCount: 0,
        activeSalesOrderCount: 0,
        ownerLabel: row.owner_user_name?.trim() || "未指定",
        ownerKind: "current_customer_owner",
        allowedActions: actions.allowedActions,
        actionBlockers: actions.actionBlockers,
    }
}

/** 服务端完整结果集分页，指标与候选项来自当前可见合同范围。 */
export type ContractListData = {
    items: ContractListRow[]
    total: number
    metrics: import("../lib/filter-contracts").ContractMetrics
    settlementOptions: { value: string; label: string }[]
    ownerOptions: { value: string; label: string }[]
    emptyReason?: string | null
    scopeVersion: string
    policyVersion: number
    organizationVersion: number
    scopeSummary: string
    asOf: string
    ownershipBasis: string
}

export async function fetchContracts(
    query: import("../lib/contracts-url-state").ContractsUrlState,
): Promise<ContractListData> {
    const page = await apiGet<
        Page<BackendContractView> & {
            metrics: {
                all: number
                effective: number
                expiring_30d: number
                expired: number
                terminated: number
            }
            settlement_options: { value: string; label: string }[]
            owner_options: { value: string; label: string }[]
            empty_reason?: string | null
            scope_version: string
            policy_version: number
            organization_version: number
            scope_summary: string
            as_of: string
            ownership_basis: string
        }
    >("/admin/contracts", {
        q: query.q?.trim() || undefined,
        metric: query.metric,
        customer_id: query.customerId,
        settlement_party_id: query.settlementPartyId,
        owner_user_ids: query.ownerUserIds,
        org_unit_ids: query.orgUnitIds || undefined,
        include_descendants:
            query.orgUnitIds && query.includeDescendants ? true : undefined,
        scope_version: query.scopeVersion,
        page: query.page,
        page_size: query.pageSize,
        sort_by:
            query.sort === "contractNo"
                ? "contract_no"
                : query.sort || "expiry_priority",
        sort_dir: query.dir || "asc",
    })
    return {
        items: page.items.map(mapListRow),
        total: page.total,
        metrics: page.metrics,
        settlementOptions: page.settlement_options,
        ownerOptions: page.owner_options,
        emptyReason: page.empty_reason,
        scopeVersion: page.scope_version,
        policyVersion: page.policy_version,
        organizationVersion: page.organization_version,
        scopeSummary: page.scope_summary,
        asOf: page.as_of,
        ownershipBasis: page.ownership_basis,
    }
}
