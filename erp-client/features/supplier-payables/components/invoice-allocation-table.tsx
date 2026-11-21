"use client"

import { MoneyValue } from "@/components/business"
import { MoneyInput } from "@/components/form/money-input"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
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

/** 同供应商应付按行选择和录入金额；多单税额随对应行填写。 */
export function InvoiceAllocationTable({
    state,
}: {
    state: AllocationSessionState
}) {
    const {
        pool,
        selected,
        amounts,
        invoiceForm,
        policyBlocksAuto,
        isSubmitting,
        toggleItem,
        toggleSelectAll,
        setAmountFor,
    } = state
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
                        <p className="text-xs text-warning">
                            请按发票实际内容填写各笔分配税额，合计须与票面税额一致。
                        </p>
                    ) : null}
                </div>
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
            <div className="overflow-x-auto border-y border-border">
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
                        ) : (
                            pool.map((row) => {
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
                                        <TableCell className="max-w-64 whitespace-normal break-all num">
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
                                            <MoneyInput
                                                id={`supplier-payables-allocation-pool-row-${rowId}-amount`}
                                                aria-label={`${row.sourceDocumentNo} 本次核销金额`}
                                                className="ml-auto w-40 text-right num"
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
                                        </TableCell>
                                        {splitTax ? (
                                            <TableCell>
                                                <invoiceForm.AppField name="allocationTaxes">
                                                    {(field) => (
                                                        <MoneyInput
                                                            id={`supplier-invoice-tax-${rowId}`}
                                                            aria-label={`${row.sourceDocumentNo} 分配税额`}
                                                            className="ml-auto w-32 text-right num"
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
                                {splitTax ? <TableCell /> : null}
                            </TableRow>
                        </TableFooter>
                    ) : null}
                </Table>
            </div>
        </section>
    )
}
