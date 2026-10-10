"use client"

import { useState } from "react"
import { MoneyValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { getErrorMessage } from "@/lib/api"
import { usePaymentSubmissionQuery } from "../hooks/queries"
import type { PaymentRequest } from "../types"
import { RequestMaterials } from "./request-materials"

export function RequestSubmissionHistory({
    request,
}: {
    request: PaymentRequest
}) {
    const [selected, setSelected] = useState<number | null>(null)
    const query = usePaymentSubmissionQuery(request.id, selected)
    if (request.submission_version === 0) return null
    const versions = Array.from(
        { length: request.submission_version },
        (_, index) => index + 1,
    )
    return (
        <section className="space-y-3 border-t pt-5" aria-label="历次提交材料">
            <div>
                <h3 className="font-medium">历次提交材料</h3>
                <p className="mt-1 text-xs text-muted-foreground">
                    按提交轮次查看当次申请金额、收款账户与完整审核依据。
                </p>
            </div>
            <div className="flex flex-wrap gap-2">
                {versions.map((version) => (
                    <Button
                        id={`ppr-submission-${version}-open`}
                        key={version}
                        variant="outline"
                        size="sm"
                        onClick={() => setSelected(version)}
                    >
                        第 {version} 次提交
                    </Button>
                ))}
            </div>
            <Dialog
                open={selected != null}
                onOpenChange={(open) => {
                    if (!open) setSelected(null)
                }}
            >
                <DialogContent
                    id="ppr-submission-dialog"
                    closeButtonId="ppr-submission-dialog-close"
                    className="flex max-h-[90vh] flex-col sm:max-w-4xl"
                >
                    <DialogHeader>
                        <DialogTitle>第 {selected} 次提交材料</DialogTitle>
                        <DialogDescription>
                            {request.request_no} · 本次提交时的原申请与审核依据
                        </DialogDescription>
                    </DialogHeader>
                    <div className="min-h-0 space-y-5 overflow-y-auto pr-1">
                        {query.isPending ? (
                            <p className="text-sm text-muted-foreground">
                                正在读取历史材料…
                            </p>
                        ) : query.isError ? (
                            <div className="space-y-3">
                                <p
                                    role="alert"
                                    className="text-sm text-destructive"
                                >
                                    {getErrorMessage(
                                        query.error,
                                        "历史材料读取失败",
                                    )}
                                </p>
                                <Button
                                    id="ppr-submission-retry"
                                    variant="outline"
                                    onClick={() => void query.refetch()}
                                >
                                    重试
                                </Button>
                            </div>
                        ) : query.data && selected != null ? (
                            <>
                                <dl className="grid gap-4 text-sm sm:grid-cols-2">
                                    <div>
                                        <dt className="text-xs text-muted-foreground">
                                            供应商 / 我方付款主体
                                        </dt>
                                        <dd className="mt-1">
                                            {query.data.supplier_name} ·{" "}
                                            {query.data.our_company_name}
                                        </dd>
                                    </div>
                                    <div>
                                        <dt className="text-xs text-muted-foreground">
                                            本次申请总额
                                        </dt>
                                        <dd className="mt-1">
                                            <MoneyValue
                                                value={query.data.total_amount}
                                            />
                                        </dd>
                                    </div>
                                    <div>
                                        <dt className="text-xs text-muted-foreground">
                                            收款户名 / 开户行
                                        </dt>
                                        <dd className="mt-1">
                                            {
                                                query.data.payment_recipient
                                                    .account_name
                                            }{" "}
                                            ·{" "}
                                            {
                                                query.data.payment_recipient
                                                    .bank_name
                                            }
                                        </dd>
                                    </div>
                                    <div>
                                        <dt className="text-xs text-muted-foreground">
                                            收款账号
                                        </dt>
                                        <dd className="num mt-1">
                                            {
                                                query.data.payment_recipient
                                                    .account_number_masked
                                            }
                                        </dd>
                                    </div>
                                    <div>
                                        <dt className="text-xs text-muted-foreground">
                                            希望付款日
                                        </dt>
                                        <dd className="num mt-1">
                                            {query.data.desired_payment_date}
                                        </dd>
                                    </div>
                                    <div>
                                        <dt className="text-xs text-muted-foreground">
                                            申请说明
                                        </dt>
                                        <dd className="mt-1 whitespace-pre-wrap">
                                            {query.data.description}
                                        </dd>
                                    </div>
                                </dl>
                                <div className="space-y-2">
                                    {query.data.lines.map((line) => (
                                        <div
                                            key={line.payable_account_id}
                                            className="flex flex-wrap justify-between gap-2 border-b pb-2 text-sm"
                                        >
                                            <span className="num">
                                                {line.purchase_order_no} · V
                                                {line.purchase_version}
                                            </span>
                                            <MoneyValue
                                                value={line.requested_amount}
                                            />
                                        </div>
                                    ))}
                                </div>
                                <RequestMaterials
                                    request={query.data}
                                    submissionVersion={selected}
                                    idPrefix={`ppr-submission-${selected}`}
                                />
                            </>
                        ) : null}
                    </div>
                </DialogContent>
            </Dialog>
        </section>
    )
}
