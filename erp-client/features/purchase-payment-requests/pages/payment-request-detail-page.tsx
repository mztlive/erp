"use client"

import type { ComponentProps } from "react"
import {
    BusinessEmptyState,
    BusinessFailureState,
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspaceMeta,
    DetailWorkspacePage,
    DetailWorkspaceSection,
    DetailWorkspaceSkeleton,
} from "@/components/business"
import { useSearchParams } from "@/lib/navigation"
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
    const back = {
        id: "ppr-detail-back",
        label: backLabel,
        href: backHref,
        visibility: (returnTo ? "always" : "standalone-only") as
            | "always"
            | "standalone-only",
    }

    if (query.isPending) {
        return (
            <DetailWorkspaceSkeleton
                title="采购付款申请"
                description="正在读取付款申请…"
                summary
            />
        )
    }

    if (query.isError) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome
                    back={back}
                    title="采购付款申请"
                    meta={<span>详情加载失败</span>}
                />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <BusinessFailureState
                            id="ppr-detail-retry"
                            title="付款申请读取失败"
                            error={query.error}
                            onRetry={() => void query.refetch()}
                        />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    const request = query.data
    if (!request) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome
                    back={back}
                    title="采购付款申请不存在"
                    meta={<span>申请可能已删除或编号有误</span>}
                />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <BusinessEmptyState
                            kind="no-data"
                            title="未找到该采购付款申请"
                            description="该付款申请可能不存在或不在当前账号的数据范围内。"
                        />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    const paymentMethodLabel =
        request.execution_method.type === "supplier_deposit"
            ? "供应商预存款"
            : "银行转账"

    const renderChrome: NonNullable<
        ComponentProps<typeof RequestDetail>["renderChrome"]
    > = ({ tabs, secondaryActions, primaryAction }) => (
        <DetailWorkspaceChrome
            back={back}
            title={request.supplier_name || "采购付款申请"}
            numberLabel="申请单号"
            documentNumber={request.request_no}
            primaryStatus={{
                label: requestStatusLabel[request.status],
                tone: requestStatusTone[request.status],
            }}
            meta={
                <DetailWorkspaceMeta
                    items={[
                        <span key="company">
                            付款主体{" "}
                            <span className="font-medium text-foreground">
                                {request.our_company_name}
                            </span>
                        </span>,
                        <span key="applicant">
                            申请人{" "}
                            <span className="font-medium text-foreground">
                                {request.applicant_name}
                            </span>
                        </span>,
                        <span key="date">
                            希望付款日{" "}
                            <span className="num font-medium text-foreground">
                                {request.desired_payment_date}
                            </span>
                        </span>,
                        <span key="method">{paymentMethodLabel}</span>,
                    ]}
                />
            }
            secondaryActions={secondaryActions}
            primaryAction={primaryAction}
            tabs={tabs}
        />
    )

    return (
        <DetailWorkspacePage>
            <RequestDetail
                key={requestId}
                request={request}
                renderChrome={renderChrome}
            />
        </DetailWorkspacePage>
    )
}
