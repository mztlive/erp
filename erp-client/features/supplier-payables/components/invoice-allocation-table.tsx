"use client"

import * as React from "react"
import { useStore } from "@tanstack/react-form"
import { SearchIcon } from "lucide-react"

import { MoneyValue } from "@/components/business"
import { MoneyInput } from "@/components/form/money-input"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import {
    Table,
    TableBody,
    TableCell,
    TableFooter,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import type { AllocationSessionState } from "@/features/supplier-payables/hooks/use-allocation-session"
import {
    cents,
    fromCents,
} from "@/features/supplier-payables/lib/allocation-model"
import { SOURCE_TYPE_LABEL } from "@/features/supplier-payables/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"

/** 同供应商应付按行选择和录入金额；支持快速检索、单行填满与多单分配税额实时合计。 */
export function InvoiceAllocationTable({
    state,
}: {
    state: AllocationSessionState
}) {
    const [search, setSearch] = React.useState("")
    const {
        pool,
        selected,
        amounts,
        factAmount,
        invoiceForm,
        policyBlocksAuto,
        isSubmitting,
        toggleItem,
        toggleSelectAll,
        setAmountFor,
    } = state
    const taxValues = useStore(invoiceForm.store, (s) => ({
        faceTax: s.values.taxAmount,
        allocationTaxes: s.values.allocationTaxes,
    }))
    const trimmedSearch = search.trim().toLowerCase()
    const visiblePool = React.useMemo(() => {
        if (!trimmedSearch) return pool
        return pool.filter((row) => {
            const docNo = row.sourceDocumentNo.toLowerCase()
            const typeLabel = SOURCE_TYPE_LABEL[row.sourceType].toLowerCase()
            return (
                docNo.includes(trimmedSearch) ||
                typeLabel.includes(trimmedSearch)
            )
        })
    }, [pool, trimmedSearch])

    const allSelected =
        pool.length > 0 &&
        pool.every((row) => selected.has(row.payableAccountId))
    const splitTax = selected.size > 1
    const available = fromCents(
        pool.reduce(
            (total, row) => total + cents(row.openInvoiceableTotal),
            BigInt(0),
        ),
    )

    let allocatedTaxCents = BigInt(0)
    if (splitTax) {
        for (const id of selected) {
            allocatedTaxCents += cents(taxValues.allocationTaxes[id] || "0")
        }
    }
    const faceTaxCents = cents(taxValues.faceTax || "0")
    const taxMismatch =
        splitTax &&
        faceTaxCents > BigInt(0) &&
        allocatedTaxCents !== faceTaxCents

    function fillRowAmount(payableAccountId: string, openInvoiceable: string) {
        const openCents = cents(openInvoiceable)
        const factCents = cents(factAmount || "0")
        if (factCents <= BigInt(0)) {
            setAmountFor(payableAccountId, fromCents(openCents))
            return
        }
        let othersAllocated = BigInt(0)
        for (const id of selected) {
            if (id !== payableAccountId) {
                othersAllocated += cents(amounts[id] || "0")
            }
        }
        const remaining = factCents - othersAllocated
        const nextCents =
            remaining > BigInt(0) && remaining < openCents
                ? remaining
                : openCents
        setAmountFor(payableAccountId, fromCents(nextCents))
    }

    return (
        <section
            id="alloc-pool"
            className="min-w-0 space-y-4"
            aria-labelledby="invoice-allocation-title"
        >
            <div className="flex flex-wrap items-center justify-between gap-3">
                <div className="space-y-1">
                    <div className="flex items-center gap-2">
                        <h2
                            id="invoice-allocation-title"
                            tabIndex={-1}
                            className="text-section-title font-semibold text-foreground tracking-tight"
                        >
                            核销明细
                        </h2>
                        <span className="num rounded-md bg-muted px-2 py-0.5 text-xs text-muted-foreground">
                            共 {pool.length} 笔 · 已选 {selected.size} 笔
                        </span>
                    </div>
                    <p className="text-xs text-muted-foreground">
                        选择当前供应商对应的采购单或结算单
                        {policyBlocksAuto
                            ? " · 当前需手动选择单据并填写核销金额"
                            : null}
                    </p>
                    {splitTax ? (
                        <p
                            className={cn(
                                "text-xs",
                                taxMismatch
                                    ? "text-destructive"
                                    : "text-warning",
                            )}
                        >
                            请按发票实际内容填写各笔分配税额，合计须与票面税额一致。
                        </p>
                    ) : null}
                </div>
                <div className="flex flex-wrap items-center gap-2">
                    {pool.length > 0 ? (
                        <div className="relative w-52">
                            <SearchIcon
                                className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
                                aria-hidden="true"
                            />
                            <Input
                                id="supplier-payables-allocation-pool-search"
                                value={search}
                                onChange={(e) => setSearch(e.target.value)}
                                placeholder="筛选单据编号"
                                aria-label="筛选可核销单据编号"
                                className="pl-8"
                            />
                        </div>
                    ) : null}
                    {selected.size > 0 ? (
                        <Button
                            id="invoice-allocation-clear-selection"
                            type="button"
                            variant="outline"
                            size="sm"
                            disabled={isSubmitting}
                            onClick={state.clearSelection}
                        >
                            清空选择
                        </Button>
                    ) : null}
                </div>
            </div>
            <div className="overflow-x-auto border-y border-border/70">
                <Table>
                    <TableHeader>
                        <TableRow>
                            <TableHead className="w-12">
                                <Checkbox
                                    id="supplier-payables-allocation-pool-select-all"
                                    checked={allSelected}
                                    indeterminate={
                                        !allSelected && selected.size > 0
                                    }
                                    disabled={!pool.length || isSubmitting}
                                    onCheckedChange={toggleSelectAll}
                                    aria-label="选择全部核销单据"
                                />
                            </TableHead>
                            <TableHead>单据类型</TableHead>
                            <TableHead>单据编号</TableHead>
                            <TableHead>到期日期</TableHead>
                            <TableHead className="text-right">
                                可收票金额（元）
                            </TableHead>
                            <TableHead className="text-right">
                                本次核销金额（元）
                            </TableHead>
                            {splitTax ? (
                                <TableHead className="text-right">
                                    分配税额（元）
                                </TableHead>
                            ) : null}
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {!pool.length ? (
                            <TableRow>
                                <TableCell
                                    colSpan={splitTax ? 7 : 6}
                                    className="h-24 text-center text-muted-foreground"
                                >
                                    当前供应商没有可核销的应付单据
                                </TableCell>
                            </TableRow>
                        ) : !visiblePool.length ? (
                            <TableRow>
                                <TableCell
                                    colSpan={splitTax ? 7 : 6}
                                    className="h-20 text-center text-muted-foreground"
                                >
                                    未找到匹配「{search.trim()}」的单据
                                </TableCell>
                            </TableRow>
                        ) : (
                            visiblePool.map((row) => {
                                const checked = selected.has(
                                    row.payableAccountId,
                                )
                                const rowId = toAutomationIdSegment(
                                    row.payableAccountId,
                                )
                                return (
                                    <TableRow
                                        key={row.payableAccountId}
                                        data-state={
                                            checked ? "selected" : undefined
                                        }
                                    >
                                        <TableCell>
                                            <Checkbox
                                                id={`supplier-payables-allocation-pool-row-${rowId}-select`}
                                                checked={checked}
                                                disabled={isSubmitting}
                                                onCheckedChange={(value) =>
                                                    toggleItem(
                                                        row.payableAccountId,
                                                        value,
                                                        row.openInvoiceableTotal,
                                                    )
                                                }
                                                aria-label={`选择 ${row.sourceDocumentNo}`}
                                            />
                                        </TableCell>
                                        <TableCell>
                                            {SOURCE_TYPE_LABEL[row.sourceType]}
                                        </TableCell>
                                        <TableCell className="num max-w-64 whitespace-normal break-all">
                                            {row.sourceDocumentNo}
                                        </TableCell>
                                        <TableCell className="num text-muted-foreground">
                                            {row.dueDate || "—"}
                                        </TableCell>
                                        <TableCell className="text-right">
                                            <MoneyValue
                                                value={row.openInvoiceableTotal}
                                            />
                                        </TableCell>
                                        <TableCell>
                                            <div className="flex items-center justify-end gap-1">
                                                <MoneyInput
                                                    id={`supplier-payables-allocation-pool-row-${rowId}-amount`}
                                                    aria-label={`${row.sourceDocumentNo} 本次核销金额`}
                                                    className="num w-36 text-right"
                                                    placeholder="0.00"
                                                    value={
                                                        checked
                                                            ? (amounts[
                                                                  row
                                                                      .payableAccountId
                                                              ] ?? "")
                                                            : ""
                                                    }
                                                    disabled={
                                                        !checked || isSubmitting
                                                    }
                                                    onChange={(event) =>
                                                        setAmountFor(
                                                            row.payableAccountId,
                                                            event.target.value,
                                                        )
                                                    }
                                                />
                                                {checked ? (
                                                    <Button
                                                        id={`supplier-payables-allocation-pool-row-${rowId}-fill`}
                                                        type="button"
                                                        size="xs"
                                                        variant="ghost"
                                                        disabled={isSubmitting}
                                                        title="按本单可收票金额与发票剩余可核销金额中的较小值填入"
                                                        onClick={() =>
                                                            fillRowAmount(
                                                                row.payableAccountId,
                                                                row.openInvoiceableTotal,
                                                            )
                                                        }
                                                    >
                                                        填满
                                                    </Button>
                                                ) : null}
                                            </div>
                                        </TableCell>
                                        {splitTax ? (
                                            <TableCell>
                                                <invoiceForm.AppField name="allocationTaxes">
                                                    {(field) => (
                                                        <MoneyInput
                                                            id={`supplier-invoice-tax-${rowId}`}
                                                            aria-label={`${row.sourceDocumentNo} 分配税额`}
                                                            className="num ml-auto w-32 text-right"
                                                            placeholder="0.00"
                                                            value={
                                                                checked
                                                                    ? (field
                                                                          .state
                                                                          .value[
                                                                          row
                                                                              .payableAccountId
                                                                      ] ?? "")
                                                                    : ""
                                                            }
                                                            disabled={
                                                                !checked ||
                                                                isSubmitting
                                                            }
                                                            onBlur={
                                                                field.handleBlur
                                                            }
                                                            onChange={(event) =>
                                                                field.handleChange(
                                                                    {
                                                                        ...field
                                                                            .state
                                                                            .value,
                                                                        [row.payableAccountId]:
                                                                            event
                                                                                .target
                                                                                .value,
                                                                    },
                                                                )
                                                            }
                                                        />
                                                    )}
                                                </invoiceForm.AppField>
                                            </TableCell>
                                        ) : null}
                                    </TableRow>
                                )
                            })
                        )}
                    </TableBody>
                    {pool.length ? (
                        <TableFooter>
                            <TableRow>
                                <TableCell colSpan={4}>合计</TableCell>
                                <TableCell className="text-right">
                                    <MoneyValue value={available} />
                                </TableCell>
                                <TableCell className="text-right">
                                    <MoneyValue value={state.allocatedHint} />
                                </TableCell>
                                {splitTax ? (
                                    <TableCell className="text-right">
                                        <div className="flex flex-col items-end">
                                            <span
                                                className={
                                                    taxMismatch
                                                        ? "text-destructive"
                                                        : undefined
                                                }
                                            >
                                                <MoneyValue
                                                    value={fromCents(
                                                        allocatedTaxCents,
                                                    )}
                                                    inheritColor={taxMismatch}
                                                />
                                            </span>
                                            {faceTaxCents > BigInt(0) ? (
                                                <span
                                                    className={cn(
                                                        "num text-xs",
                                                        taxMismatch
                                                            ? "text-destructive"
                                                            : "text-muted-foreground",
                                                    )}
                                                >
                                                    票面{" "}
                                                    <MoneyValue
                                                        value={fromCents(
                                                            faceTaxCents,
                                                        )}
                                                        inheritColor
                                                    />
                                                </span>
                                            ) : null}
                                        </div>
                                    </TableCell>
                                ) : null}
                            </TableRow>
                        </TableFooter>
                    ) : null}
                </Table>
            </div>
        </section>
    )
}
