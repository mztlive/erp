"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"

import {
    BusinessFailureState,
    ObjectSectionTabs,
    ObjectSectionTabsPanel,
    PageActions,
    PageHeader,
    PageScaffold,
    surfacePanelClassName,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"
import { ContractPaperDialog } from "@/features/contracts/components/contract-paper-dialog"
import { ContractDetailAttachments } from "@/features/contracts/components/contract-detail-attachments"
import { ContractDetailHeader } from "@/features/contracts/components/contract-detail-header"
import { ContractDetailOverview } from "@/features/contracts/components/contract-detail-overview"
import { ContractDetailSalesOrders } from "@/features/contracts/components/contract-detail-sales-orders"
import { ContractDetailSettlement } from "@/features/contracts/components/contract-detail-settlement"
import { ContractDetailVersions } from "@/features/contracts/components/contract-detail-versions"
import { useContractCenterQuery } from "@/features/contracts/hooks/queries"
import {
    CONTRACT_SECTION_NAV,
    contractSectionHref,
    resolveSection,
    type ContractDetailSectionId,
} from "@/features/contracts/lib/contract-detail-helpers"

export function ContractDetailPage({
    contractId,
    section,
}: {
    contractId: string
    section?: string
}) {
    const router = useRouter()
    const query = useContractCenterQuery(contractId)

    const activeSection = resolveSection(section)
    const [paperOpen, setPaperOpen] = React.useState(false)

    const handleSectionChange = React.useCallback(
        (next: string) => {
            const sectionId = resolveSection(next)
            router.replace(contractSectionHref(contractId, sectionId), {
                scroll: false,
            })
        },
        [contractId, router],
    )

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
                    id="card-contracts-detail-failure"
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
                        <Button
                            id="card-contracts-detail-notfound-back"
                            render={<Link href="/sales/contracts" />}
                        >
                            返回列表
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    return (
        <PageScaffold>
            <PageHeader
                title="合同详情"
                actions={
                    <PageActions
                        actions={[
                            {
                                actionKey: "back",
                                id: "card-contracts-detail-header-back",
                                label: "返回列表",
                                variant: "outline",
                                render: <Link href="/sales/contracts" />,
                            },
                        ]}
                    />
                }
            />

            <ContractDetailHeader
                contract={contract}
                onPaperOpen={() => setPaperOpen(true)}
            />

            <div
                className={cn(surfacePanelClassName, "min-w-0 overflow-hidden")}
            >
                <ObjectSectionTabs
                    id={`contracts-detail-tabs-${contractId}`}
                    value={activeSection}
                    onValueChange={handleSectionChange}
                    items={CONTRACT_SECTION_NAV}
                    listLabel="合同分区"
                >
                    <ObjectSectionTabsPanel value="overview">
                        <ContractDetailOverview
                            contract={contract}
                            onOpenSalesOrders={() =>
                                handleSectionChange(
                                    "sales-orders" satisfies ContractDetailSectionId,
                                )
                            }
                        />
                    </ObjectSectionTabsPanel>

                    <ObjectSectionTabsPanel value="settlement">
                        <ContractDetailSettlement contract={contract} />
                    </ObjectSectionTabsPanel>

                    <ObjectSectionTabsPanel value="attachments">
                        <ContractDetailAttachments contract={contract} />
                    </ObjectSectionTabsPanel>

                    <ObjectSectionTabsPanel value="sales-orders">
                        <ContractDetailSalesOrders contract={contract} />
                    </ObjectSectionTabsPanel>

                    <ObjectSectionTabsPanel value="versions">
                        <ContractDetailVersions contract={contract} />
                    </ObjectSectionTabsPanel>
                </ObjectSectionTabs>
            </div>

            <ContractPaperDialog
                contract={contract}
                open={paperOpen}
                onOpenChange={setPaperOpen}
            />
        </PageScaffold>
    )
}
