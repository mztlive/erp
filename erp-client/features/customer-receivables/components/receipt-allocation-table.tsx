"use client"

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
import { Checkbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
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
    const visibleTargets = session.pool.filter((target) =>
        `${target.salesOrderNo} ${target.label} ${target.dueDate ?? ""}`
            .toLocaleLowerCase()
            .includes(keyword),
    )

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
                关联销售单应收
            </h2>
            <div className="max-w-lg">
                <ListSearchField
                    id="customer-receivables-session-allocation-search"
                    value={search}
                    onChange={setSearch}
                    placeholder="搜索销售单号或应收项目"
                    aria-label="搜索可关联的销售单应收"
                />
            </div>
            <TableToolbarScope>
                <div className="min-w-0">
                    <TableToolbar>
                        <div
                            className="flex flex-wrap items-center gap-x-4 gap-y-1 text-muted-foreground"
                            role="status"
                        >
                            <span>显示 {visibleTargets.length} 条应收</span>
                            <span>
                                已选 {allocations.length} 笔
                                {keyword ? "（含搜索结果外）" : ""}
                            </span>
                        </div>
                    </TableToolbar>
                    <Table data-density="comfortable">
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
                                    应收项目金额
                                </TableHead>
                                <TableHead scope="col">到期日</TableHead>
                                <TableHead scope="col" data-align="end">
                                    本次核销金额
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
                                                <Checkbox
                                                    id={`customer-receivables-session-allocation-${segment}-select`}
                                                    aria-label={`关联 ${target.label}`}
                                                    checked={Boolean(line)}
                                                    disabled={
                                                        disabled ||
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
                                                <div className="min-w-48 whitespace-normal">
                                                    <div className="num font-medium">
                                                        {target.salesOrderNo}
                                                    </div>
                                                    <div className="mt-1 text-xs text-muted-foreground">
                                                        {target.label}
                                                    </div>
                                                </div>
                                            </TableCell>
                                            <TableCell data-align="end">
                                                <MoneyValue
                                                    value={target.openAmount}
                                                />
                                            </TableCell>
                                            <TableCell className="num">
                                                {target.dueDate || "—"}
                                            </TableCell>
                                            <TableCell data-align="end">
                                                {line ? (
                                                    <div className="ml-auto min-w-44 max-w-60 whitespace-normal">
                                                        <div className="flex items-center justify-end gap-1">
                                                            <Input
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
                应收项目金额用于核对来源；实际可核销余额由系统在提交时校验。允许保留未核销回款余额。
            </p>
            <ValidationSummary
                id="customer-receivables-session-validation"
                issues={issues}
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
