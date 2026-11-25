"use client"

import {
    DetailWorkspaceFact,
    DetailWorkspaceFacts,
    DetailWorkspaceSection,
} from "@/components/business"
import type { ContractCenterView } from "@/features/contracts/types"

/** 结算与开票分区：当前修订的结构化记录。 */
export function ContractDetailSettlement({
    contract,
}: {
    contract: ContractCenterView
}) {
    const rev = contract.currentRevision

    return (
        <DetailWorkspaceSection title="结算与开票" width="form">
            <DetailWorkspaceFacts columns={2}>
                <DetailWorkspaceFact
                    label="结算主体"
                    value={rev.settlementParty.displayName}
                />
                <DetailWorkspaceFact
                    label="付款条件"
                    value={rev.paymentTermSnapshot.label}
                />
                <DetailWorkspaceFact
                    label="付款说明"
                    value={rev.paymentTermSnapshot.description}
                />
                <DetailWorkspaceFact
                    label="开票类型"
                    value={rev.invoiceRequirementSnapshot.titleType}
                />
                <DetailWorkspaceFact
                    label="开票内容"
                    value={rev.invoiceRequirementSnapshot.contentSummary}
                />
                <DetailWorkspaceFact
                    label="税号"
                    value={rev.invoiceRequirementSnapshot.taxIdMasked ?? "—"}
                    numeric
                />
            </DetailWorkspaceFacts>
        </DetailWorkspaceSection>
    )
}
