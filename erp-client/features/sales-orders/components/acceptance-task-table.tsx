"use client"

import { Fragment } from "react"
import { ChevronLeftIcon, ChevronRightIcon } from "lucide-react"

import { OptionCombobox, ReasonTextareaField } from "@/components/business"
import { Button } from "@/components/ui/button"
import { NativeCheckbox } from "@/components/ui/checkbox"
import { Field, FieldLabel } from "@/components/ui/field"
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
import { toAutomationIdSegment } from "@/lib/automation-id"
import { displayName } from "@/lib/display-name"
import { compareDecimal } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import type { AcceptanceSelectionApi } from "../hooks/use-acceptance-selection"
import {
    batchDecisionsForFact,
    exceptionQuantityLabel,
    qtyWithUnit,
} from "../lib/acceptance-model"
import {
    BATCH_DECISION_LABEL,
    FULFILLMENT_TYPE_LABEL,
    type AcceptanceBatchDecision,
    type AcceptanceEligibleFact,
} from "../lib/acceptance-types"
import type { buildAcceptanceTaskTotals } from "./acceptance-task-totals"

export const ACCEPTANCE_REASON_MAX_BYTES = 512

const quantityClassName =
    "num min-w-0 border-0 bg-surface-sunken px-2 text-right shadow-none hover:bg-card focus-visible:bg-card"

export type AcceptanceTaskIssue = {
    target: string
    message: string
    factId?: string
}

export function acceptanceTaskRowId(idPrefix: string, factId: string) {
    return `${idPrefix}-batch-${toAutomationIdSegment(factId)}`
}

export function AcceptanceTaskTable({
    idPrefix,
    id,
    rows,
    totalCount,
    filteredCount,
    page,
    pageCount,
    onPageChange,
    totals,
    selection,
    checked,
    setChecked,
    issuesByFact,
    readOnly,
    pulse,
    onFill,
    onQuantityChange,
}: {
    idPrefix: string
    id: string
    rows: AcceptanceEligibleFact[]
    totalCount: number
    filteredCount: number
    page: number
    pageCount: number
    onPageChange: (nextPage: number) => void
    totals: ReturnType<typeof buildAcceptanceTaskTotals>
    selection: AcceptanceSelectionApi
    checked: ReadonlySet<string>
    setChecked: (
        rows: readonly AcceptanceEligibleFact[],
        value: boolean,
    ) => void
    issuesByFact: ReadonlyMap<string, AcceptanceTaskIssue[]>
    readOnly: boolean
    pulse: (id: string) => string | false
    onFill: (rows: readonly AcceptanceEligibleFact[]) => void
    onQuantityChange: (fact: AcceptanceEligibleFact, quantity: string) => void
}) {
    const visibleCheckedCount = rows.filter((fact) =>
        checked.has(fact.fulfillmentLineId),
    ).length
    const isFiltered = filteredCount < totalCount
    const showPagination = filteredCount > 20 && pageCount > 1

    return (
        <div
            id={id}
            tabIndex={-1}
            className={cn("min-w-0 rounded-md outline-none", pulse(id))}
        >
            <Table className="table-fixed" aria-label="待验交付批次">
                <colgroup>
                    <col className="w-9" />
                    <col />
                    <col className="w-[30%]" />
                    <col className="w-[26%]" />
                </colgroup>
                <TableHeader className="bg-transparent [&_tr]:border-0">
                    <TableRow className="border-0 hover:bg-transparent">
                        <TableHead className="rounded-l-md bg-surface-sunken px-2">
                            <NativeCheckbox
                                id={`${idPrefix}-check-visible`}
                                aria-label="勾选当前页批次"
                                checked={
                                    rows.length > 0 &&
                                    visibleCheckedCount === rows.length
                                }
                                indeterminate={
                                    visibleCheckedCount > 0 &&
                                    visibleCheckedCount < rows.length
                                }
                                disabled={readOnly || rows.length === 0}
                                onCheckedChange={(value) =>
                                    setChecked(rows, value)
                                }
                            />
                        </TableHead>
                        <TableHead className="bg-surface-sunken px-2">
                            商品 / 交付批次
                        </TableHead>
                        <TableHead className="bg-surface-sunken px-2">
                            本次数量
                        </TableHead>
                        <TableHead className="rounded-r-md bg-surface-sunken px-2">
                            验收结果
                        </TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {rows.length === 0 ? (
                        <TableRow>
                            <TableCell
                                colSpan={4}
                                className="py-4 text-center text-muted-foreground"
                            >
                                没有符合条件的交付批次
                            </TableCell>
                        </TableRow>
                    ) : null}
                    {rows.map((fact) => {
                        const factId = fact.fulfillmentLineId
                        const prefix = acceptanceTaskRowId(idPrefix, factId)
                        const draft = selection.selected.get(factId)
                        const result = draft?.result ?? "SKIP"
                        const exception =
                            draft && draft.result !== "PASS" ? draft : undefined
                        const issues = issuesByFact.get(factId)
                        const hasSubRow = Boolean(exception || issues?.length)
                        const isSevereException =
                            exception?.result === "REJECT" ||
                            exception?.result === "SERVICE_FAIL"
                        const issueId = issues?.length
                            ? `${prefix}-issues`
                            : undefined
                        const invalid = (field: string) =>
                            issues?.some(
                                (issue) =>
                                    issue.target === `${prefix}-${field}`,
                            ) || undefined
                        const label = `${fact.itemSnapshot} · 第 ${fact.lineNo} 行`
                        const documentNo = displayName(
                            fact.fulfillmentNo,
                            fact.fulfillmentLineId,
                            fact.deliveryId,
                        )

                        return (
                            <Fragment key={factId}>
                                <TableRow
                                    id={prefix}
                                    data-state={
                                        checked.has(factId)
                                            ? "selected"
                                            : undefined
                                    }
                                    className={cn(
                                        hasSubRow && "border-b-0",
                                        !draft && "opacity-60",
                                        pulse(prefix),
                                    )}
                                >
                                    <TableCell className="px-2">
                                        <NativeCheckbox
                                            id={`${prefix}-check`}
                                            aria-label={`勾选 ${label} 批量处理`}
                                            checked={checked.has(factId)}
                                            disabled={readOnly}
                                            onCheckedChange={(value) =>
                                                setChecked([fact], value)
                                            }
                                        />
                                    </TableCell>
                                    <TableCell
                                        className={cn(
                                            "px-2 whitespace-normal wrap-anywhere",
                                            !draft && "text-muted-foreground",
                                        )}
                                    >
                                        <div className="font-medium">
                                            {fact.itemSnapshot}
                                        </div>
                                        <div className="text-xs text-muted-foreground">
                                            {
                                                FULFILLMENT_TYPE_LABEL[
                                                    fact.fulfillmentFactType
                                                ]
                                            }
                                            {documentNo ? (
                                                <span className="num select-all">
                                                    {" "}
                                                    · {documentNo}
                                                </span>
                                            ) : null}
                                        </div>
                                    </TableCell>
                                    <TableCell className="px-2 whitespace-normal">
                                        <div className="flex min-w-0 flex-wrap items-center gap-1">
                                            <Input
                                                id={`${prefix}-qty`}
                                                size="sm"
                                                aria-label={`${label} 本次验收数量`}
                                                aria-invalid={invalid("qty")}
                                                aria-describedby={issueId}
                                                inputMode="decimal"
                                                className={cn(
                                                    quantityClassName,
                                                    "w-16 max-w-full",
                                                )}
                                                value={draft?.qty ?? ""}
                                                placeholder="0"
                                                disabled={readOnly}
                                                onChange={(event) =>
                                                    onQuantityChange(
                                                        fact,
                                                        event.target.value,
                                                    )
                                                }
                                                onBlur={(event) => {
                                                    const value =
                                                        event.target.value
                                                    if (!value.trim()) return
                                                    try {
                                                        if (
                                                            compareDecimal(
                                                                value,
                                                                "0",
                                                                6,
                                                            ) === 0
                                                        )
                                                            selection.skipFact(
                                                                factId,
                                                            )
                                                    } catch {
                                                        // 无效数量保留在输入框供修改。
                                                    }
                                                }}
                                            />
                                            <span className="num wrap-anywhere text-tiny text-muted-foreground">
                                                /{" "}
                                                {qtyWithUnit(
                                                    fact.eligibleQuantity,
                                                    fact.unitName,
                                                )}
                                            </span>
                                            <Button
                                                id={`${prefix}-fill`}
                                                type="button"
                                                variant="ghost"
                                                size="xs"
                                                disabled={readOnly}
                                                onClick={() => onFill([fact])}
                                            >
                                                填余量
                                            </Button>
                                        </div>
                                    </TableCell>
                                    <TableCell className="px-2">
                                        <OptionCombobox
                                            id={`${prefix}-result`}
                                            size="sm"
                                            allowClear={false}
                                            disabled={readOnly}
                                            aria-label={`${label} 验收结果`}
                                            value={result}
                                            options={batchDecisionsForFact(
                                                fact,
                                            ).map((decision) => ({
                                                value: decision,
                                                label: BATCH_DECISION_LABEL[
                                                    decision
                                                ],
                                            }))}
                                            inputClassName="w-full min-w-0 border-0 bg-surface-sunken shadow-none hover:bg-card"
                                            onValueChange={(value) => {
                                                const decision =
                                                    value as AcceptanceBatchDecision | null
                                                if (decision === "SKIP")
                                                    selection.skipFact(factId)
                                                else if (decision)
                                                    selection.selectResult(
                                                        fact,
                                                        decision,
                                                    )
                                            }}
                                        />
                                    </TableCell>
                                </TableRow>
                                {hasSubRow ? (
                                    <TableRow
                                        className={cn(
                                            isSevereException
                                                ? "bg-destructive-soft/40"
                                                : "bg-warning-soft/40",
                                            pulse(prefix),
                                        )}
                                    >
                                        <TableCell
                                            colSpan={4}
                                            className={cn(
                                                "border-l-2 px-3 py-2 whitespace-normal",
                                                isSevereException
                                                    ? "border-l-destructive"
                                                    : "border-l-warning",
                                            )}
                                        >
                                            {exception ? (
                                                <div className="grid min-w-0 gap-2 @[480px]/acceptance-task:grid-cols-[7rem_minmax(0,1fr)]">
                                                    <Field className="min-w-0 gap-1">
                                                        <FieldLabel
                                                            htmlFor={`${prefix}-exc`}
                                                        >
                                                            {exceptionQuantityLabel(
                                                                exception.result,
                                                            )}
                                                        </FieldLabel>
                                                        <Input
                                                            id={`${prefix}-exc`}
                                                            size="sm"
                                                            inputMode="decimal"
                                                            className={
                                                                quantityClassName
                                                            }
                                                            disabled={readOnly}
                                                            value={
                                                                exception.exceptionQty
                                                            }
                                                            aria-invalid={invalid(
                                                                "exc",
                                                            )}
                                                            aria-describedby={
                                                                issueId
                                                            }
                                                            onChange={(event) =>
                                                                selection.updateDraft(
                                                                    factId,
                                                                    {
                                                                        exceptionQty:
                                                                            event
                                                                                .target
                                                                                .value,
                                                                    },
                                                                )
                                                            }
                                                        />
                                                    </Field>
                                                    <ReasonTextareaField
                                                        id={`${prefix}-reason`}
                                                        mode="utf8-bytes"
                                                        max={
                                                            ACCEPTANCE_REASON_MAX_BYTES
                                                        }
                                                        label="异常原因"
                                                        rows={1}
                                                        required
                                                        invalid={invalid(
                                                            "reason",
                                                        )}
                                                        aria-describedby={
                                                            issueId
                                                        }
                                                        disabled={readOnly}
                                                        value={exception.reason}
                                                        onChange={(reason) =>
                                                            selection.updateDraft(
                                                                factId,
                                                                { reason },
                                                            )
                                                        }
                                                    />
                                                </div>
                                            ) : null}
                                            {issues?.length ? (
                                                <ul
                                                    id={issueId}
                                                    className={cn(
                                                        "mt-1 wrap-anywhere text-xs",
                                                        isSevereException
                                                            ? "text-destructive-soft-foreground"
                                                            : "text-warning-soft-foreground",
                                                    )}
                                                >
                                                    {issues.map((issue) => (
                                                        <li key={issue.message}>
                                                            {issue.message}
                                                        </li>
                                                    ))}
                                                </ul>
                                            ) : null}
                                        </TableCell>
                                    </TableRow>
                                ) : null}
                            </Fragment>
                        )
                    })}
                </TableBody>
                <TableFooter className="border-0 bg-transparent">
                    <TableRow className="border-0 hover:bg-transparent">
                        <TableCell
                            colSpan={2}
                            className="rounded-l-md bg-surface-sunken px-2 whitespace-normal"
                        >
                            <span>
                                整单合计 · 本次{" "}
                                <span className="num">{totals.rowCount}</span>{" "}
                                行
                            </span>
                            {isFiltered ? (
                                <span className="ml-1.5 text-tiny text-muted-foreground">
                                    （当前显示{" "}
                                    <span className="num">
                                        {filteredCount} / {totalCount}
                                    </span>{" "}
                                    项，合计按整单）
                                </span>
                            ) : null}
                        </TableCell>
                        <TableCell className="num bg-surface-sunken px-2 whitespace-normal wrap-anywhere">
                            {totals.registered}
                        </TableCell>
                        <TableCell className="rounded-r-md bg-surface-sunken px-2 whitespace-normal text-muted-foreground">
                            <div className="flex flex-wrap items-center justify-between gap-2">
                                <span>
                                    可验{" "}
                                    <span className="num wrap-anywhere">
                                        {totals.pending}
                                    </span>
                                </span>
                                {showPagination ? (
                                    <div className="flex items-center gap-1">
                                        <Button
                                            id={`${idPrefix}-page-prev`}
                                            type="button"
                                            size="icon-xs"
                                            variant="ghost"
                                            aria-label="上一页"
                                            disabled={page === 0}
                                            onClick={() =>
                                                onPageChange(page - 1)
                                            }
                                        >
                                            <ChevronLeftIcon aria-hidden="true" />
                                        </Button>
                                        <span className="num">
                                            {page + 1} / {pageCount}
                                        </span>
                                        <Button
                                            id={`${idPrefix}-page-next`}
                                            type="button"
                                            size="icon-xs"
                                            variant="ghost"
                                            aria-label="下一页"
                                            disabled={page + 1 >= pageCount}
                                            onClick={() =>
                                                onPageChange(page + 1)
                                            }
                                        >
                                            <ChevronRightIcon aria-hidden="true" />
                                        </Button>
                                    </div>
                                ) : null}
                            </div>
                        </TableCell>
                    </TableRow>
                </TableFooter>
            </Table>
        </div>
    )
}
