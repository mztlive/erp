"use client"

import type { ComponentProps } from "react"
import { useSearchParams } from "next/navigation"
import { RefreshCwIcon } from "lucide-react"
import {
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspacePage,
    DetailWorkspaceSection,
    detailWorkspaceStyles,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { getErrorMessage } from "@/lib/api"
import { usePaymentRequestQuery } from "../hooks/queries"
import { RequestDetail } from "../components/request-detail"
import { requestStatusLabel, requestStatusTone } from "../lib/display"

function safeReturnTo(value: string | null) {
    if (!value || !value.startsWith("/") || value.startsWith("//")) return null
    return value
}

export function PaymentRequestDetailPage({ requestId }: { requestId: string }) {
    const searchParams = useSearchParams()
    const query = usePaymentRequestQuery(requestId)
    const returnTo = safeReturnTo(searchParams.get("returnTo"))
    const backHref = returnTo ?? "/procurement/payment-requests"
    const backLabel = returnTo
        ? returnTo.startsWith("/procurement/orders/")
            ? "返回采购单"
            : "返回上一页"
        : "返回申请列表"
    const refresh = (
        <Button
            id="ppr-detail-refresh"
            variant="ghost"
            size="sm"
            className={detailWorkspaceStyles.quietButton}
            onClick={() => void query.refetch()}
        >
            <RefreshCwIcon data-icon="inline-start" aria-hidden="true" />
            刷新
        </Button>
    )
    const renderChrome: NonNullable<
        ComponentProps<typeof RequestDetail>["renderChrome"]
    > = ({ tabs, secondaryActions, primaryAction }) => (
        <DetailWorkspaceChrome
            back={{ id: "ppr-detail-back", label: backLabel, href: backHref }}
            title={query.data?.supplier_name ?? "采购付款申请"}
            numberLabel="申请单号"
            documentNumber={query.data?.request_no}
            primaryStatus={
                query.data && {
                    label: requestStatusLabel[query.data.status],
                    tone: requestStatusTone[query.data.status],
                }
            }
            secondaryActions={
                <>
                    {refresh}
                    {secondaryActions}
                </>
            }
            primaryAction={primaryAction}
            tabs={tabs}
        />
    )
    return (
        <DetailWorkspacePage>
            {query.isPending || query.isError ? (
                <>
                    {renderChrome({})}
                    <DetailWorkspaceBody>
                        <DetailWorkspaceSection>
                            {query.isPending ? (
                                <p
                                    role="status"
                                    className="text-xs text-muted-foreground"
                                >
                                    正在读取付款申请…
                                </p>
                            ) : (
                                <p
                                    role="alert"
                                    className="text-body-compact text-destructive"
                                >
                                    {getErrorMessage(
                                        query.error,
                                        "付款申请读取失败，请刷新后重试",
                                    )}
                                </p>
                            )}
                        </DetailWorkspaceSection>
                    </DetailWorkspaceBody>
                </>
            ) : (
                <RequestDetail
                    key={requestId}
                    request={query.data}
                    renderChrome={renderChrome}
                />
            )}
        </DetailWorkspacePage>
    )
}
