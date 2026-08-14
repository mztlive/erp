"use client"

import * as React from "react"
import Link from "next/link"

import {
    BusinessFailureState,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { ContractPaperDialog } from "@/features/contracts/components/contract-paper-dialog"
import { ContractDetailAttachments } from "@/features/contracts/components/contract-detail-attachments"
import { ContractDetailHeader } from "@/features/contracts/components/contract-detail-header"
import { ContractDetailOverview } from "@/features/contracts/components/contract-detail-overview"
import { ContractDetailSalesOrders } from "@/features/contracts/components/contract-detail-sales-orders"
import { ContractDetailSettlement } from "@/features/contracts/components/contract-detail-settlement"
import { ContractDetailVersions } from "@/features/contracts/components/contract-detail-versions"
import { useContractCenterQuery } from "@/features/contracts/hooks/queries"
import { resolveSection } from "@/features/contracts/lib/contract-detail-helpers"

export function ContractDetailPage({
    contractId,
    section,
}: {
    contractId: string
    section?: string
}) {
    const query = useContractCenterQuery(contractId)

    const activeSection = resolveSection(section)
    const [paperOpen, setPaperOpen] = React.useState(false)

    const contract = query.data

    if (query.isPending) {
        return (
            <PageScaffold>
                <PageHeader title="合同" description="正在加载详情…" />
                <div className="space-y-3" aria-busy="true" aria-label="加载中">
                    <div className="h-16 animate-pulse rounded-lg bg-muted" />
                    <div className="h-24 animate-pulse rounded-lg bg-muted" />
                    <div className="h-40 animate-pulse rounded-lg bg-muted" />
                </div>
            </PageScaffold>
        )
    }

    if (query.isError) {
        return (
            <PageScaffold>
                <PageHeader title="合同" />
                <BusinessFailureState
                    title="合同加载失败"
                    error={query.error}
                    onRetry={() => {
                        void query.refetch()
                    }}
                />
            </PageScaffold>
        )
    }

    if (!contract) {
        return (
            <PageScaffold>
                <PageHeader
                    title="合同不存在"
                    description="未找到这份合同。可能编号有误，或当前角色无权查看。"
                    actions={
                        <Button render={<Link href="/sales/contracts" />}>
                            返回列表
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    return (
        <PageScaffold>
            <ContractDetailHeader
                contract={contract}
                activeSection={activeSection}
                onPaperOpen={() => setPaperOpen(true)}
            />

            {activeSection === "overview" ? (
                <ContractDetailOverview contract={contract} />
            ) : null}

            {activeSection === "settlement" ? (
                <ContractDetailSettlement contract={contract} />
            ) : null}

            {activeSection === "attachments" ? (
                <ContractDetailAttachments contract={contract} />
            ) : null}

            {activeSection === "sales-orders" ? (
                <ContractDetailSalesOrders contract={contract} />
            ) : null}

            {activeSection === "versions" ? (
                <ContractDetailVersions contract={contract} />
            ) : null}

            <ContractPaperDialog
                contract={contract}
                open={paperOpen}
                onOpenChange={setPaperOpen}
            />
        </PageScaffold>
    )
}
