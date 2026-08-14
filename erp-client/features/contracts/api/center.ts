import { apiGet } from "@/lib/api"
import { paymentTermLabel } from "@/lib/business-options"

import {
    asContractStatus,
    baseActions,
    isApiError,
    loadCustomerBrief,
    loadFileAsset,
    mapScanStatus,
    paymentTermDays,
    tsToIso,
} from "@/features/contracts/api/helpers"
import type { BackendContractDetail } from "@/features/contracts/api/wire-types"
import type {
    ContractAttachmentView,
    ContractCenterView,
    ContractRevisionSummary,
} from "@/features/contracts/types"
import {
    CONTRACT_STATUS_LABEL,
    CONTRACT_STATUS_TONE,
} from "@/features/contracts/types"

/**
 * 合同对象中心。
 */
export async function fetchContractCenter(
    contractId: string,
): Promise<ContractCenterView | null> {
    if (!contractId) return null

    let detail: BackendContractDetail
    try {
        detail = await apiGet<BackendContractDetail>(
            `/admin/contracts/${contractId}`,
        )
    } catch (error) {
        if (
            isApiError(error) &&
            (error.status === 404 || error.status === 403)
        ) {
            return null
        }
        throw error
    }

    const status = asContractStatus(String(detail.status))
    const actions = baseActions(status)
    const current =
        detail.revisions.find((r) => r.id === detail.current_revision_id) ??
        detail.revisions[0]
    const customer = await loadCustomerBrief(detail.customer_id)

    const attachments: ContractAttachmentView[] = []
    for (const rev of detail.revisions) {
        const file = await loadFileAsset(rev.contract_pdf_file_id)
        if (!file) {
            attachments.push({
                id: rev.contract_pdf_file_id,
                name: `${detail.contract_no}-r${rev.revision_no}.pdf`,
                contentType: "application/pdf",
                revisionNo: rev.revision_no,
                uploadedBy: "—",
                uploadedAt: tsToIso(rev.created_at),
                securityState: "processing",
                canDownload: false,
            })
            continue
        }
        const securityState = mapScanStatus(file.security_scan_status)
        attachments.push({
            id: file.id,
            name: file.file_name,
            contentType: file.content_type,
            revisionNo: rev.revision_no,
            uploadedBy: file.created_by,
            uploadedAt: tsToIso(file.created_at),
            securityState,
            canDownload: true,
        })
    }

    const revisionTimeline: ContractRevisionSummary[] = detail.revisions.map(
        (r) => ({
            revisionId: r.id,
            revisionNo: r.revision_no,
            validFrom: r.valid_from,
            validTo: r.valid_to ?? "9999-12-31",
            changeReason: r.archive_source,
            effectiveAt: tsToIso(r.created_at),
            isCurrent:
                r.id === detail.current_revision_id ||
                r === detail.revisions[0],
        }),
    )

    const nowIso = new Date().toISOString()
    const termCode = current?.payment_term_code ?? "CONTRACT"
    const termName =
        current?.payment_term_name ?? paymentTermLabel(termCode) ?? termCode

    return {
        contractId: detail.id,
        contractNo: detail.contract_no,
        status,
        statusLabel: CONTRACT_STATUS_LABEL[status],
        statusTone: CONTRACT_STATUS_TONE[status],
        lockVersion: detail.version,
        customer: {
            id: detail.customer_id,
            displayName:
                current?.customer_name ??
                customer?.displayName ??
                detail.customer_id,
            reference: customer?.customerNo,
        },
        ownerLabel: customer?.ownerLabel ?? "—",
        ownerKind: "current_customer_owner",
        currentRevision: {
            revisionId: current?.id ?? detail.current_revision_id ?? detail.id,
            revisionNo: current?.revision_no ?? 1,
            settlementParty: {
                id: detail.settlement_party_id,
                displayName:
                    current?.settlement_party_name ??
                    detail.settlement_party_id,
            },
            paymentTermSnapshot: {
                label: termName,
                days: paymentTermDays(termCode),
                description: termName,
            },
            invoiceRequirementSnapshot: {
                titleType: current?.invoice_type ?? "—",
                contentSummary: current ? `税点 ${current.tax_point}` : "—",
            },
            validFrom:
                current?.valid_from ?? tsToIso(detail.created_at).slice(0, 10),
            validTo: current?.valid_to ?? "9999-12-31",
            signedAt: current?.signed_at,
            effectiveAt: current ? tsToIso(current.created_at) : undefined,
            termsSummary: termName,
        },
        attachments,
        relatedSalesOrders: [],
        revisionTimeline,
        auditTimeline: [],
        allowedActions: actions.allowedActions,
        actionBlockers: actions.actionBlockers,
        sourceAsOf: nowIso,
        relatedSalesOrdersAsOf: nowIso,
        queriedAt: nowIso,
        selectableForNewSalesOrder: actions.selectable,
        selectableBlocker: actions.selectableBlocker,
    }
}
