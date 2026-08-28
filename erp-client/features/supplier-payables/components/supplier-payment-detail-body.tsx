"use client"

import * as React from "react"
import Link from "next/link"
import { ExternalLinkIcon } from "lucide-react"

import {
    MoneyValue,
    RelatedDocumentList,
    type RelatedDocument,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { FileUpload } from "@/components/ui/file-upload"
import { useSupplierPaymentBankReceiptQuery } from "@/features/supplier-payables/hooks/queries"
import { paymentPreviewHref } from "@/features/supplier-payables/lib/related-documents"
import type { PaymentAllocationLine, PaymentRow } from "@/features/supplier-payables/types"
import {
    ALLOCATION_ACTION_LABEL,
    SOURCE_TYPE_LABEL,
} from "@/features/supplier-payables/types"
import { formatDateTime } from "@/lib/datetime"

/** 供应商付款事实详情；普通付款登记即过账，不包含独立审批区。 */
export function SupplierPaymentDetailBody({ row }: { row: PaymentRow }) {
    const posted = row.status === "POSTED" || row.status === "REVERSED"
    return (
        <div className="space-y-5 overflow-auto p-6">
            {posted ? (
                <Alert variant="info">
                    <AlertTitle>已过账记录只读</AlertTitle>
                    <AlertDescription>
                        已过账记录不可编辑、不可删除；纠错仅能追加冲正。
                    </AlertDescription>
                </Alert>
            ) : null}
            <div className="grid grid-cols-2 gap-3">
                <Fact label="付款单号" value={row.paymentNo} mono />
                <Fact label="供应商" value={row.supplierName} />
                <Fact
                    label="付款时间"
                    value={formatDateTime(row.paidAt, "full", "passthrough")}
                    mono
                />
                <Fact
                    label="付款金额"
                    value={<MoneyValue value={row.amount} taxBasis="gross" />}
                />
                <Fact label="银行流水号" value={row.bankReferenceMasked} mono />
                {row.paymentRecipient ? (
                    <>
                        <Fact
                            label="收款户名"
                            value={row.paymentRecipient.accountName}
                        />
                        <Fact
                            label="收款银行"
                            value={[
                                row.paymentRecipient.bankName,
                                row.paymentRecipient.bankBranchName,
                            ]
                                .filter(Boolean)
                                .join(" · ")}
                        />
                        <Fact
                            label="收款账号"
                            value={row.paymentRecipient.accountNumberMasked}
                            mono
                        />
                    </>
                ) : null}
                <Fact
                    label="净已分配"
                    value={
                        <MoneyValue
                            value={row.allocatedTotal}
                            taxBasis="gross"
                        />
                    }
                />
                <Fact
                    label="未分配"
                    value={
                        <MoneyValue
                            value={row.unallocatedAmount}
                            taxBasis="gross"
                        />
                    }
                />
            </div>
            <BankReceiptPreview row={row} />
            <section>
                <h4 className="mb-2 text-sm font-semibold">关联单据</h4>
                <RelatedDocumentList
                    documents={relatedDocumentsFromPayment(row)}
                    emptyContent="此付款尚未核销到应付或来源单据。"
                />
            </section>
            <section>
                <h4 className="mb-2 text-sm font-semibold">
                    分配明细（新增不覆盖原金额）
                </h4>
                {row.allocations.length === 0 ? (
                    <p className="text-sm text-muted-foreground">尚无分配行</p>
                ) : (
                    <ul className="space-y-2">
                        {row.allocations.map((allocation) => (
                            <AllocationLineItem
                                key={allocation.allocationId}
                                allocation={allocation}
                            />
                        ))}
                    </ul>
                )}
            </section>
        </div>
    )
}

/** 通过付款归属接口受控读取银行回单，不暴露底层对象地址。 */
function BankReceiptPreview({ row }: { row: PaymentRow }) {
    const query = useSupplierPaymentBankReceiptQuery(
        row.paymentId,
        Boolean(row.bankReceipt),
    )
    const [previewUrl, setPreviewUrl] = React.useState<string>()
    React.useEffect(() => {
        if (!query.data) {
            setPreviewUrl(undefined)
            return
        }
        const url = URL.createObjectURL(query.data)
        setPreviewUrl(url)
        return () => URL.revokeObjectURL(url)
    }, [query.data])

    return (
        <section>
            <h4 className="mb-2 text-sm font-semibold">银行回单</h4>
            {row.bankReceipt ? (
                <div className="max-w-md space-y-2">
                    <FileUpload
                        onFilesSelected={() => undefined}
                        multiple={false}
                        density="compact"
                        preview={{
                            src: previewUrl,
                            name: row.bankReceipt.fileName,
                            status: "uploaded",
                        }}
                    />
                    {query.isError ? (
                        <p className="text-xs text-destructive">
                            回单预览加载失败，请稍后重试。
                        </p>
                    ) : null}
                </div>
            ) : (
                <p className="text-sm text-muted-foreground">
                    历史付款未留存银行回单图片
                </p>
            )}
        </section>
    )
}

function relatedDocumentsFromPayment(row: PaymentRow): RelatedDocument[] {
    const documents: RelatedDocument[] = []
    const seen = new Set<string>()
    for (const allocation of row.allocations) {
        if (allocation.sourceHref && !seen.has(`source:${allocation.sourceHref}`)) {
            seen.add(`source:${allocation.sourceHref}`)
            documents.push({
                id: `source:${allocation.sourceHref}`,
                documentType: SOURCE_TYPE_LABEL[allocation.sourceType],
                documentNumber: allocation.sourceDocumentNo,
                status: { label: "已关联", tone: "success" },
                measure: {
                    kind: "amount",
                    value: <MoneyValue value={allocation.amount} />,
                    label: "核销金额",
                },
                owner: row.supplierName,
                openAction: (
                    <Button
                        type="button"
                        size="xs"
                        variant="outline"
                        render={<Link href={allocation.sourceHref} />}
                    >
                        打开
                        <ExternalLinkIcon className="size-3.5" />
                    </Button>
                ),
            })
        }
        if (
            allocation.payableHref &&
            !seen.has(`payable:${allocation.payableAccountId}`)
        ) {
            seen.add(`payable:${allocation.payableAccountId}`)
            documents.push({
                id: `payable:${allocation.payableAccountId}`,
                documentType: "应付台账",
                documentNumber: allocation.sourceDocumentNo,
                status: { label: "已核销", tone: "success" },
                measure: {
                    kind: "amount",
                    value: <MoneyValue value={allocation.amount} />,
                    label: "核销金额",
                },
                owner: row.supplierName,
                openAction: (
                    <Button
                        type="button"
                        size="xs"
                        variant="outline"
                        render={<Link href={allocation.payableHref} />}
                    >
                        打开
                    </Button>
                ),
            })
        }
    }
    if (row.reverseOfPaymentId) {
        documents.push({
            id: `payment:${row.reverseOfPaymentId}`,
            documentType: "原付款单",
            documentNumber: "查看原付款",
            status: { label: "已冲正", tone: "warning" },
            measure: {
                kind: "amount",
                value: <MoneyValue value={row.amount} />,
            },
            owner: row.supplierName,
            openAction: (
                <Button
                    type="button"
                    size="xs"
                    variant="outline"
                    render={
                        <Link href={paymentPreviewHref(row.reverseOfPaymentId)} />
                    }
                >
                    打开
                </Button>
            ),
        })
    }
    return documents
}

function AllocationLineItem({
    allocation,
}: {
    allocation: PaymentAllocationLine
}) {
    return (
        <li className="rounded-xl border px-3 py-2 text-sm">
            <div className="flex justify-between gap-2">
                <span>
                    <Badge
                        variant={
                            allocation.action === "REVERSE"
                                ? "warning"
                                : "secondary"
                        }
                    >
                        {ALLOCATION_ACTION_LABEL[allocation.action]}
                    </Badge>{" "}
                    {SOURCE_TYPE_LABEL[allocation.sourceType]}{" "}
                    {allocation.sourceDocumentNo}
                </span>
                <MoneyValue value={allocation.amount} />
            </div>
            <div className="mt-1 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                <span>
                    {formatDateTime(
                        allocation.occurredAt,
                        "full",
                        "passthrough",
                    )}
                    {allocation.reverseOfAllocationId ? " · 冲减原分配" : null}
                </span>
                {allocation.sourceHref ? (
                    <Button
                        type="button"
                        size="xs"
                        variant="ghost"
                        render={<Link href={allocation.sourceHref} />}
                    >
                        查看来源
                    </Button>
                ) : null}
                {allocation.payableHref ? (
                    <Button
                        type="button"
                        size="xs"
                        variant="ghost"
                        render={<Link href={allocation.payableHref} />}
                    >
                        查看应付
                    </Button>
                ) : null}
            </div>
        </li>
    )
}

function Fact({
    label,
    value,
    mono,
}: {
    label: string
    value: React.ReactNode
    mono?: boolean
}) {
    return (
        <div>
            <div className="text-xs text-muted-foreground">{label}</div>
            <div
                className={
                    mono ? "num text-sm font-medium" : "text-sm font-medium"
                }
            >
                {value}
            </div>
        </div>
    )
}
