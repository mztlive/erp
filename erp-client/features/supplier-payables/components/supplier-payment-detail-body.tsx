"use client"

import * as React from "react"
import Link from "next/link"
import { ExternalLinkIcon } from "lucide-react"

import {
    BusinessStatusBadge,
    MoneyValue,
    RelatedDocumentList,
    type RelatedDocument,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    DescriptionDetails,
    DescriptionItem,
    DescriptionList,
    DescriptionTerm,
} from "@/components/ui/description-list"
import { FileUpload } from "@/components/ui/file-upload"
import { useSupplierPaymentBankReceiptQuery } from "@/features/supplier-payables/hooks/queries"
import {
    paymentPreviewHref,
    paymentRelatedDocumentRefs,
    paymentReversalPreviewHref,
    sourceDocumentOpenLabel,
    type PaymentRelatedDocumentRef,
} from "@/features/supplier-payables/lib/related-documents"
import type {
    PaymentAllocationLine,
    PaymentRow,
} from "@/features/supplier-payables/types"
import {
    ALLOCATION_ACTION_LABEL,
    SOURCE_TYPE_LABEL,
} from "@/features/supplier-payables/types"
import { formatDateTime } from "@/lib/datetime"

/**
 * 供应商付款事实详情；普通付款登记即过账，不包含独立审批区。
 *
 * @param row 已加载的付款事实。
 * @param onOpenPayable 在当前页打开应付预览，不跳到台账列表。
 */
export function SupplierPaymentDetailBody({
    row,
    onOpenPayable,
}: {
    row: PaymentRow
    onOpenPayable?: (payableAccountId: string) => void
}) {
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
            <DescriptionList columns="two">
                <DescriptionItem>
                    <DescriptionTerm>付款单号</DescriptionTerm>
                    <DescriptionDetails className="num font-medium">
                        {row.paymentNo}
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>供应商</DescriptionTerm>
                    <DescriptionDetails className="font-medium">
                        {row.supplierName}
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>付款时间</DescriptionTerm>
                    <DescriptionDetails className="num font-medium">
                        {formatDateTime(row.paidAt, "full", "passthrough")}
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>付款金额</DescriptionTerm>
                    <DescriptionDetails className="font-medium">
                        <MoneyValue value={row.amount} taxBasis="gross" />
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>银行流水号</DescriptionTerm>
                    <DescriptionDetails className="num break-all font-medium">
                        {row.bankReferenceMasked}
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>已付款</DescriptionTerm>
                    <DescriptionDetails className="font-medium">
                        <MoneyValue
                            value={row.allocatedTotal}
                            taxBasis="gross"
                        />
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>未付款</DescriptionTerm>
                    <DescriptionDetails className="font-medium">
                        <MoneyValue
                            value={row.unallocatedAmount}
                            taxBasis="gross"
                        />
                    </DescriptionDetails>
                </DescriptionItem>
            </DescriptionList>
            <p className="text-xs text-muted-foreground">
                已付款是这笔钱已经付给具体应付的金额；未付款是还没指定付给哪一笔应付的余额。
            </p>
            {row.paymentRecipient ? (
                <section>
                    <h4 className="mb-2 text-sm font-semibold">收款信息</h4>
                    <DescriptionList columns="two">
                        <DescriptionItem>
                            <DescriptionTerm>收款户名</DescriptionTerm>
                            <DescriptionDetails className="break-words font-medium">
                                {row.paymentRecipient.accountName}
                            </DescriptionDetails>
                        </DescriptionItem>
                        <DescriptionItem>
                            <DescriptionTerm>收款银行</DescriptionTerm>
                            <DescriptionDetails className="break-words font-medium">
                                {[
                                    row.paymentRecipient.bankName,
                                    row.paymentRecipient.bankBranchName,
                                ]
                                    .filter(Boolean)
                                    .join(" · ")}
                            </DescriptionDetails>
                        </DescriptionItem>
                        <DescriptionItem className="sm:col-span-2">
                            <DescriptionTerm>收款账号</DescriptionTerm>
                            <DescriptionDetails>
                                <code className="num block break-all font-mono text-sm font-medium">
                                    {
                                        row.paymentRecipient
                                            .accountNumberMasked
                                    }
                                </code>
                            </DescriptionDetails>
                        </DescriptionItem>
                    </DescriptionList>
                </section>
            ) : null}
            <BankReceiptPreview row={row} />
            <PaymentReversalHistory row={row} />
            <section>
                <h4 className="mb-1 text-sm font-semibold">关联单据</h4>
                <p className="mb-2 text-xs text-muted-foreground">
                    应付按来源采购单或结算单记账。查看应付在本页打开，打开采购单会进入该单据。
                </p>
                <RelatedDocumentList
                    documents={relatedDocumentsFromPayment(row, onOpenPayable)}
                    emptyContent="此付款尚未付到任何应付。"
                />
            </section>
            <section>
                <h4 className="mb-1 text-sm font-semibold">付款去向</h4>
                <p className="mb-2 text-xs text-muted-foreground">
                    这笔付款分别付给了哪些应付、各付了多少。冲正不会改已经记下的行，只会再记一笔冲减。
                </p>
                {row.allocations.length === 0 ? (
                    <p className="text-sm text-muted-foreground">
                        尚未付到任何应付
                    </p>
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

/** 原付款上的冲正追踪记录；审批中的记录不参与付款金额计算。 */
function PaymentReversalHistory({ row }: { row: PaymentRow }) {
    return (
        <section>
            <h4 className="mb-1 text-sm font-semibold">冲正记录</h4>
            <p className="mb-2 text-xs text-muted-foreground">
                待审批记录仅用于跟踪；审批通过后才改变原付款事实。
            </p>
            {row.relatedReversals.length === 0 ? (
                <p className="text-sm text-muted-foreground">尚未发起冲正</p>
            ) : (
                <ul className="space-y-2">
                    {row.relatedReversals.map((reversal) => (
                        <li
                            key={reversal.reversalId}
                            className="rounded-xl border px-3 py-2 text-sm"
                        >
                            <div className="flex flex-wrap items-center justify-between gap-2">
                                <span className="flex flex-wrap items-center gap-2">
                                    <span className="num font-medium">
                                        {reversal.reversalNo}
                                    </span>
                                    <BusinessStatusBadge
                                        context="list"
                                        label={reversal.statusLabel}
                                        tone={reversal.statusTone}
                                    />
                                </span>
                                <MoneyValue value={reversal.amount} />
                            </div>
                            <div className="mt-1 flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
                                <span>
                                    {reversal.reasonText} ·{" "}
                                    {formatDateTime(
                                        reversal.occurredAt,
                                        "full",
                                        "passthrough",
                                    )}
                                </span>
                                <Button
                                    type="button"
                                    size="xs"
                                    variant="ghost"
                                    render={
                                        <Link
                                            href={paymentReversalPreviewHref(
                                                reversal.reversalId,
                                            )}
                                        />
                                    }
                                >
                                    查看进度
                                </Button>
                            </div>
                        </li>
                    ))}
                </ul>
            )}
        </section>
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

/**
 * 把付款核销收成关联单据，并接上原地打开应付、打开来源单的动作。
 *
 * @param row 付款事实。
 * @param onOpenPayable 在当前页打开应付预览。
 */
function relatedDocumentsFromPayment(
    row: PaymentRow,
    onOpenPayable?: (payableAccountId: string) => void,
): RelatedDocument[] {
    return paymentRelatedDocumentRefs(row).map((ref) => ({
        id: ref.id,
        documentType: ref.documentType,
        documentNumber: ref.documentNumber,
        status: { label: ref.statusLabel, tone: ref.statusTone },
        measure: {
            kind: "amount",
            value: <MoneyValue value={ref.amount} />,
            label: "付款金额",
        },
        owner: row.supplierName,
        openAction: (
            <RelatedDocumentActions
                refItem={ref}
                reverseOfPaymentId={row.reverseOfPaymentId}
                onOpenPayable={onOpenPayable}
            />
        ),
    }))
}

/**
 * 关联单据动作：应付在本页打开；采购单/结算单进入对象中心。
 */
function RelatedDocumentActions({
    refItem,
    reverseOfPaymentId,
    onOpenPayable,
}: {
    refItem: PaymentRelatedDocumentRef
    reverseOfPaymentId?: string
    onOpenPayable?: (payableAccountId: string) => void
}) {
    if (refItem.kind === "original-payment" && reverseOfPaymentId) {
        return (
            <Button
                type="button"
                size="xs"
                variant="outline"
                render={<Link href={paymentPreviewHref(reverseOfPaymentId)} />}
            >
                打开
            </Button>
        )
    }

    return (
        <div className="flex flex-wrap justify-end gap-1">
            {refItem.payableAccountId && onOpenPayable ? (
                <Button
                    type="button"
                    size="xs"
                    variant="outline"
                    onClick={() => onOpenPayable(refItem.payableAccountId!)}
                >
                    查看应付
                </Button>
            ) : null}
            {refItem.sourceHref ? (
                <Button
                    type="button"
                    size="xs"
                    variant="outline"
                    render={<Link href={refItem.sourceHref} />}
                >
                    {sourceDocumentOpenLabel(refItem.sourceType)}
                    <ExternalLinkIcon className="size-3.5" />
                </Button>
            ) : null}
        </div>
    )
}

/**
 * 单笔付款去向。只说明付给了哪张来源单、付了多少，导航放在关联单据。
 *
 * @param allocation 付款核销行。
 */
function AllocationLineItem({
    allocation,
}: {
    allocation: PaymentAllocationLine
}) {
    const verb = allocation.action === "REVERSE" ? "冲减" : "付给"
    return (
        <li className="rounded-xl border px-3 py-2 text-sm">
            <div className="flex justify-between gap-2">
                <span>
                    {verb} {SOURCE_TYPE_LABEL[allocation.sourceType]}{" "}
                    {allocation.sourceDocumentNo}
                </span>
                <MoneyValue value={allocation.amount} />
            </div>
            <div className="mt-1 text-xs text-muted-foreground">
                {formatDateTime(allocation.occurredAt, "full", "passthrough")}
                {allocation.reverseOfAllocationId
                    ? " · 冲减此前一笔"
                    : null}
            </div>
        </li>
    )
}
