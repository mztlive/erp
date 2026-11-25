"use client"

import * as React from "react"
import { z } from "zod"

import { todayDate } from "@/lib/datetime"

import {
    MoneyValue,
    RateValue,
    detailWorkspaceStyles,
} from "@/components/business"
import { PurchaseOrderDetailSection as DetailRecordSection } from "./purchase-order-detail-section"
import {
    EVIDENCE_FILE_MAX_BYTES,
    EVIDENCE_FILE_TYPES,
    useAppForm,
} from "@/components/form"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { downloadFileAsset } from "@/features/file-assets/api/file-assets"
import { expenseLineName } from "@/features/purchase-orders/api/purchase-order-mapping"
import { useConfirmExpenseOccurrenceMutation } from "@/features/purchase-orders/hooks/queries"
import { inputTaxRatePercent } from "@/features/purchase-orders/lib/input-tax-rate"
import type { PurchaseOrderCenterView } from "@/features/purchase-orders/types"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { compareDecimal, sumFixed } from "@/lib/fixed-decimal"
import { hasPermission } from "@/lib/permissions"

type ExpenseLine = PurchaseOrderCenterView["currentContent"]["lines"][number]

const CONFIRMABLE_STATUSES = new Set(["EFFECTIVE", "PARTIAL", "COMPLETED"])

const confirmSchema = z.object({
    occurredOn: z
        .string()
        .regex(/^\d{4}-\d{2}-\d{2}$/, "请选择发生日期")
        .refine((value) => value <= todayDate(), "发生日期不能晚于今天"),
    evidence: z
        .custom<File | null>(
            (value) =>
                value === null ||
                (typeof File !== "undefined" && value instanceof File),
            "凭证文件无效",
        )
        .superRefine((file, context) => {
            if (!file) {
                context.addIssue({ code: "custom", message: "请上传凭证" })
            } else if (
                !(EVIDENCE_FILE_TYPES as readonly string[]).includes(file.type)
            ) {
                context.addIssue({
                    code: "custom",
                    message: "凭证仅支持 PDF、JPG、PNG 或 WebP",
                })
            } else if (file.size === 0 || file.size > EVIDENCE_FILE_MAX_BYTES) {
                context.addIssue({
                    code: "custom",
                    message: "请选择不超过 5 MB 的非空文件",
                })
            }
        }),
})

/**
 * 采购单「费用确认」：逐条费用行展示是否已发生；采购确认发生并上传凭证后计入实际经营盈亏。
 * 只在采购单含费用行时出现；确认需 `purchase_order:confirm_expense` 且采购单已生效。
 */
export function PurchaseOrderExpenseSection({
    order,
    costMasked,
}: {
    order: PurchaseOrderCenterView
    costMasked: boolean
}) {
    const account = useAccountProfileQuery()
    const canConfirm =
        hasPermission(
            account.data?.permissions,
            "purchase_order:confirm_expense",
        ) && CONFIRMABLE_STATUSES.has(order.identity.status)
    const canDownload = hasPermission(
        account.data?.permissions,
        "file_asset:preview",
    )
    const [confirming, setConfirming] = React.useState<ExpenseLine | null>(null)
    const lines = order.currentContent.lines.filter((line) => line.expense)
    if (lines.length === 0) return null
    const expenseOnly = lines.length === order.currentContent.lines.length
    const expenseSubtotal = expenseOnly
        ? order.currentContent.totals.gross
        : sumFixed(
              lines.map((line) => line.grossAmount),
              { maxScale: 2, outputScale: 2 },
          )
    const pending = lines.filter(
        (line) =>
            !line.expenseOccurrence &&
            compareDecimal(line.grossAmount, "0", 2) > 0,
    ).length
    return (
        <DetailRecordSection
            width="full"
            title="费用确认"
            count={lines.length}
            description={
                pending > 0 ? `${pending} 条待确认发生` : "无待确认费用"
            }
        >
            <p className="rounded-lg bg-muted/35 px-4 py-2.5 text-xs text-muted-foreground">
                {pending > 0
                    ? "费用发生后上传运单、签收单或账单凭证。确认后计入来源销售单的实际经营盈亏，该费用行不能再改单修改。"
                    : "费用已确认发生或已移除，无需继续确认。"}
            </p>
            <div
                data-slot="detail-inline-table"
                className={detailWorkspaceStyles.dataTableWrapper}
            >
                <Table>
                    <TableHeader>
                        <TableRow>
                            <TableHead>费用</TableHead>
                            <TableHead scope="col" data-align="end">
                                进项税率
                            </TableHead>
                            <TableHead data-align="end">含税金额</TableHead>
                            <TableHead>状态</TableHead>
                            <TableHead data-align="end">操作</TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {lines.map((line) => (
                            <ExpenseRow
                                key={line.lineId}
                                order={order}
                                line={line}
                                costMasked={costMasked}
                                showTaxAmount={expenseOnly}
                                canConfirm={canConfirm}
                                canDownload={canDownload}
                                onConfirm={() => setConfirming(line)}
                            />
                        ))}
                    </TableBody>
                </Table>
            </div>
            <div className="flex flex-wrap items-baseline justify-end gap-x-4 gap-y-1 pt-1 text-xs text-muted-foreground">
                <span className="num">共 {lines.length} 项</span>
                <span>
                    {expenseOnly ? "含税合计" : "费用含税小计"}{" "}
                    <span className="num text-body-compact font-semibold text-foreground">
                        {costMasked ? (
                            "•••"
                        ) : (
                            <MoneyValue value={expenseSubtotal} />
                        )}
                    </span>
                </span>
            </div>
            <ConfirmExpenseDialog
                order={order}
                line={confirming}
                onOpenChange={(open) => {
                    if (!open) setConfirming(null)
                }}
            />
        </DetailRecordSection>
    )
}

function ExpenseRow({
    order,
    line,
    costMasked,
    canConfirm,
    showTaxAmount,
    canDownload,
    onConfirm,
}: {
    order: PurchaseOrderCenterView
    line: ExpenseLine
    costMasked: boolean
    canConfirm: boolean
    showTaxAmount: boolean
    canDownload: boolean
    onConfirm: () => void
}) {
    const expense = line.expense
    if (!expense) return null
    const key = toAutomationIdSegment(
        `${order.identity.purchaseOrderId}-${expense.expenseLineId}`,
    )
    const removed = compareDecimal(line.grossAmount, "0", 2) <= 0
    const occurrence = line.expenseOccurrence
    return (
        <TableRow>
            <TableCell className="max-w-80 whitespace-normal">
                <div className="font-medium">{expenseLineName(expense)}</div>
                <div className="text-xs text-muted-foreground">
                    {expense.salesOrderLineId
                        ? "归属指定销售明细"
                        : "归属整张销售单"}
                </div>
            </TableCell>
            <TableCell data-align="end">
                {costMasked ? (
                    <span
                        className="text-muted-foreground"
                        aria-label="税率已隐藏"
                    >
                        •••
                    </span>
                ) : (
                    <RateValue
                        value={inputTaxRatePercent(line.inputTaxRate)}
                        precision={2}
                    />
                )}
            </TableCell>
            <TableCell className="text-right">
                <div className="font-medium">
                    {costMasked ? (
                        "•••"
                    ) : (
                        <MoneyValue value={line.grossAmount} />
                    )}
                </div>
                {showTaxAmount ? (
                    <div className="mt-1 text-xs text-muted-foreground">
                        税额{" "}
                        {costMasked ? (
                            "•••"
                        ) : (
                            <MoneyValue value={line.taxAmount} />
                        )}
                    </div>
                ) : null}
            </TableCell>
            <TableCell>
                {occurrence ? (
                    <StatusBadge
                        tone="success"
                        label={`已发生 · ${occurrence.occurredOn}`}
                    />
                ) : removed ? (
                    <StatusBadge tone="neutral" label="已移除" />
                ) : (
                    <StatusBadge tone="warning" label="待发生" />
                )}
            </TableCell>
            <TableCell data-align="end">
                {occurrence && canDownload ? (
                    <Button
                        id={`procurement-orders-expense-evidence-${key}`}
                        type="button"
                        size="sm"
                        variant="ghost"
                        className={detailWorkspaceStyles.inlineLinkButton}
                        onClick={() => {
                            void downloadFileAsset(
                                occurrence.evidenceAttachmentId,
                                `${expense.description}-发生凭证`,
                            ).catch(() => undefined)
                        }}
                    >
                        下载凭证
                    </Button>
                ) : !occurrence && !removed && canConfirm ? (
                    <Button
                        id={`procurement-orders-expense-confirm-${key}`}
                        type="button"
                        size="sm"
                        variant="outline"
                        onClick={onConfirm}
                    >
                        确认发生
                    </Button>
                ) : null}
            </TableCell>
        </TableRow>
    )
}

function ConfirmExpenseDialog({
    order,
    line,
    onOpenChange,
}: {
    order: PurchaseOrderCenterView
    line: ExpenseLine | null
    onOpenChange: (open: boolean) => void
}) {
    const mutation = useConfirmExpenseOccurrenceMutation(
        order.identity.purchaseOrderId,
    )
    const [failure, setFailure] = React.useState<string | null>(null)
    const defaults = () => ({
        occurredOn: todayDate(),
        evidence: null as File | null,
    })
    const form = useAppForm({
        defaultValues: defaults(),
        validators: { onChange: confirmSchema },
        onSubmit: async ({ value }) => {
            const expenseLineId = line?.expense?.expenseLineId
            if (!expenseLineId || !value.evidence) return
            setFailure(null)
            try {
                await mutation.mutateAsync({
                    expenseLineId,
                    value: {
                        occurredOn: value.occurredOn,
                        evidence: value.evidence,
                    },
                })
                onOpenChange(false)
            } catch (error) {
                setFailure(getErrorMessage(error, "确认失败，请稍后重试。"))
            }
        },
    })
    const openId = line?.expense?.expenseLineId ?? null
    const lastOpenId = React.useRef<string | null>(null)
    React.useEffect(() => {
        if (openId && lastOpenId.current !== openId) {
            form.reset(defaults())
            setFailure(null)
        }
        lastOpenId.current = openId
    }, [form, openId])

    return (
        <Dialog
            open={Boolean(line)}
            onOpenChange={(next) => {
                if (!next && mutation.isPending) return
                onOpenChange(next)
            }}
        >
            <DialogContent
                size="md"
                closeButtonId="procurement-orders-expense-confirm-close"
                showCloseButton={!mutation.isPending}
            >
                <DialogHeader>
                    <DialogTitle>确认费用已发生</DialogTitle>
                    <DialogDescription>
                        {line?.expense
                            ? `「${expenseLineName(line.expense)}」确认后按 ${line.grossAmount} 元计入实际经营盈亏，之后不能再改单修改。`
                            : null}
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="flex flex-col gap-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <form.AppField
                        name="occurredOn"
                        children={(field) => (
                            <field.DateField
                                id="procurement-orders-expense-confirm-occurred-on"
                                label="发生日期"
                                required
                                clearable={false}
                                disabledDates={{
                                    after: new Date(`${todayDate()}T00:00:00`),
                                }}
                            />
                        )}
                    />
                    <form.AppField
                        name="evidence"
                        children={(field) => (
                            <field.EvidenceUploadField
                                id="procurement-orders-expense-confirm-evidence"
                                label="发生凭证"
                                required
                                uploadLabel="上传运单、签收单、服务记录或账单"
                            />
                        )}
                    />
                    {failure ? (
                        <p
                            role="alert"
                            className="text-body-compact text-destructive"
                        >
                            {failure}
                        </p>
                    ) : null}
                    <DialogFooter>
                        <Button
                            id="procurement-orders-expense-confirm-cancel"
                            type="button"
                            variant="outline"
                            disabled={mutation.isPending}
                            onClick={() => onOpenChange(false)}
                        >
                            取消
                        </Button>
                        <form.AppForm>
                            <form.SubmitButton
                                id="procurement-orders-expense-confirm-submit"
                                label="确认发生"
                                pendingLabel="确认中…"
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
