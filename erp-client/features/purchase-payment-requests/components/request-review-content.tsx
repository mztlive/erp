"use client"

import { useState, type ReactNode } from "react"
import { ChevronDownIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import type { PaymentRequest } from "../types"
import { RequestMaterials } from "./request-materials"
import { RequestReviewSummary } from "./request-review-summary"
import { RequestReviewOrders } from "./request-review-orders"
import { RequestReviewHistory } from "./request-review-history"
import { RequestSubmissionHistory } from "./request-submission-history"

/** 付款审批单栏阅读面；审批命令继续由现有底栏承载。 */
export function RequestReviewContent({
    request,
    account,
    approval,
    embedded,
    showHistory,
    submissionVersion,
    workItemId,
}: {
    request: PaymentRequest
    account: ReactNode
    approval: ReactNode
    embedded: boolean
    showHistory: boolean
    submissionVersion?: number
    workItemId?: string
}) {
    const [historyOpen, setHistoryOpen] = useState(false)
    const bank = request.execution_method.type === "bank_transfer"
    return (
        <div className="@container/payment-review mx-auto w-full max-w-6xl space-y-5">
            <RequestReviewSummary
                request={request}
                submissionVersion={submissionVersion}
            />
            <div className="space-y-8 rounded-xl border border-border/60 bg-card p-6 sm:p-8">
                <RequestReviewOrders request={request} />
                <RequestReviewHistory
                    request={request}
                    workItemId={workItemId}
                    submissionVersion={submissionVersion}
                />
            </div>
            <section
                aria-label="收款与材料"
                className="space-y-8 rounded-xl border border-border/60 bg-card p-6 sm:p-8"
            >
                <h2 className="text-lg font-semibold">
                    {bank ? "收款与材料" : "预存款与材料"}
                </h2>
                <dl className="grid gap-x-10 gap-y-5 text-sm @min-[780px]/payment-review:grid-cols-2">
                    <Info label="付款主体">{request.our_company_name}</Info>
                    {bank ? (
                        <>
                            <Info label="收款户名">
                                {request.payment_recipient?.account_name ??
                                    "未记录"}
                            </Info>
                            <Info label="开户行">
                                {request.payment_recipient?.bank_name ??
                                    "未记录"}
                            </Info>
                            <Info label="收款账号">{account}</Info>
                        </>
                    ) : (
                        <>
                            <Info label="付款方式">供应商预存款</Info>
                            <Info label="预存款账户">
                                {request.deposit_account?.identity
                                    .external_account_ref
                                    ? `人民币预存款 · ****${request.deposit_account.identity.external_account_ref.slice(-4)}`
                                    : "未记录账户标识"}
                            </Info>
                        </>
                    )}
                    <Info label="申请人">{request.applicant_name}</Info>
                    <Info label="申请说明">{request.description || "—"}</Info>
                </dl>
                {!bank && (
                    <p className="text-sm text-muted-foreground">
                        批准后占用预存款额度，实际扣款核实后核销，不登记银行付款。
                    </p>
                )}
                <RequestMaterials
                    request={request}
                    workItemId={workItemId}
                    submissionVersion={submissionVersion}
                    concise
                />
                {showHistory && (
                    <div className="border-t pt-4">
                        <Button
                            id="ppr-review-history-toggle"
                            variant="ghost"
                            aria-expanded={historyOpen}
                            aria-controls="ppr-review-history-content"
                            onClick={() => setHistoryOpen((open) => !open)}
                        >
                            审批记录
                            <ChevronDownIcon
                                className={
                                    historyOpen ? "rotate-180" : undefined
                                }
                            />
                        </Button>
                        {historyOpen && (
                            <div
                                id="ppr-review-history-content"
                                className="space-y-6 pt-4"
                            >
                                {approval ?? (
                                    <p className="text-sm text-muted-foreground">
                                        尚未提交财务审核。
                                    </p>
                                )}
                                {!embedded && (
                                    <RequestSubmissionHistory
                                        request={request}
                                    />
                                )}
                            </div>
                        )}
                    </div>
                )}
            </section>
        </div>
    )
}

function Info({ label, children }: { label: string; children: ReactNode }) {
    return (
        <div className="grid min-w-0 grid-cols-[5rem_minmax(0,1fr)] gap-4">
            <dt className="text-muted-foreground">{label}</dt>
            <dd className="min-w-0 whitespace-pre-wrap break-words">
                {children}
            </dd>
        </div>
    )
}
