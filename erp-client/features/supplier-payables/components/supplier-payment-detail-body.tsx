"use client"

import * as React from "react"
import Link from "next/link"
import {
    CircleHelpIcon,
    ExternalLinkIcon,
    GitCompareArrowsIcon,
    ImageIcon,
    InfoIcon,
    LandmarkIcon,
    Link2Icon,
    Undo2Icon,
    type LucideIcon,
} from "lucide-react"

import {
    BusinessStatusBadge,
    MoneyValue,
    RelatedDocumentList,
    type RelatedDocument,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field"
import { FileUpload } from "@/components/ui/file-upload"
import { Input } from "@/components/ui/input"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { useSupplierPaymentBankReceiptQuery } from "@/features/supplier-payables/hooks/queries"
import {
    paymentPreviewHref,
    paymentRelatedDocumentRefs,
    paymentReversalPreviewHref,
    sourceDocumentHref,
    sourceDocumentOpenLabel,
    type PaymentRelatedDocumentRef,
} from "@/features/supplier-payables/lib/related-documents"
import {
    SOURCE_TYPE_LABEL,
    type PaymentAllocationLine,
    type PaymentRow,
} from "@/features/supplier-payables/types"
import { formatDateTime } from "@/lib/datetime"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"

const PAYMENT_DETAIL_SECTIONS = [
    { id: "basic", label: "基本信息", icon: InfoIcon },
    { id: "recipient", label: "收款信息", icon: LandmarkIcon },
    { id: "receipt", label: "银行回单", icon: ImageIcon },
    { id: "reversals", label: "冲正记录", icon: Undo2Icon },
    { id: "related", label: "关联单据", icon: Link2Icon },
    { id: "allocations", label: "付款去向", icon: GitCompareArrowsIcon },
] as const

type PaymentDetailSectionId = (typeof PAYMENT_DETAIL_SECTIONS)[number]["id"]

/**
 * 供应商付款事实详情；普通付款登记即过账，不包含独立审批区。
 * 按分区展示，避免一张长页把核销、回单和收款信息叠在一起。
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
        <Tabs
            defaultValue="basic"
            orientation="vertical"
            className="flex min-h-0 min-w-0 flex-1 flex-row gap-0"
        >
            <TabsList
                aria-label="付款详情分区"
                className="h-auto w-44 shrink-0 flex-col items-stretch justify-start gap-1 rounded-none bg-transparent p-3 sm:w-52"
            >
                {PAYMENT_DETAIL_SECTIONS.map((section) => (
                    <SectionTab
                        key={section.id}
                        value={section.id}
                        icon={section.icon}
                    >
                        {section.label}
                    </SectionTab>
                ))}
            </TabsList>
            <SectionPanel value="basic" icon={InfoIcon} title="基本信息">
                {posted ? (
                    <Alert variant="info">
                        <AlertTitle>已过账记录只读</AlertTitle>
                        <AlertDescription>
                            已过账记录不可编辑、不可删除；纠错仅能追加冲正。
                        </AlertDescription>
                    </Alert>
                ) : null}
                <div className="grid grid-cols-1 gap-x-6 gap-y-5 sm:grid-cols-2">
                    <ReadOnlyField
                        id="supplier-payables-payment-detail-field-payment-no-input"
                        label="付款单号"
                        value={row.paymentNo}
                        description="过账后不可改号"
                        mono
                    />
                    <ReadOnlyField
                        id="supplier-payables-payment-detail-field-supplier-name-input"
                        label="供应商"
                        value={row.supplierName}
                        description="本付款对应的往来供应商"
                    />
                    <ReadOnlyField
                        id="supplier-payables-payment-detail-field-paid-at-input"
                        label="付款时间"
                        value={formatDateTime(
                            row.paidAt,
                            "full",
                            "passthrough",
                        )}
                        description="银行实际付款时间"
                        mono
                    />
                    <ReadOnlyField
                        id="supplier-payables-payment-detail-field-amount-input"
                        label="付款金额"
                        value={
                            <MoneyValue value={row.amount} taxBasis="gross" />
                        }
                        description="本单含税付款总额"
                    />
                    <ReadOnlyField
                        id="supplier-payables-payment-detail-field-bank-reference-input"
                        label="银行流水号"
                        value={row.bankReferenceMasked}
                        description="用于对账的银行引用"
                        mono
                        className="sm:col-span-2"
                    />
                    <ReadOnlyField
                        id="supplier-payables-payment-detail-field-allocated-total-input"
                        label="已付款"
                        value={
                            <MoneyValue
                                value={row.allocatedTotal}
                                taxBasis="gross"
                            />
                        }
                        description="已经付给具体应付的金额"
                    />
                    <ReadOnlyField
                        id="supplier-payables-payment-detail-field-unallocated-amount-input"
                        label="未付款"
                        value={
                            <MoneyValue
                                value={row.unallocatedAmount}
                                taxBasis="gross"
                            />
                        }
                        description="还没指定付给哪一笔应付的余额"
                    />
                </div>
            </SectionPanel>
            <SectionPanel
                value="recipient"
                icon={LandmarkIcon}
                title="收款信息"
            >
                {row.paymentRecipient ? (
                    <div className="grid grid-cols-1 gap-x-6 gap-y-5 sm:grid-cols-2">
                        <ReadOnlyField
                            label="收款户名"
                            value={row.paymentRecipient.accountName}
                            description="供应商收款账户户名"
                        />
                        <ReadOnlyField
                            label="收款银行"
                            value={[
                                row.paymentRecipient.bankName,
                                row.paymentRecipient.bankBranchName,
                            ]
                                .filter(Boolean)
                                .join(" · ")}
                            description="开户银行与网点"
                        />
                        <ReadOnlyField
                            label="收款账号"
                            value={row.paymentRecipient.accountNumberMasked}
                            description="脱敏账号，完整号码需受控揭示"
                            mono
                            className="sm:col-span-2"
                        />
                    </div>
                ) : (
                    <p className="text-sm text-muted-foreground">
                        未记录收款账户
                    </p>
                )}
            </SectionPanel>
            <SectionPanel value="receipt" icon={ImageIcon} title="银行回单">
                <BankReceiptPreview row={row} />
            </SectionPanel>
            <SectionPanel value="reversals" icon={Undo2Icon} title="冲正记录">
                <PaymentReversalHistory row={row} />
            </SectionPanel>
            <SectionPanel value="related" icon={Link2Icon} title="关联单据">
                <p className="text-sm text-muted-foreground">
                    应付按来源采购单或结算单记账。查看应付在本页打开，打开采购单会进入该单据。
                </p>
                <RelatedDocumentList
                    documents={relatedDocumentsFromPayment(row, onOpenPayable)}
                    emptyContent="此付款尚未付到任何应付。"
                />
            </SectionPanel>
            <SectionPanel
                value="allocations"
                icon={GitCompareArrowsIcon}
                title="付款去向"
            >
                <p className="text-sm text-muted-foreground">
                    这笔付款分别付给了哪些应付、各付了多少。点来源单进入采购单或结算单。冲正不会改已经记下的行，只会再记一笔冲减。
                </p>
                {row.allocations.length === 0 ? (
                    <p className="text-sm text-muted-foreground">
                        尚未付到任何应付
                    </p>
                ) : (
                    <ul className="flex flex-col gap-2">
                        {row.allocations.map((allocation) => (
                            <AllocationLineItem
                                key={allocation.allocationId}
                                allocation={allocation}
                            />
                        ))}
                    </ul>
                )}
            </SectionPanel>
        </Tabs>
    )
}

function SectionTab({
    value,
    icon: Icon,
    children,
}: {
    value: PaymentDetailSectionId
    icon: LucideIcon
    children: React.ReactNode
}) {
    return (
        <TabsTrigger
            id={`supplier-payables-payment-detail-tab-${value}`}
            value={value}
            className={cn(
                "h-10 flex-none justify-start gap-2 rounded-lg px-3 py-2 text-muted-foreground after:hidden",
                "data-active:bg-muted data-active:font-medium data-active:text-foreground",
                "before:absolute before:inset-y-1.5 before:left-0 before:w-0.5 before:rounded-full before:bg-transparent data-active:before:bg-primary",
            )}
        >
            <Icon />
            {children}
        </TabsTrigger>
    )
}

function SectionPanel({
    value,
    icon: Icon,
    title,
    children,
}: {
    value: PaymentDetailSectionId
    icon: LucideIcon
    title: string
    children: React.ReactNode
}) {
    return (
        <TabsContent
            value={value}
            className="min-h-0 flex-1 overflow-y-auto p-6"
        >
            <div className="mb-5 flex items-center gap-2">
                <span className="flex size-6 items-center justify-center rounded-full border border-border text-muted-foreground">
                    <Icon className="size-3.5" aria-hidden />
                </span>
                <h3 className="text-sm font-medium">{title}</h3>
            </div>
            <div className="flex flex-col gap-5">{children}</div>
        </TabsContent>
    )
}

function ReadOnlyField({
    label,
    value,
    description,
    className,
    mono = false,
    id,
}: {
    label: string
    value: React.ReactNode
    description?: string
    className?: string
    mono?: boolean
    id?: string
}) {
    const fallbackId = `supplier-payables-payment-detail-field-${toAutomationIdSegment(label)}-input`
    const fieldId = id ?? fallbackId
    const isText = typeof value === "string"

    return (
        <Field className={className}>
            <FieldLabel htmlFor={isText ? fieldId : undefined}>
                {label}
            </FieldLabel>
            {isText ? (
                <Input
                    id={fieldId}
                    readOnly
                    value={value}
                    className={cn(
                        "bg-muted hover:border-input hover:bg-muted focus-visible:border-input focus-visible:ring-0",
                        mono && "num font-mono",
                    )}
                />
            ) : (
                <div
                    className={cn(
                        "flex min-h-control items-center rounded-lg border border-input bg-muted px-3 py-1.5 text-sm",
                        mono && "num font-mono",
                    )}
                >
                    {value}
                </div>
            )}
            {description ? (
                <FieldDescription className="flex items-start justify-between gap-2">
                    <span>{description}</span>
                    <CircleHelpIcon
                        className="mt-0.5 size-3.5 shrink-0 text-muted-foreground/70"
                        aria-hidden
                    />
                </FieldDescription>
            ) : null}
        </Field>
    )
}

/** 原付款上的冲正追踪记录；审批中的记录不参与付款金额计算。 */
function PaymentReversalHistory({ row }: { row: PaymentRow }) {
    return (
        <>
            <p className="text-sm text-muted-foreground">
                待审批记录仅用于跟踪；审批通过后才改变原付款事实。
            </p>
            {row.relatedReversals.length === 0 ? (
                <p className="text-sm text-muted-foreground">尚未发起冲正</p>
            ) : (
                <ul className="flex flex-col gap-2">
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
                                    id={`supplier-payables-payment-detail-reversal-${toAutomationIdSegment(reversal.reversalId)}-open`}
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
        </>
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

    if (!row.bankReceipt) {
        return (
            <p className="text-sm text-muted-foreground">
                历史付款未留存银行回单图片
            </p>
        )
    }

    return (
        <div className="max-w-md">
            <FileUpload
                idPrefix="supplier-payables-payment-detail-receipt-preview"
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
                <p className="mt-2 text-xs text-destructive">
                    回单预览加载失败，请稍后重试。
                </p>
            ) : null}
        </div>
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
 *
 * @param refItem 去重后的关联单据。
 * @param reverseOfPaymentId 冲正场景下的原付款主键。
 * @param onOpenPayable 在当前页打开应付预览。
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
    const payableAccountId = refItem.payableAccountId
    if (refItem.kind === "original-payment" && reverseOfPaymentId) {
        return (
            <Button
                id={`supplier-payables-payment-detail-related-${toAutomationIdSegment(refItem.id)}-open-original`}
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
            {payableAccountId && onOpenPayable ? (
                <Button
                    id={`supplier-payables-payment-detail-related-${toAutomationIdSegment(refItem.id)}-open-payable`}
                    type="button"
                    size="xs"
                    variant="outline"
                    onClick={() => onOpenPayable(payableAccountId)}
                >
                    查看应付
                </Button>
            ) : null}
            {refItem.sourceHref ? (
                <Button
                    id={`supplier-payables-payment-detail-related-${toAutomationIdSegment(refItem.id)}-open-source`}
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
 * 单笔付款去向。说明付给了哪张来源单、付了多少，并可跳转到采购单或结算单。
 *
 * @param allocation 付款核销行。
 */
function AllocationLineItem({
    allocation,
}: {
    allocation: PaymentAllocationLine
}) {
    const verb = allocation.action === "REVERSE" ? "冲减" : "付给"
    const href =
        allocation.sourceHref ??
        sourceDocumentHref(allocation.sourceType, allocation.sourceDocumentId)
    return (
        <li className="rounded-xl border px-3 py-2 text-sm">
            <div className="flex justify-between gap-2">
                <span>
                    {verb} {SOURCE_TYPE_LABEL[allocation.sourceType]}{" "}
                    {allocation.sourceDocumentNo}
                </span>
                <MoneyValue value={allocation.amount} />
            </div>
            <div className="mt-1 flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
                <span>
                    {formatDateTime(
                        allocation.occurredAt,
                        "full",
                        "passthrough",
                    )}
                    {allocation.reverseOfAllocationId
                        ? " · 冲减此前一笔"
                        : null}
                </span>
                {href ? (
                    <Button
                        id={`supplier-payables-payment-detail-allocation-${toAutomationIdSegment(allocation.allocationId)}-open`}
                        type="button"
                        size="xs"
                        variant="outline"
                        render={<Link href={href} />}
                    >
                        {sourceDocumentOpenLabel(allocation.sourceType)}
                        <ExternalLinkIcon data-icon="inline-end" />
                    </Button>
                ) : null}
            </div>
        </li>
    )
}
