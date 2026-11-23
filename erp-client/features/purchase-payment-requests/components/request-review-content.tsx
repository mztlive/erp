"use client"

import { useState, type ReactNode } from "react"
import { ChevronDownIcon } from "lucide-react"
import {
    DetailWorkspaceFact,
    DetailWorkspaceFacts,
    DetailWorkspaceSection,
    detailWorkspaceStyles,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import type { PaymentRequest } from "../types"
import { RequestMaterials } from "./request-materials"
import { RequestReviewSummary } from "./request-review-summary"
import { RequestReviewOrders } from "./request-review-orders"
import { RequestReviewHistory } from "./request-review-history"
import { RequestSubmissionHistory } from "./request-submission-history"

/** 付款审批纯白通栏阅读面；审批路线位于页头下方，审批命令由底栏承载。 */
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
        <div className="@container/payment-review min-w-0">
            <RequestReviewSummary
                request={request}
                submissionVersion={submissionVersion}
            />
            <DetailWorkspaceSection ariaLabel="采购明细与付款记录">
                <RequestReviewOrders request={request} />
                <RequestReviewHistory
                    request={request}
                    workItemId={workItemId}
                    submissionVersion={submissionVersion}
                />
            </DetailWorkspaceSection>
            <DetailWorkspaceSection
                title={bank ? "收款与材料" : "预存款与材料"}
                ariaLabel="收款与材料"
            >
                <DetailWorkspaceFacts columns={3}>
                    <DetailWorkspaceFact
                        label="付款主体"
                        value={request.our_company_name}
                    />
                    {bank ? (
                        <>
                            <DetailWorkspaceFact
                                label="收款户名"
                                value={
                                    request.payment_recipient?.account_name ??
                                    "未记录"
                                }
                            />
                            <DetailWorkspaceFact
                                label="开户行"
                                value={
                                    request.payment_recipient?.bank_name ??
                                    "未记录"
                                }
                            />
                            <DetailWorkspaceFact
                                label="收款账号"
                                value={account}
                            />
                        </>
                    ) : (
                        <>
                            <DetailWorkspaceFact
                                label="付款方式"
                                value="供应商预存款"
                            />
                            <DetailWorkspaceFact
                                label="预存款账户"
                                numeric
                                value={
                                    request.deposit_account?.identity
                                        .external_account_ref
                                        ? `人民币预存款 · ****${request.deposit_account.identity.external_account_ref.slice(-4)}`
                                        : "未记录账户标识"
                                }
                            />
                        </>
                    )}
                    <DetailWorkspaceFact
                        label="申请人"
                        value={request.applicant_name}
                    />
                    <DetailWorkspaceFact
                        label="申请说明"
                        value={request.description || "—"}
                    />
                </DetailWorkspaceFacts>
                {!bank && (
                    <p className="text-xs text-muted-foreground">
                        批准后占用预存款额度，实际扣款核实后核销，不登记银行付款。
                    </p>
                )}
                <RequestMaterials
                    request={request}
                    workItemId={workItemId}
                    submissionVersion={submissionVersion}
                    concise
                />
                {showHistory && !embedded && (
                    <div className="border-t border-border/60 pt-4">
                        <Button
                            id="ppr-review-history-toggle"
                            variant="ghost"
                            size="sm"
                            className={detailWorkspaceStyles.quietButton}
                            aria-expanded={historyOpen}
                            aria-controls="ppr-review-history-content"
                            onClick={() => setHistoryOpen((open) => !open)}
                        >
                            审批记录
                            <ChevronDownIcon
                                aria-hidden="true"
                                className={
                                    historyOpen
                                        ? "rotate-180 transition-transform"
                                        : "transition-transform"
                                }
                            />
                        </Button>
                        {historyOpen && (
                            <div
                                id="ppr-review-history-content"
                                className="space-y-6 pt-4"
                            >
                                {approval ?? (
                                    <p className="text-xs text-muted-foreground">
                                        尚未提交财务审核。
                                    </p>
                                )}
                                <RequestSubmissionHistory request={request} />
                            </div>
                        )}
                    </div>
                )}
            </DetailWorkspaceSection>
        </div>
    )
}
