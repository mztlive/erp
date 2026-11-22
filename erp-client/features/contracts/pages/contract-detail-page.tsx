"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter } from "@/lib/navigation"

import {
    BusinessFailureState,
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspacePage,
    DetailWorkspaceSection,
    DetailWorkspaceSkeleton,
    DetailWorkspaceTabs,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { ContractUploadDialog } from "@/features/contracts/contract-upload-dialog"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { ContractPaperDialog } from "@/features/contracts/components/contract-paper-dialog"
import { ContractSalesAccessDialog } from "@/features/contracts/components/contract-sales-access-dialog"
import { ContractDetailAttachments } from "@/features/contracts/components/contract-detail-attachments"
import { ContractDetailHeader } from "@/features/contracts/components/contract-detail-header"
import {
    ContractDetailOverview,
    ContractDetailRelatedSalesSummary,
} from "@/features/contracts/components/contract-detail-overview"
import { ContractDetailSalesOrders } from "@/features/contracts/components/contract-detail-sales-orders"
import { ContractDetailSettlement } from "@/features/contracts/components/contract-detail-settlement"
import { ContractDetailVersions } from "@/features/contracts/components/contract-detail-versions"
import { useContractCenterQuery } from "@/features/contracts/hooks/queries"
import {
    CONTRACT_SECTION_NAV,
    contractSectionHref,
    resolveSection,
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
    const profile = useAccountProfileQuery()
    const [importOpen, setImportOpen] = React.useState(false)
    const [importTarget, setImportTarget] = React.useState<{
        contractId: string
        version: number
    }>()

    const activeSection = resolveSection(section)
    const tabsId = `contracts-detail-tabs-${contractId}`
    const tabIdPrefix = `${tabsId}-tab`
    const [paperOpen, setPaperOpen] = React.useState(false)
    const [salesAccessOpen, setSalesAccessOpen] = React.useState(false)

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
            <DetailWorkspaceSkeleton
                title="合同"
                description="正在加载详情…"
                sidebar
            />
        )
    }

    if (query.isError) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title="合同" />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <BusinessFailureState
                            id="card-contracts-detail-failure"
                            title="合同加载失败"
                            error={query.error}
                            onRetry={() => void query.refetch()}
                        />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    if (!contract) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome
                    title="合同不存在"
                    meta="未找到这份合同。可能编号有误，或当前角色无权查看。"
                    primaryAction={
                        <Button
                            id="card-contracts-detail-notfound-back"
                            size="sm"
                            render={<Link href="/sales/contracts" />}
                        >
                            返回列表
                        </Button>
                    }
                />
            </DetailWorkspacePage>
        )
    }

    return (
        <DetailWorkspacePage>
            <ContractDetailHeader
                contract={contract}
                tabs={
                    <div id={tabsId}>
                        <DetailWorkspaceTabs
                            value={activeSection}
                            onValueChange={handleSectionChange}
                            items={CONTRACT_SECTION_NAV}
                            ariaLabel="合同分区"
                            tabIdPrefix={tabIdPrefix}
                        />
                    </div>
                }
                onPaperOpen={() => setPaperOpen(true)}
                onManageSalesAccess={
                    hasPermission(
                        profile.data?.permissions,
                        "contract:update",
                    ) &&
                    hasPermission(
                        profile.data?.permissions,
                        "customer_assignment:create",
                    )
                        ? () => setSalesAccessOpen(true)
                        : undefined
                }
                onImportRevision={
                    hasPermission(
                        profile.data?.permissions,
                        "contract:update",
                    ) &&
                    hasPermission(
                        profile.data?.permissions,
                        "contract:create",
                    ) &&
                    contract.status === "EFFECTIVE"
                        ? () => {
                              setImportTarget({
                                  contractId,
                                  version: contract.lockVersion,
                              })
                              setImportOpen(true)
                          }
                        : undefined
                }
            />

            <div
                id={`${tabsId}-panel-${activeSection}`}
                role="tabpanel"
                aria-labelledby={`${tabIdPrefix}-${activeSection}`}
                tabIndex={0}
                className="min-w-0 flex-1"
            >
                <DetailWorkspaceBody
                    sidebarLabel="关联销售摘要"
                    sidebar={
                        activeSection === "overview" ? (
                            <ContractDetailRelatedSalesSummary
                                contract={contract}
                                onOpenSalesOrders={() =>
                                    handleSectionChange("sales-orders")
                                }
                            />
                        ) : undefined
                    }
                    alerts={contract.loadWarnings?.map((message) => (
                        <BusinessFailureState
                            key={message}
                            title="部分关联资料未能读取"
                            description={message}
                        />
                    ))}
                >
                    {activeSection === "overview" ? (
                        <ContractDetailOverview contract={contract} />
                    ) : activeSection === "settlement" ? (
                        <ContractDetailSettlement contract={contract} />
                    ) : activeSection === "attachments" ? (
                        <ContractDetailAttachments contract={contract} />
                    ) : activeSection === "sales-orders" ? (
                        <ContractDetailSalesOrders contract={contract} />
                    ) : (
                        <ContractDetailVersions contract={contract} />
                    )}
                </DetailWorkspaceBody>
            </div>

            <ContractUploadDialog
                open={importOpen}
                onOpenChange={setImportOpen}
                initialCustomerId={contract.customer.id}
                revisionTarget={importTarget}
                onSuccess={() => {
                    void query.refetch()
                }}
            />
            <ContractSalesAccessDialog
                contract={contract}
                open={salesAccessOpen}
                onOpenChange={setSalesAccessOpen}
            />
            <ContractPaperDialog
                contract={contract}
                open={paperOpen}
                onOpenChange={setPaperOpen}
            />
        </DetailWorkspacePage>
    )
}
