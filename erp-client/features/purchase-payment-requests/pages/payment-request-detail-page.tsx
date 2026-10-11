"use client"

import { RefreshCwIcon } from "lucide-react"
import { DetailPageHeader, PageScaffold } from "@/components/business"
import { Button } from "@/components/ui/button"
import { getErrorMessage } from "@/lib/api"
import { usePaymentRequestQuery } from "../hooks/queries"
import { RequestDetail } from "../components/request-detail"
import { requestStatusLabel, requestStatusTone } from "../lib/display"

export function PaymentRequestDetailPage({ requestId }: { requestId: string }) {
    const query = usePaymentRequestQuery(requestId)
    return (
        <PageScaffold>
            <DetailPageHeader
                back={{
                    id: "ppr-detail-back",
                    label: "返回申请列表",
                    href: "/procurement/payment-requests",
                }}
                title={query.data?.supplier_name ?? "采购付款申请"}
                numberLabel="申请单号"
                documentNumber={query.data?.request_no}
                primaryStatus={
                    query.data
                        ? {
                              label: requestStatusLabel[query.data.status],
                              tone: requestStatusTone[query.data.status],
                          }
                        : undefined
                }
                secondaryActions={
                    <Button
                        id="ppr-detail-refresh"
                        variant="ghost"
                        size="sm"
                        onClick={() => void query.refetch()}
                    >
                        <RefreshCwIcon />
                        刷新
                    </Button>
                }
            />
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
                <div className="min-w-0">
                    <RequestDetail key={requestId} request={query.data} />
                </div>
            )}
        </PageScaffold>
    )
}
