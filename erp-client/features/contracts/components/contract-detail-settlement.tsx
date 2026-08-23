"use client"

import { DocumentSection, DocumentSummary } from "@/components/business"
import type { ContractCenterView } from "@/features/contracts/types"

/** 结算与开票分区：当前修订的结构化记录。 */
export function ContractDetailSettlement({
    contract,
}: {
    contract: ContractCenterView
}) {
    const rev = contract.currentRevision

    return (
        <DocumentSection
            title="结算与开票"
            description="当前合同修订的结构化记录；销售单关联时锁定该版本。"
        >
            <DocumentSummary
                columns="two"
                items={[
                    {
                        id: "party",
                        label: "结算主体",
                        value: rev.settlementParty.displayName,
                    },
                    {
                        id: "payment",
                        label: "付款条件",
                        value: rev.paymentTermSnapshot.label,
                    },
                    {
                        id: "payment-desc",
                        label: "付款说明",
                        value: rev.paymentTermSnapshot.description,
                    },
                    {
                        id: "invoice-type",
                        label: "开票类型",
                        value: rev.invoiceRequirementSnapshot.titleType,
                    },
                    {
                        id: "invoice-content",
                        label: "开票内容",
                        value: rev.invoiceRequirementSnapshot.contentSummary,
                    },
                    {
                        id: "tax",
                        label: "税号（打码）",
                        value:
                            rev.invoiceRequirementSnapshot.taxIdMasked ?? "—",
                        numeric: true,
                    },
                ]}
            />
        </DocumentSection>
    )
}
