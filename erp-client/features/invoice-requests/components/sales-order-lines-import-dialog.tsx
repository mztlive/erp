"use client"

import { useState } from "react"
import { CircleAlertIcon, RefreshCwIcon } from "lucide-react"

import { MoneyValue } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import {
    Dialog,
    DialogBody,
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
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { formatFixedDisplay, sumFixed } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"

import type { InvoiceRequestLine } from "../api"
import { useSalesOrderInvoiceLines } from "../hooks/queries"
import {
    INVOICE_QUANTITY_SCALE,
    INVOICE_REQUEST_LINE_LIMIT,
    INVOICE_UNIT_PRICE_SCALE,
    invoiceExceedsAvailable,
} from "../line-amounts"

/** 从销售单当前生效版本选择明细一键带入开票明细。 */
export function SalesOrderLinesImportDialog({
    open,
    onOpenChange,
    salesOrderId,
    availableAmount,
    existingLineCount,
    hasMeaningfulExistingLines,
    onImport,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    salesOrderId: string
    availableAmount?: string | null
    existingLineCount: number
    hasMeaningfulExistingLines: boolean
    onImport: (lines: InvoiceRequestLine[], mode: "replace" | "append") => void
}) {
    const query = useSalesOrderInvoiceLines(salesOrderId, open)
    const [deselectedIds, setDeselectedIds] = useState<ReadonlySet<string>>(
        () => new Set(),
    )

    const lines = query.data?.lines ?? []
    const selectableLines = lines.filter((line) => line.selectable)
    const selectedLines = selectableLines.filter(
        (line) => !deselectedIds.has(line.id),
    )
    const allSelected =
        selectableLines.length > 0 &&
        selectedLines.length === selectableLines.length
    const someSelected =
        selectedLines.length > 0 &&
        selectedLines.length < selectableLines.length

    const selectedTotal = selectedLines.length
        ? sumFixed(
              selectedLines
                  .map((line) => line.lineAmount)
                  .filter((amount): amount is string => amount !== null),
              { maxScale: 2, outputScale: 2 },
          )
        : null
    const exceedsAvailable = invoiceExceedsAvailable(
        selectedTotal,
        availableAmount,
    )
    const remainingSlots = Math.max(
        0,
        INVOICE_REQUEST_LINE_LIMIT - existingLineCount,
    )
    const appendOverflow =
        hasMeaningfulExistingLines && selectedLines.length > remainingSlots
    const replaceOverflow = selectedLines.length > INVOICE_REQUEST_LINE_LIMIT

    function toggleLine(id: string, checked: boolean) {
        setDeselectedIds((prev) => {
            const next = new Set(prev)
            if (checked) next.delete(id)
            else next.add(id)
            return next
        })
    }

    function toggleAll(checked: boolean) {
        if (checked) {
            setDeselectedIds(new Set())
        } else {
            setDeselectedIds(new Set(selectableLines.map((line) => line.id)))
        }
    }

    function confirmImport(mode: "replace" | "append") {
        if (selectedLines.length === 0) return
        const limit =
            mode === "append" ? remainingSlots : INVOICE_REQUEST_LINE_LIMIT
        const imported: InvoiceRequestLine[] = selectedLines
            .slice(0, limit)
            .map((line) => ({
                content: line.content,
                quantity: line.quantity,
                unit_price: line.unitPrice,
            }))
        if (imported.length === 0) return
        onImport(imported, mode)
        setDeselectedIds(new Set())
        onOpenChange(false)
    }

    return (
        <Dialog
            open={open}
            onOpenChange={(nextOpen) => {
                if (!nextOpen) setDeselectedIds(new Set())
                onOpenChange(nextOpen)
            }}
        >
            <DialogContent
                id="invoice-request-import-dialog"
                closeButtonId="invoice-request-import-dialog-close"
                size="xl"
                scroll="body"
            >
                <DialogHeader>
                    <DialogTitle>
                        从销售单带入明细
                        {query.data?.orderNo ? (
                            <span className="num ml-2 font-normal text-muted-foreground">
                                {query.data.orderNo}
                            </span>
                        ) : null}
                    </DialogTitle>
                    <DialogDescription>
                        勾选需要开票的销售单明细行；带入后可按本次开票比例调整数量、单价或开票内容。
                    </DialogDescription>
                </DialogHeader>

                <DialogBody className="space-y-3">
                    {query.isPending ? (
                        <div className="space-y-2 py-4" aria-busy="true">
                            <div className="h-9 animate-pulse rounded bg-muted" />
                            <div className="h-9 animate-pulse rounded bg-muted" />
                            <div className="h-9 animate-pulse rounded bg-muted" />
                        </div>
                    ) : query.isError ? (
                        <Alert variant="destructive">
                            <CircleAlertIcon aria-hidden="true" />
                            <AlertTitle>销售单明细读取失败</AlertTitle>
                            <AlertDescription className="flex flex-wrap items-center gap-2">
                                <span>
                                    {getErrorMessage(
                                        query.error,
                                        "请稍后重试。",
                                    )}
                                </span>
                                <Button
                                    id="invoice-request-import-retry"
                                    type="button"
                                    variant="outline"
                                    size="xs"
                                    onClick={() => void query.refetch()}
                                >
                                    <RefreshCwIcon
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                    />
                                    重试
                                </Button>
                            </AlertDescription>
                        </Alert>
                    ) : lines.length === 0 ? (
                        <p className="py-8 text-center text-xs text-muted-foreground">
                            该销售单当前版本没有可带入的商品明细。
                        </p>
                    ) : (
                        <>
                            <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
                                <span>
                                    共{" "}
                                    <span className="num font-medium text-foreground">
                                        {lines.length}
                                    </span>{" "}
                                    行，可带入{" "}
                                    <span className="num font-medium text-foreground">
                                        {selectableLines.length}
                                    </span>{" "}
                                    行，已选{" "}
                                    <span className="num font-medium text-foreground">
                                        {selectedLines.length}
                                    </span>{" "}
                                    行
                                </span>
                                {selectedTotal ? (
                                    <span className="flex items-baseline gap-1.5">
                                        <span>所选合计</span>
                                        <MoneyValue
                                            value={selectedTotal}
                                            display="exact"
                                            className="font-semibold"
                                        />
                                    </span>
                                ) : null}
                            </div>

                            {exceedsAvailable && availableAmount ? (
                                <Alert variant="warning">
                                    <CircleAlertIcon aria-hidden="true" />
                                    <AlertDescription>
                                        所选明细合计超过本单当前可申请额度（
                                        <MoneyValue
                                            value={availableAmount}
                                            display="exact"
                                            className="inline font-medium"
                                        />
                                        ），带入后请调减本次开票数量或单价。
                                    </AlertDescription>
                                </Alert>
                            ) : null}

                            {appendOverflow ? (
                                <Alert variant="warning">
                                    <CircleAlertIcon aria-hidden="true" />
                                    <AlertDescription>
                                        当前表单还可追加 {remainingSlots}{" "}
                                        行（单笔上限{" "}
                                        {INVOICE_REQUEST_LINE_LIMIT}{" "}
                                        行）；若选择“追加”，将仅带入前{" "}
                                        {remainingSlots} 行。
                                    </AlertDescription>
                                </Alert>
                            ) : null}

                            <div className="overflow-x-auto border-y border-border/70">
                                <Table className="min-w-[580px]">
                                    <TableHeader>
                                        <TableRow>
                                            <TableHead className="w-10">
                                                <Checkbox
                                                    id="invoice-request-import-toggle-all"
                                                    aria-label="全选可带入明细"
                                                    checked={allSelected}
                                                    indeterminate={someSelected}
                                                    disabled={
                                                        selectableLines.length ===
                                                        0
                                                    }
                                                    onCheckedChange={(
                                                        checked,
                                                    ) =>
                                                        toggleAll(
                                                            Boolean(checked),
                                                        )
                                                    }
                                                />
                                            </TableHead>
                                            <TableHead>开票内容</TableHead>
                                            <TableHead className="w-28 text-right">
                                                数量
                                            </TableHead>
                                            <TableHead className="w-32 text-right">
                                                含税单价（元）
                                            </TableHead>
                                            <TableHead className="w-32 text-right">
                                                行金额
                                            </TableHead>
                                        </TableRow>
                                    </TableHeader>
                                    <TableBody>
                                        {lines.map((line) => {
                                            const checked =
                                                line.selectable &&
                                                !deselectedIds.has(line.id)
                                            const key = toAutomationIdSegment(
                                                line.id,
                                            )
                                            return (
                                                <TableRow
                                                    key={line.id}
                                                    className={cn(
                                                        !line.selectable &&
                                                            "opacity-60",
                                                    )}
                                                >
                                                    <TableCell>
                                                        <Checkbox
                                                            id={`invoice-request-import-line-${key}`}
                                                            aria-label={`选择第 ${line.lineNo} 行 ${line.content || "明细"}`}
                                                            checked={checked}
                                                            disabled={
                                                                !line.selectable
                                                            }
                                                            onCheckedChange={(
                                                                next,
                                                            ) =>
                                                                toggleLine(
                                                                    line.id,
                                                                    Boolean(
                                                                        next,
                                                                    ),
                                                                )
                                                            }
                                                        />
                                                    </TableCell>
                                                    <TableCell className="min-w-48 break-words whitespace-normal">
                                                        <div className="flex flex-wrap items-center gap-1.5">
                                                            <span>
                                                                {line.content ||
                                                                    "—"}
                                                            </span>
                                                            {line.disabledReason ? (
                                                                <Badge variant="neutral">
                                                                    {
                                                                        line.disabledReason
                                                                    }
                                                                </Badge>
                                                            ) : null}
                                                        </div>
                                                    </TableCell>
                                                    <TableCell className="num text-right">
                                                        {line.quantity
                                                            ? `${formatFixedDisplay(line.quantity, { maxScale: INVOICE_QUANTITY_SCALE })}${line.unit ? ` ${line.unit}` : ""}`
                                                            : "—"}
                                                    </TableCell>
                                                    <TableCell className="num text-right">
                                                        {line.unitPrice
                                                            ? formatFixedDisplay(
                                                                  line.unitPrice,
                                                                  {
                                                                      maxScale:
                                                                          INVOICE_UNIT_PRICE_SCALE,
                                                                  },
                                                              )
                                                            : "—"}
                                                    </TableCell>
                                                    <TableCell className="text-right">
                                                        {line.lineAmount ? (
                                                            <MoneyValue
                                                                value={
                                                                    line.lineAmount
                                                                }
                                                                display="exact"
                                                            />
                                                        ) : (
                                                            <span className="text-muted-foreground">
                                                                —
                                                            </span>
                                                        )}
                                                    </TableCell>
                                                </TableRow>
                                            )
                                        })}
                                    </TableBody>
                                </Table>
                            </div>
                        </>
                    )}
                </DialogBody>

                <DialogFooter>
                    <Button
                        id="invoice-request-import-cancel"
                        type="button"
                        variant="outline"
                        onClick={() => onOpenChange(false)}
                    >
                        取消
                    </Button>
                    {hasMeaningfulExistingLines ? (
                        <>
                            <Button
                                id="invoice-request-import-append"
                                type="button"
                                variant="outline"
                                disabled={
                                    selectedLines.length === 0 ||
                                    remainingSlots <= 0
                                }
                                onClick={() => confirmImport("append")}
                            >
                                追加到当前明细
                            </Button>
                            <Button
                                id="invoice-request-import-replace"
                                type="button"
                                disabled={
                                    selectedLines.length === 0 ||
                                    replaceOverflow
                                }
                                onClick={() => confirmImport("replace")}
                            >
                                替换当前明细
                            </Button>
                        </>
                    ) : (
                        <Button
                            id="invoice-request-import-confirm"
                            type="button"
                            disabled={
                                selectedLines.length === 0 || replaceOverflow
                            }
                            onClick={() => confirmImport("replace")}
                        >
                            带入所选明细（{selectedLines.length} 行）
                        </Button>
                    )}
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
