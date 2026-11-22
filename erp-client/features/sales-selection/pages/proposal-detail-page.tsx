"use client"

import Link from "next/link"
import { CopyIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspaceMeta,
    DetailWorkspacePage,
    DetailWorkspaceSection,
    DetailWorkspaceSkeleton,
    GuardedBusinessAction,
    detailWorkspaceStyles,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { toast } from "@/components/ui/toast"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { isNotFoundError } from "@/lib/api/errors"
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

function safeReturnToHref(returnTo?: string): string | undefined {
    if (!returnTo) return undefined
    if (!returnTo.startsWith("/") || returnTo.startsWith("//")) return undefined
    return returnTo
}

export function ProposalDetailPage({
    proposalId,
    returnTo,
}: {
    proposalId: string
    returnTo?: string
}) {
    const query = useProposalQuery(proposalId)
    const account = useAccountProfileQuery()
    const proposal = query.data
    const customReturnHref = safeReturnToHref(returnTo)

    const fallbackBack = {
        id: "selection-proposal-back",
        label: customReturnHref ? "返回上一页" : "返回选品册",
        href: customReturnHref ?? "/sales/selection",
        visibility: customReturnHref
            ? ("always" as const)
            : ("standalone-only" as const),
    }

    if (query.isPending) {
        return (
            <DetailWorkspaceSkeleton
                title="销售方案"
                description="正在加载销售方案…"
                sidebar
            />
        )
    }

    if (query.isError && !isNotFoundError(query.error)) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title="销售方案" back={fallbackBack} />
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

    if (!proposal || (query.isError && isNotFoundError(query.error))) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome
                    title="未找到销售方案"
                    back={fallbackBack}
                />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <BusinessEmptyState
                            kind="no-data"
                            title="未找到该销售方案"
                            description="可能方案编号有误，或当前账号无权查看。"
                            action={
                                <Button
                                    id="selection-proposal-notfound-back"
                                    size="sm"
                                    render={
                                        <Link
                                            href={
                                                customReturnHref ??
                                                "/sales/selection"
                                            }
                                        />
                                    }
                                >
                                    返回选品册列表
                                </Button>
                            }
                        />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    const sourceBookHref = `/sales/selection/${encodeURIComponent(proposal.booklet_id)}`
    const backLink = {
        id: "selection-proposal-back",
        label: customReturnHref ? "返回上一页" : "返回选品册",
        href: customReturnHref ?? sourceBookHref,
        visibility: customReturnHref
            ? ("always" as const)
            : ("standalone-only" as const),
    }

    const canCreateOrderPermission = hasPermission(
        account.data?.permissions,
        "sales_order:create",
    )
    const isQuantitySubmit = proposal.submit_mode === "BY_QUANTITY"
    const hasLines = proposal.display_lines.length > 0

    return (
        <DetailWorkspacePage>
            <DetailWorkspaceChrome
                title={proposal.customer_name}
                back={backLink}
                documentNumber={proposal.proposal_no}
                numberLabel="方案编号"
                primaryStatus={{ label: "客户已提交", tone: "success" }}
                navigationMeta="选品册 / 销售方案"
                secondaryActions={
                    <Button
                        id="selection-proposal-open-book"
                        size="sm"
                        variant="ghost"
                        className={detailWorkspaceStyles.quietButton}
                        render={<Link href={sourceBookHref} />}
                    >
                        查看来源选品册
                    </Button>
                }
                primaryAction={
                    isQuantitySubmit && canCreateOrderPermission ? (
                        <GuardedBusinessAction
                            id="selection-proposal-create-order"
                            size="sm"
                            disabled={!hasLines}
                            reason={
                                !hasLines
                                    ? "当前方案暂无商品明细，无法创建销售单"
                                    : undefined
                            }
                            render={
                                hasLines ? (
                                    <Link
                                        href={`/sales/orders?mode=create&selectionProposalId=${encodeURIComponent(proposal.id)}`}
                                    />
                                ) : undefined
                            }
                        >
                            创建销售单
                        </GuardedBusinessAction>
                    ) : undefined
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
                            <span key="customer">
                                客户{" "}
                                <Link
                                    id="selection-proposal-customer-link"
                                    href={`/sales/customers/${encodeURIComponent(proposal.customer_id)}`}
                                    className="font-medium text-foreground underline-offset-4 hover:text-primary hover:underline"
                                >
                                    {proposal.customer_name}
                                </Link>
                            </span>,
                            <Button
                                key="copy"
                                id="selection-proposal-copy-number"
                                type="button"
                                variant="ghost"
                                size="xs"
                                className="-mx-1 h-5 px-1.5 text-xs font-normal text-muted-foreground hover:text-foreground"
                                aria-label="复制方案编号"
                                title="复制方案编号"
                                onClick={() =>
                                    void copyProposalNumber(
                                        proposal.proposal_no,
                                    )
                                }
                            >
                                <CopyIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                复制编号
                            </Button>,
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
