"use client"

import Link from "next/link"
import { CopyIcon } from "lucide-react"

import {
    BusinessFailureState,
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspaceMeta,
    DetailWorkspacePage,
    DetailWorkspaceSection,
    detailWorkspaceStyles,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { toast } from "@/components/ui/toast"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { ProposalItems } from "@/features/sales-selection/components/proposal-items"
import { ProposalSummary } from "@/features/sales-selection/components/proposal-summary"
import { useProposalQuery } from "@/features/sales-selection/queries"
import { FORM_LABEL, SUBMIT_MODE_LABEL } from "@/features/sales-selection/types"

async function copyProposalNumber(proposalNo: string) {
    try {
        await navigator.clipboard.writeText(proposalNo)
        toast.add({ title: "方案编号已复制", type: "success" })
    } catch {
        toast.add({
            title: "请手动复制方案编号",
            description: proposalNo,
            type: "info",
        })
    }
}

const backToList = {
    id: "selection-proposal-back",
    label: "返回",
    href: "/sales/selection",
}

export function ProposalDetailPage({ proposalId }: { proposalId: string }) {
    const query = useProposalQuery(proposalId)
    const account = useAccountProfileQuery()
    const proposal = query.data

    if (query.isError) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title="销售方案" back={backToList} />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <BusinessFailureState
                            id="selection-proposal"
                            title="销售方案加载失败"
                            description="请重试，或返回选品册确认是否有权查看该方案。"
                            error={query.error}
                            onRetry={() => void query.refetch()}
                        />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }
    if (!proposal) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title="销售方案" back={backToList} />
                <DetailWorkspaceBody
                    sidebarLabel="方案摘要"
                    sidebar={<Skeleton className="h-32" />}
                >
                    <DetailWorkspaceSection>
                        <div aria-busy="true" aria-label="正在加载销售方案">
                            <span role="status" className="sr-only">
                                正在加载销售方案…
                            </span>
                            <Skeleton className="h-80" />
                        </div>
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    const canCreateOrder =
        proposal.submit_mode === "BY_QUANTITY" &&
        hasPermission(account.data?.permissions, "sales_order:create")
    const openBook = (
        <Button
            id="selection-proposal-open-book"
            size="sm"
            variant={canCreateOrder ? "ghost" : "default"}
            className={
                canCreateOrder ? detailWorkspaceStyles.quietButton : undefined
            }
            render={
                <Link
                    href={`/sales/selection/${encodeURIComponent(proposal.booklet_id)}`}
                />
            }
        >
            查看来源选品册
        </Button>
    )

    return (
        <DetailWorkspacePage>
            <DetailWorkspaceChrome
                title={proposal.customer_name}
                back={backToList}
                navigationMeta="选品册 / 销售方案"
                secondaryActions={canCreateOrder ? openBook : undefined}
                primaryAction={
                    canCreateOrder ? (
                        <Button
                            id="selection-proposal-create-order"
                            size="sm"
                            render={
                                <Link
                                    href={`/sales/orders?mode=create&selectionProposalId=${encodeURIComponent(proposal.id)}`}
                                />
                            }
                        >
                            创建销售单
                        </Button>
                    ) : (
                        openBook
                    )
                }
                titleExtra={
                    <span className="inline-flex flex-wrap items-center gap-1.5">
                        <Badge variant="secondary">
                            {FORM_LABEL[proposal.form]}
                        </Badge>
                        <Badge variant="secondary">
                            {SUBMIT_MODE_LABEL[proposal.submit_mode]}
                        </Badge>
                    </span>
                }
                meta={
                    <DetailWorkspaceMeta
                        items={[
                            "销售方案",
                            <span
                                key="number"
                                className="inline-flex max-w-full flex-wrap items-center gap-1"
                            >
                                <span>
                                    方案编号：
                                    <span className="num select-text">
                                        {proposal.proposal_no}
                                    </span>
                                </span>
                                <Button
                                    id="selection-proposal-copy-number"
                                    variant="ghost"
                                    size="icon-xs"
                                    aria-label="复制方案编号"
                                    title="复制方案编号"
                                    onClick={() =>
                                        void copyProposalNumber(
                                            proposal.proposal_no,
                                        )
                                    }
                                >
                                    <CopyIcon aria-hidden="true" />
                                </Button>
                            </span>,
                        ]}
                    />
                }
            />
            <DetailWorkspaceBody
                sidebarLabel="方案摘要"
                sidebar={<ProposalSummary proposal={proposal} />}
            >
                <ProposalItems proposal={proposal} />
            </DetailWorkspaceBody>
        </DetailWorkspacePage>
    )
}
