"use client"

import { useState } from "react"
import { PlusIcon } from "lucide-react"
import {
    MoneyValue,
    ValidationSummary,
    type ValidationIssue,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { MoneyInput } from "@/components/form/money-input"
import { RequiredMark } from "@/components/ui/required-mark"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import type {
    AllocationDraftLine,
    AllocationSessionView,
    AllocationTarget,
} from "../types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { allocationSourceKey, useAllocationSources } from "../hooks/queries"
import { ReceiptAllocationSource } from "./receipt-allocation-source"
import { ReceiptAllocationPicker } from "./receipt-allocation-picker"

/** 正文只编辑已选应收；候选搜索与批量选择放在添加弹窗中。 */
export function ReceiptAllocationTable({
    session,
    allocations,
    issues,
    disabled,
    removalDisabled,
    onAdd,
    onRemove,
    onAmountChange,
    onFill,
    onLocateIssue,
}: {
    session: AllocationSessionView
    allocations: readonly AllocationDraftLine[]
    issues: readonly ValidationIssue[]
    disabled: boolean
    removalDisabled: boolean
    onAdd: (targets: readonly AllocationTarget[]) => void
    onRemove: (lineKey: string) => void
    onAmountChange: (lineKey: string, amount: string) => void
    onFill: (line: AllocationDraftLine) => void
    onLocateIssue: (issue: ValidationIssue) => void
}) {
    const [pickerOpen, setPickerOpen] = useState(false)
    const targets = new Map(
        session.pool.map((target) => [target.targetId, target]),
    )
    const selectedTargets = session.pool.filter((target) =>
        allocations.some((line) => line.targetId === target.targetId),
    )
    const { canRead, sources } = useAllocationSources(selectedTargets)
    return (
        <section
            id="customer-receivables-session-allocations"
            aria-labelledby="customer-receivables-session-allocations-title"
            className="min-w-0 space-y-4"
        >
            <div className="flex flex-wrap items-center justify-between gap-3">
                <div className="flex items-center gap-2">
                    <h2
                        id="customer-receivables-session-allocations-title"
                        tabIndex={-1}
                        className="text-section-title font-semibold text-foreground tracking-tight"
                    >
                        核销应收
                    </h2>
                    <span className="num rounded-md bg-muted px-2 py-0.5 text-xs text-muted-foreground">
                        {allocations.length} 笔应收
                    </span>
                </div>
                <Button
                    id="customer-receivables-session-add-receivable"
                    type="button"
                    size="sm"
                    variant={allocations.length ? "outline" : "default"}
                    disabled={disabled || removalDisabled}
                    onClick={() => setPickerOpen(true)}
                >
                    <PlusIcon data-icon="inline-start" aria-hidden="true" />
                    添加应收
                </Button>
            </div>
            {allocations.length === 0 ? (
                <div className="border-y border-border py-14 text-center">
                    <p className="font-medium text-foreground">
                        尚未添加待核销应收
                    </p>
                    <p className="mt-1 text-xs text-muted-foreground">
                        点击右上角「添加应收」选择销售单；支持一次批量添加多笔。
                    </p>
                </div>
            ) : (
                <div>
                    <div className="overflow-x-auto border-y border-border">
                        <Table
                            className="min-w-[680px] table-fixed"
                            data-density="comfortable"
                        >
                            <caption className="sr-only">
                                本次已选应收与核销金额
                            </caption>
                            <TableHeader>
                                <TableRow>
                                    <TableHead>销售单 / 应收项目</TableHead>
                                    <TableHead
                                        className="w-36"
                                        data-align="end"
                                    >
                                        可核销金额
                                    </TableHead>
                                    <TableHead
                                        className="w-72"
                                        data-align="end"
                                    >
                                        本次核销金额
                                        <RequiredMark />
                                    </TableHead>
                                    <TableHead className="w-16">
                                        <span className="sr-only">操作</span>
                                    </TableHead>
                                </TableRow>
                            </TableHeader>
                            <TableBody>
                                {allocations.map((line) => {
                                    const target = targets.get(line.targetId)
                                    const source = target
                                        ? sources.get(
                                              allocationSourceKey(target),
                                          )
                                        : undefined
                                    const segment = toAutomationIdSegment(
                                        line.targetId,
                                    )
                                    const amountId = `customer-receivables-session-allocation-${segment}-amount`
                                    const rowIssues = issues.filter(
                                        (issue) => issue.targetId === amountId,
                                    )
                                    const errorId = `${amountId}-error`
                                    return (
                                        <TableRow key={line.lineKey}>
                                            <TableCell>
                                                {target ? (
                                                    <ReceiptAllocationSource
                                                        target={target}
                                                        source={
                                                            canRead
                                                                ? source?.data
                                                                : undefined
                                                        }
                                                        canRead={canRead}
                                                        pending={
                                                            source?.isPending ??
                                                            false
                                                        }
                                                        failed={
                                                            source?.isError ??
                                                            false
                                                        }
                                                        onRetry={() =>
                                                            void source?.refetch()
                                                        }
                                                    />
                                                ) : (
                                                    <div className="whitespace-normal">
                                                        <p className="font-medium">
                                                            {line.salesOrderNo}
                                                        </p>
                                                        <p className="text-xs text-muted-foreground">
                                                            {line.label}
                                                        </p>
                                                    </div>
                                                )}
                                                {target?.dueDate ? (
                                                    <p className="num mt-1 text-xs text-muted-foreground">
                                                        到期 {target.dueDate}
                                                    </p>
                                                ) : null}
                                            </TableCell>
                                            <TableCell
                                                data-align="end"
                                                className="text-muted-foreground"
                                            >
                                                <MoneyValue
                                                    value={line.openAmount}
                                                />
                                            </TableCell>
                                            <TableCell data-align="end">
                                                <div className="ml-auto w-60 whitespace-normal">
                                                    <div className="flex items-center justify-end gap-2">
                                                        <MoneyInput
                                                            id={amountId}
                                                            className="num h-control min-w-0 bg-background text-right font-semibold"
                                                            placeholder="填写金额"
                                                            inputMode="decimal"
                                                            aria-label={`${line.label} 本次核销金额`}
                                                            aria-invalid={
                                                                rowIssues.length >
                                                                    0 ||
                                                                undefined
                                                            }
                                                            aria-describedby={
                                                                rowIssues.length
                                                                    ? errorId
                                                                    : undefined
                                                            }
                                                            value={line.amount}
                                                            disabled={disabled}
                                                            onChange={(event) =>
                                                                onAmountChange(
                                                                    line.lineKey,
                                                                    event.target
                                                                        .value,
                                                                )
                                                            }
                                                        />
                                                        <Button
                                                            id={`customer-receivables-session-allocation-${segment}-fill`}
                                                            type="button"
                                                            size="xs"
                                                            variant="ghost"
                                                            disabled={disabled}
                                                            title="按本单可核销金额与回款剩余可分配金额中的较小值填入"
                                                            onClick={() =>
                                                                onFill(line)
                                                            }
                                                        >
                                                            自动填入
                                                        </Button>
                                                    </div>
                                                    {rowIssues.length > 0 ? (
                                                        <p
                                                            id={errorId}
                                                            className="mt-1 text-xs text-destructive"
                                                        >
                                                            {rowIssues
                                                                .map(
                                                                    (issue) =>
                                                                        issue.message,
                                                                )
                                                                .join("；")}
                                                        </p>
                                                    ) : null}
                                                </div>
                                            </TableCell>
                                            <TableCell>
                                                <Button
                                                    id={`customer-receivables-session-allocation-${segment}-remove`}
                                                    type="button"
                                                    size="xs"
                                                    variant="ghost"
                                                    disabled={
                                                        disabled ||
                                                        removalDisabled
                                                    }
                                                    aria-label={`移除 ${line.label}`}
                                                    onClick={() =>
                                                        onRemove(line.lineKey)
                                                    }
                                                >
                                                    移除
                                                </Button>
                                            </TableCell>
                                        </TableRow>
                                    )
                                })}
                            </TableBody>
                        </Table>
                    </div>
                    <p className="pt-2 text-xs text-muted-foreground">
                        可核销余额已扣除待审批占用；剩余回款可后续核销。
                    </p>
                </div>
            )}
            <ValidationSummary
                id="customer-receivables-session-validation"
                issues={issues}
                title="提交前请处理"
                onLocate={onLocateIssue}
            />
            {pickerOpen ? (
                <ReceiptAllocationPicker
                    session={session}
                    allocations={allocations}
                    disabled={disabled || removalDisabled}
                    onAdd={onAdd}
                    onClose={() => setPickerOpen(false)}
                />
            ) : null}
        </section>
    )
}
