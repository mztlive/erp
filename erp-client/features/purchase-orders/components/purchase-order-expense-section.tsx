"use client"

import * as React from "react"
import { z } from "zod"

import { MoneyValue, RateValue } from "@/components/business"
import { PurchaseOrderDetailSection as DetailRecordSection } from "./purchase-order-detail-section"
import {
    EVIDENCE_FILE_MAX_BYTES,
    EVIDENCE_FILE_TYPES,
    useAppForm,
} from "@/components/form"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
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
import type { PurchaseOrderCenterView } from "@/features/purchase-orders/types"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { compareDecimal, multiplyFixed } from "@/lib/fixed-decimal"
import { hasPermission } from "@/lib/permissions"

type ExpenseLine = PurchaseOrderCenterView["currentContent"]["lines"][number]

const CONFIRMABLE_STATUSES = new Set(["EFFECTIVE", "PARTIAL", "COMPLETED"])

/** 本地时区今天（YYYY-MM-DD）。 */
function todayDate(): string {
    const now = new Date()
    const pad = (value: number) => String(value).padStart(2, "0")
    return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`
}

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
    const pending = lines.filter(
        (line) =>
            !line.expenseOccurrence &&
            compareDecimal(line.grossAmount, "0", 2) > 0,
    ).length
    return (
        <DetailRecordSection
            title="费用确认"
            count={lines.length}
            description={
                pending > 0 ? `${pending} 条待确认发生` : "无待确认费用"
            }
        >
            <p className="rounded-lg bg-muted/40 px-4 py-3 text-sm leading-6 text-muted-foreground">
                {pending > 0
                    ? "费用发生后上传运单、签收单或账单凭证。确认后计入来源销售单的实际经营盈亏，该费用行不能再改单修改。"
                    : "费用已确认发生或已移除，无需继续确认。"}
            </p>
            <Table>
                <TableHeader>
                    <TableRow>
                        <TableHead>费用</TableHead>
                        {expenseOnly ? (
                            <TableHead className="text-right">税率</TableHead>
                        ) : null}
                        <TableHead className="text-right">含税金额</TableHead>
                        <TableHead>状态</TableHead>
                        <TableHead className="text-right">操作</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {lines.map((line) => (
                        <ExpenseRow
                            key={line.lineId}
                            order={order}
                            line={line}
                            costMasked={costMasked}
                            showTax={expenseOnly}
                            canConfirm={canConfirm}
                            canDownload={canDownload}
                            onConfirm={() => setConfirming(line)}
                        />
                    ))}
                </TableBody>
            </Table>
            {expenseOnly ? (
                <div className="flex flex-wrap items-baseline justify-end gap-3 border-t border-border/60 pt-4 text-sm">
                    <span className="text-muted-foreground">合计（含税）</span>
                    <span className="text-xl font-semibold">
                        {costMasked ? (
                            "•••"
                        ) : (
                            <MoneyValue
                                value={order.currentContent.totals.gross}
                            />
                        )}
                    </span>
                </div>
            ) : null}
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
    showTax,
    canDownload,
    onConfirm,
}: {
    order: PurchaseOrderCenterView
    line: ExpenseLine
    costMasked: boolean
    canConfirm: boolean
    showTax: boolean
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
            {showTax ? (
                <TableCell className="text-right">
                    {costMasked ? (
                        "•••"
                    ) : (
                        <RateValue
                            value={multiplyFixed(line.inputTaxRate, "100", {
                                leftMaxScale: 6,
                                rightMaxScale: 0,
                                outputScale: 2,
                            })}
                            precision={2}
                        />
                    )}
                </TableCell>
            ) : null}
            <TableCell className="text-right">
                <div className="font-medium">
                    {costMasked ? (
                        "•••"
                    ) : (
                        <MoneyValue value={line.grossAmount} />
                    )}
                </div>
                {showTax ? (
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
                    <Badge variant="secondary">
                        已发生 · {occurrence.occurredOn}
                    </Badge>
                ) : removed ? (
                    <Badge variant="neutral">已移除</Badge>
                ) : (
                    <Badge variant="warning">待发生</Badge>
                )}
            </TableCell>
            <TableCell className="text-right">
                {occurrence && canDownload ? (
                    <Button
                        id={`procurement-orders-expense-evidence-${key}`}
                        type="button"
                        size="sm"
                        variant="ghost"
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
        <Dialog open={Boolean(line)} onOpenChange={onOpenChange}>
            <DialogContent
                className="sm:max-w-lg"
                closeButtonId="procurement-orders-expense-confirm-close"
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
                        <p role="alert" className="text-sm text-destructive">
                            {failure}
                        </p>
                    ) : null}
                    <DialogFooter>
                        <Button
                            id="procurement-orders-expense-confirm-cancel"
                            type="button"
                            variant="ghost"
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
