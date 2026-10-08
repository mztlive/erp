"use client"

import { CopyIcon } from "lucide-react"

import {
    BusinessFailureState,
    DetailPageHeader,
    PageScaffold,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { toast } from "@/components/ui/toast"
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
    const proposal = query.data

    if (query.isError) {
        return (
            <PageScaffold>
                <DetailPageHeader title="销售方案" back={backToList} />
                <BusinessFailureState
                    id="selection-proposal"
                    title="销售方案加载失败"
                    description="请重试，或返回选品册确认是否有权查看该方案。"
                    error={query.error}
                    onRetry={() => void query.refetch()}
                />
            </PageScaffold>
        )
    }
    if (!proposal) {
        return (
            <PageScaffold>
                <DetailPageHeader title="销售方案" back={backToList} />
                <div
                    aria-busy="true"
                    aria-label="正在加载销售方案"
                    className="space-y-6"
                >
                    <span role="status" className="sr-only">
                        正在加载销售方案…
                    </span>
                    <Skeleton className="h-24 w-full max-w-2xl rounded-xl" />
                    <div className="grid gap-8 xl:grid-cols-[minmax(0,1fr)_320px]">
                        <Skeleton className="h-80 rounded-xl" />
                        <Skeleton className="h-80 rounded-xl" />
                    </div>
                </div>
            </PageScaffold>
        )
    }

    return (
        <PageScaffold className="gap-8">
            <DetailPageHeader
                title={proposal.customer_name}
                headingProps={{
                    className: "text-page-title md:text-page-title",
                }}
                back={backToList}
                navigationMeta="选品册 / 销售方案"
                meta={
                    <div className="flex flex-col items-start gap-3">
                        <p className="text-base font-medium text-foreground">
                            销售方案
                        </p>
                        <div className="flex max-w-full items-center gap-2">
                            <span className="min-w-0 break-all">
                                方案编号：
                                <span className="num select-text">
                                    {proposal.proposal_no}
                                </span>
                            </span>
                            <Button
                                id="selection-proposal-copy-number"
                                variant="ghost"
                                size="icon-sm"
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
                        </div>
                        <div className="flex flex-wrap gap-2">
                            <Badge variant="secondary">
                                {FORM_LABEL[proposal.form]}
                            </Badge>
                            <Badge variant="secondary">
                                {SUBMIT_MODE_LABEL[proposal.submit_mode]}
                            </Badge>
                        </div>
                    </div>
                }
            />
            <div className="grid min-w-0 items-start gap-8 xl:grid-cols-[minmax(0,1fr)_320px]">
                <ProposalItems proposal={proposal} />
                <ProposalSummary proposal={proposal} />
            </div>
        </PageScaffold>
    )
}
