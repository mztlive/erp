"use client"

import { RequiredMark } from "@/components/ui/required-mark"

import { useState } from "react"

import {
    MoneyValue,
    ValidationSummary,
    type ValidationIssue,
} from "@/components/business"
import { ListSearchField } from "@/components/business/list-search-field"
import {
    TableToolbar,
    TableToolbarScope,
} from "@/components/business/table-toolbar"
import { Button } from "@/components/ui/button"
import { NativeCheckbox } from "@/components/ui/checkbox"
import { MoneyInput } from "@/components/form/money-input"
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
} from "@/features/customer-receivables/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { allocationSourceKey, useAllocationSources } from "../hooks/queries"
import { compareAmounts } from "../lib/allocation-math"
import { ReceiptAllocationSource } from "./receipt-allocation-source"

/** 回款应收勾选与金额编辑共用一张表，搜索不移除已选分配。 */
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
}: {
    session: AllocationSessionView
    allocations: readonly AllocationDraftLine[]
    issues: readonly ValidationIssue[]
    disabled: boolean
    removalDisabled: boolean
    onAdd: (target: AllocationTarget) => void
    onRemove: (lineKey: string) => void
    onAmountChange: (lineKey: string, amount: string) => void
    onFill: (line: AllocationDraftLine) => void
}) {
    const [search, setSearch] = useState("")
    const keyword = search.trim().toLocaleLowerCase()
    const lines = new Map(allocations.map((line) => [line.targetId, line]))
    const { canRead, sources } = useAllocationSources(session.pool)
    const visibleTargets = session.pool.filter((target) => {
        const source = canRead
            ? sources.get(allocationSourceKey(target))?.data
            : undefined
        return [
            target.salesOrderNo,
            target.label,
            target.dueDate,
            target.customerName,
            source?.customerName,
            source?.projectName,
            source?.contractNo,
            source?.ownerName,
            ...(source?.products ?? []),
        ]
            .filter(Boolean)
            .join(" ")
            .toLocaleLowerCase()
            .includes(keyword)
    })

    return (
        <section
            id="customer-receivables-session-allocations"
            aria-labelledby="customer-receivables-session-allocations-title"
            className="min-w-0 space-y-4"
        >
            <h2
                id="customer-receivables-session-allocations-title"
                className="text-base font-semibold"
            >
                核销明细
            </h2>
            <TableToolbarScope>
                <div className="min-w-0 space-y-4">
                    <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
                        <div className="min-w-0 max-w-lg flex-1 basis-64">
                            <ListSearchField
                                id="customer-receivables-session-allocation-search"
                                value={search}
                                onChange={setSearch}
                                placeholder="搜索单号、客户、商品或销售"
                                aria-label="搜索可关联的销售单应收"
                            />
                        </div>
                        <TableToolbar className="py-0">
                            <div
                                className="flex flex-wrap gap-x-3 gap-y-1 text-muted-foreground"
                                role="status"
                            >
                                <span>共 {visibleTargets.length} 条</span>
                                <span>
                                    已选 {allocations.length} 笔
                                    {keyword ? "（含搜索结果外）" : ""}
                                </span>
                            </div>
                        </TableToolbar>
                    </div>
                    <Table data-density="comfortable" className="min-w-3xl">
                        <caption className="sr-only">
                            选择销售单应收并填写本次核销金额
                        </caption>
                        <TableHeader>
                            <TableRow>
                                <TableHead scope="col" className="w-12">
                                    <span className="sr-only">选择应收</span>
                                </TableHead>
                                <TableHead scope="col">
                                    销售单 / 应收项目
                                </TableHead>
                                <TableHead scope="col" data-align="end">
                                    当前可核销金额
                                </TableHead>
                                <TableHead scope="col">到期日</TableHead>
                                <TableHead scope="col" data-align="end">
                                    本次核销金额
                                    <RequiredMark />
                                </TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {visibleTargets.length === 0 ? (
                                <TableRow>
                                    <TableCell
                                        colSpan={5}
                                        className="text-center text-muted-foreground"
                                    >
                                        {session.pool.length === 0
                                            ? "当前结算主体没有可关联的应收项目。"
                                            : "没有符合搜索条件的应收；已选分配保持不变。"}
                                    </TableCell>
                                </TableRow>
                            ) : (
                                visibleTargets.map((target) => {
                                    const source = sources.get(
                                        allocationSourceKey(target),
                                    )
                                    const line = lines.get(target.targetId)
                                    const segment = toAutomationIdSegment(
                                        target.targetId,
                                    )
                                    const amountId = `customer-receivables-session-allocation-${segment}-amount`
                                    const rowIssues = line
                                        ? issues.filter(
                                              (issue) =>
                                                  issue.targetId === amountId,
                                          )
                                        : []
                                    const errorId = `${amountId}-error`
                                    return (
                                        <TableRow
                                            key={target.targetId}
                                            data-state={
                                                line ? "selected" : undefined
                                            }
                                        >
                                            <TableCell>
                                                <NativeCheckbox
                                                    className="rounded-sm border-input bg-surface-control"
                                                    id={`customer-receivables-session-allocation-${segment}-select`}
                                                    aria-label={`关联 ${target.label}`}
                                                    checked={Boolean(line)}
                                                    disabled={
                                                        disabled ||
                                                        (!line &&
                                                            compareAmounts(
                                                                target.openAmount,
                                                                "0",
                                                            ) <= 0) ||
                                                        (Boolean(line) &&
                                                            removalDisabled)
                                                    }
                                                    onCheckedChange={(
                                                        checked,
                                                    ) => {
                                                        if (checked)
                                                            onAdd(target)
                                                        else if (line)
                                                            onRemove(
                                                                line.lineKey,
                                                            )
                                                    }}
                                                />
                                            </TableCell>
                                            <TableCell>
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
                                                        source?.isError ?? false
                                                    }
                                                    onRetry={() =>
                                                        void source?.refetch()
                                                    }
                                                />
                                            </TableCell>
                                            <TableCell data-align="end">
                                                <MoneyValue
                                                    value={target.openAmount}
                                                />
                                                {compareAmounts(
                                                    target.openAmount,
                                                    "0",
                                                ) <= 0 ? (
                                                    <div className="text-xs text-muted-foreground">
                                                        暂无可核销余额
                                                    </div>
                                                ) : null}
                                            </TableCell>
                                            <TableCell className="num">
                                                {target.dueDate || "—"}
                                            </TableCell>
                                            <TableCell data-align="end">
                                                {line ? (
                                                    <div className="ml-auto min-w-36 max-w-52 whitespace-normal">
                                                        <div className="flex flex-wrap items-center justify-end gap-1">
                                                            <MoneyInput
                                                                id={amountId}
                                                                className="num text-right"
                                                                inputMode="decimal"
                                                                aria-label={`${target.label} 本次核销金额`}
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
                                                                value={
                                                                    line.amount
                                                                }
                                                                disabled={
                                                                    disabled
                                                                }
                                                                onChange={(
                                                                    event,
                                                                ) =>
                                                                    onAmountChange(
                                                                        line.lineKey,
                                                                        event
                                                                            .target
                                                                            .value,
                                                                    )
                                                                }
                                                            />
                                                            <Button
                                                                id={`customer-receivables-session-allocation-${segment}-fill`}
                                                                type="button"
                                                                size="xs"
                                                                variant="ghost"
                                                                disabled={
                                                                    disabled
                                                                }
                                                                onClick={() =>
                                                                    onFill(line)
                                                                }
                                                            >
                                                                填入剩余
                                                            </Button>
                                                        </div>
                                                        {rowIssues.length >
                                                        0 ? (
                                                            <p
                                                                id={errorId}
                                                                className="mt-1 text-xs text-destructive"
                                                            >
                                                                {rowIssues
                                                                    .map(
                                                                        (
                                                                            issue,
                                                                        ) =>
                                                                            issue.message,
                                                                    )
                                                                    .join("；")}
                                                            </p>
                                                        ) : null}
                                                    </div>
                                                ) : (
                                                    <span className="text-muted-foreground">
                                                        —
                                                    </span>
                                                )}
                                            </TableCell>
                                        </TableRow>
                                    )
                                })
                            )}
                        </TableBody>
                    </Table>
                </div>
            </TableToolbarScope>
            <p className="text-xs text-muted-foreground">
                当前可核销金额已扣除已核销、应收冲减及待审批回款占用；允许保留未核销回款余额。
            </p>
            <ValidationSummary
                id="customer-receivables-session-validation"
                issues={issues.filter((issue) => issue.id !== "need-alloc")}
                title="提交前请处理"
                onLocate={(issue) => {
                    const targetId = issue.targetId
                    if (!targetId) return
                    setSearch("")
                    requestAnimationFrame(() => {
                        const target = document.getElementById(targetId)
                        target?.scrollIntoView({ block: "center" })
                        target?.focus({ preventScroll: true })
                    })
                }}
            />
        </section>
    )
}
