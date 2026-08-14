import { apiGet, type Page } from "@/lib/api"

import {
    asContractStatus,
    baseActions,
    isExpiringWithin30Days,
    loadCustomerBrief,
    loadPartyName,
    tsToIso,
} from "@/features/contracts/api/helpers"
import type {
    BackendContractDetail,
    BackendContractRevision,
    BackendContractView,
} from "@/features/contracts/api/wire-types"
import type { ContractListRow } from "@/features/contracts/types"
import {
    CONTRACT_STATUS_LABEL,
    CONTRACT_STATUS_TONE,
} from "@/features/contracts/types"

function mapListRow(
    row: BackendContractView,
    revision: BackendContractRevision | null,
    customer: Awaited<ReturnType<typeof loadCustomerBrief>>,
    settlementName: string,
): ContractListRow {
    const status = asContractStatus(String(row.status))
    const actions = baseActions(status)
    const validFrom =
        revision?.valid_from ?? tsToIso(row.created_at).slice(0, 10)
    const validTo = revision?.valid_to ?? "9999-12-31"

    return {
        contractId: row.id,
        contractNo: row.contract_no,
        customer: {
            customerId: row.customer_id,
            customerNo: customer?.customerNo ?? row.customer_id,
            displayName:
                revision?.customer_name ??
                customer?.displayName ??
                row.customer_id,
        },
        settlementParty: {
            partyId: row.settlement_party_id,
            displayName:
                revision?.settlement_party_name ??
                settlementName ??
                row.settlement_party_id,
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
        ownerLabel: customer?.ownerLabel ?? "—",
        ownerKind: "current_customer_owner",
        allowedActions: actions.allowedActions,
        actionBlockers: actions.actionBlockers,
    }
}

/**
 * 合同列表（全量拉取一页大容量；页面本地筛选/指标仍可用 filter-contracts）。
 */
export async function fetchContracts(): Promise<ContractListRow[]> {
    const page = await apiGet<Page<BackendContractView>>("/admin/contracts", {
        page: 1,
        page_size: 100,
        sort_by: "created_at",
        sort_dir: "desc",
    })

    const rows = await Promise.all(
        page.items.map(async (row) => {
            let revision: BackendContractRevision | null = null
            try {
                const detail = await apiGet<BackendContractDetail>(
                    `/admin/contracts/${row.id}`,
                )
                revision =
                    detail.revisions.find(
                        (r) => r.id === detail.current_revision_id,
                    ) ??
                    detail.revisions[0] ??
                    null
            } catch {
                revision = null
            }
            const customer = await loadCustomerBrief(row.customer_id)
            const settlementName =
                revision?.settlement_party_name ??
                (await loadPartyName(row.settlement_party_id))
            return mapListRow(row, revision, customer, settlementName)
        }),
    )

    return rows
}
