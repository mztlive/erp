"use client"

import Link from "next/link"
import { ArrowLeftIcon, RefreshCwIcon } from "lucide-react"
import { PageScaffold } from "@/components/business"
import {
    ListWorkspaceHeader,
    listWorkspaceStyles,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { getErrorMessage } from "@/lib/api"
import { usePaymentRequestQuery } from "../hooks/queries"
import { RequestDetail } from "../components/request-detail"

export function PaymentRequestDetailPage({ requestId }: { requestId: string }) {
    const query = usePaymentRequestQuery(requestId)
    return (
        <PageScaffold density="compact" className={listWorkspaceStyles.page}>
            <ListWorkspaceHeader
                eyebrow="采购 · 财务"
                title="采购付款申请"
                description={query.data?.request_no}
            >
                <Button
                    id="ppr-detail-back"
                    variant="outline"
                    render={<Link href="/procurement/payment-requests" />}
                >
                    <ArrowLeftIcon />
                    返回申请列表
                </Button>
                <Button
                    id="ppr-detail-refresh"
                    variant="ghost"
                    onClick={() => void query.refetch()}
                >
                    <RefreshCwIcon />
                    刷新
                </Button>
            </ListWorkspaceHeader>
            {query.isPending ? (
                <p className="py-8 text-sm text-muted-foreground">
                    正在读取付款申请…
                </p>
            ) : query.isError ? (
                <p role="alert" className="py-6 text-sm text-destructive">
                    {getErrorMessage(
                        query.error,
                        "付款申请读取失败，请刷新后重试",
                    )}
                </p>
            ) : (
                <div className="max-w-5xl">
                    <RequestDetail request={query.data} />
                </div>
            )}
        </PageScaffold>
    )
}
