"use client"

import { SupplierCreditUsePanel } from "./supplier-credit-use"
import { useState, type ReactNode } from "react"
import { ChevronDownIcon, ChevronRightIcon } from "lucide-react"
import { MoneyValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import type { PaymentRequest, PaymentTaskContext } from "../types"
import { RequestPaymentForm } from "./request-payment-form"
import { RequestPaymentHistory } from "./request-payment-history"

function ReferenceSection({
    id,
    label,
    children,
}: {
    id: string
    label: string
    children: ReactNode
}) {
    const [open, setOpen] = useState(false)
    return (
        <section className="border-t py-1">
            <Button
                id={`${id}-toggle`}
                variant="ghost"
                className="h-auto w-full justify-start px-0 py-3 text-sm"
                aria-expanded={open}
                aria-controls={id}
                onClick={() => setOpen(!open)}
            >
                {open ? <ChevronDownIcon /> : <ChevronRightIcon />}
                {label}
            </Button>
            <div id={id} hidden={!open}>
                {open && <div className="space-y-5 pb-4">{children}</div>}
            </div>
        </section>
    )
}

/** 出纳按申请执行付款，仅展示执行信息与历史付款回单。 */
export function RequestPaymentExecution({
    request,
    task,
    onTaskCompleted,
}: {
    request: PaymentRequest
    task: PaymentTaskContext
    onTaskCompleted?: () => void
}) {
    return (
        <div className="space-y-6">
            <section aria-label="付款任务摘要" className="space-y-4">
                <div className="flex flex-wrap items-center justify-between gap-4 rounded-lg bg-muted/45 px-5 py-4">
                    <div className="space-y-1">
                        <p className="text-xs text-muted-foreground">
                            本申请剩余待付
                        </p>
                        <MoneyValue value={request.unpaid_amount} size="hero" />
                        <p className="text-xs text-muted-foreground">
                            申请金额 <MoneyValue value={request.total_amount} />
                            <span className="mx-2">·</span>
                            本申请已结算{" "}
                            <MoneyValue value={request.paid_amount} />
                        </p>
                    </div>
                    <div className="space-y-1 text-sm">
                        <p className="text-xs text-muted-foreground">
                            希望付款日
                        </p>
                        <p className="num font-medium">
                            {request.desired_payment_date}
                        </p>
                    </div>
                </div>
                <dl className="space-y-2 text-sm">
                    <div className="flex flex-wrap gap-x-3 gap-y-1">
                        <dt className="text-muted-foreground">申请人</dt>
                        <dd>{request.applicant_name}</dd>
                    </div>
                    <div className="grid grid-cols-[auto_minmax(0,1fr)] gap-3">
                        <dt className="text-muted-foreground">申请说明</dt>
                        <dd className="whitespace-pre-wrap break-words">
                            {request.description}
                        </dd>
                    </div>
                </dl>
            </section>
            <SupplierCreditUsePanel
                request={request}
                task={task}
                onTaskCompleted={onTaskCompleted}
            />
            <RequestPaymentForm
                request={request}
                task={task}
                onTaskCompleted={onTaskCompleted}
            />
            {(request.payments.length > 0 ||
                request.credit_uses.length > 0) && (
                <ReferenceSection
                    id="ppr-pay-history"
                    label={`查看付款与留抵记录（${request.payments.length + request.credit_uses.length} 笔）`}
                >
                    <RequestPaymentHistory
                        request={request}
                        workItemId={task.workItemId}
                    />
                </ReferenceSection>
            )}
        </div>
    )
}
