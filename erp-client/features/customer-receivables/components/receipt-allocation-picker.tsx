"use client"

import { useState } from "react"
import { useStore } from "@tanstack/react-form"
import { useAppForm } from "@/components/form"
import { MoneyValue } from "@/components/business"
import { ListSearchField } from "@/components/business/list-search-field"
import {
    TableToolbar,
    TableToolbarScope,
} from "@/components/business/table-toolbar"
import { Button } from "@/components/ui/button"
import { NativeCheckbox } from "@/components/ui/checkbox"
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
import { toAutomationIdSegment } from "@/lib/automation-id"
import type {
    AllocationDraftLine,
    AllocationSessionView,
    AllocationTarget,
} from "../types"
import { allocationSourceKey, useAllocationSources } from "../hooks/queries"
import { compareAmounts } from "../lib/allocation-math"
import { ReceiptAllocationSource } from "./receipt-allocation-source"

/** 弹窗只选择新增应收，确认前不修改核销明细；搜索保留已勾选项。 */
export function ReceiptAllocationPicker({
    session,
    allocations,
    disabled,
    onAdd,
    onClose,
}: {
    session: AllocationSessionView
    allocations: readonly AllocationDraftLine[]
    disabled: boolean
    onAdd: (targets: readonly AllocationTarget[]) => void
    onClose: () => void
}) {
    const [search, setSearch] = useState("")
    const form = useAppForm({
        defaultValues: { targetIds: [] as string[] },
        onSubmit: ({ value }) => {
            if (disabled) return
            const selected = candidates.filter((target) =>
                value.targetIds.includes(target.targetId),
            )
            if (selected.length === 0) return
            onAdd(selected)
            onClose()
        },
    })
    const targetIds = useStore(form.store, (state) => state.values.targetIds)
    const existingIds = new Set(allocations.map((line) => line.targetId))
    const candidates = session.pool.filter(
        (target) =>
            target.counterpartyPartyId === session.counterpartyPartyId &&
            !existingIds.has(target.targetId) &&
            compareAmounts(target.openAmount, "0") > 0,
    )
    const { canRead, sources } = useAllocationSources(candidates)
    const keyword = search.trim().toLocaleLowerCase()
    const visible = candidates.filter((target) => {
        const source = canRead
            ? sources.get(allocationSourceKey(target))?.data
            : undefined
        return [
            target.salesOrderNo,
            target.label,
            target.customerName,
            target.dueDate,
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
        <Dialog
            open
            onOpenChange={(open) => {
                if (!open) onClose()
            }}
        >
            <DialogContent
                id="customer-receivables-picker-dialog"
                size="xl"
                closeButtonId="customer-receivables-picker-close"
                initialFocus={() =>
                    document.getElementById(
                        "customer-receivables-session-allocation-search",
                    )
                }
                finalFocus={() =>
                    document.getElementById(
                        "customer-receivables-session-add-receivable",
                    )
                }
            >
                <DialogHeader>
                    <DialogTitle>添加应收</DialogTitle>
                    <DialogDescription>
                        选择 {session.counterpartyPartyName}{" "}
                        的应收，添加后核对本次核销金额。
                    </DialogDescription>
                </DialogHeader>
                <form
                    id="customer-receivables-picker-form"
                    className="min-w-0 space-y-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        event.stopPropagation()
                        void form.handleSubmit()
                    }}
                >
                    <ListSearchField
                        id="customer-receivables-session-allocation-search"
                        value={search}
                        onChange={setSearch}
                        placeholder="搜索单号、客户、商品或销售"
                        aria-label="搜索可添加的应收"
                    />
                    <TableToolbarScope>
                        <TableToolbar>
                            <div className="flex flex-wrap items-center gap-3 text-muted-foreground">
                                <span>共 {visible.length} 条</span>
                                <span role="status">
                                    已选 {targetIds.length} 笔
                                    {keyword && targetIds.length > 0
                                        ? "（含搜索结果外）"
                                        : ""}
                                </span>
                            </div>
                        </TableToolbar>
                        <div className="max-h-96 overflow-auto">
                            <form.Field name="targetIds">
                                {(field) => (
                                    <Table className="min-w-xl">
                                        <caption className="sr-only">
                                            选择要加入本次核销的应收
                                        </caption>
                                        <TableHeader>
                                            <TableRow>
                                                <TableHead className="w-12">
                                                    <span className="sr-only">
                                                        选择
                                                    </span>
                                                </TableHead>
                                                <TableHead>
                                                    销售单 / 应收项目
                                                </TableHead>
                                                <TableHead>到期日</TableHead>
                                                <TableHead data-align="end">
                                                    可核销金额
                                                </TableHead>
                                            </TableRow>
                                        </TableHeader>
                                        <TableBody>
                                            {visible.length === 0 ? (
                                                <TableRow>
                                                    <TableCell
                                                        colSpan={4}
                                                        className="py-10 text-center text-muted-foreground"
                                                    >
                                                        {candidates.length === 0
                                                            ? "没有可添加的应收，已有项目请在核销明细中查看。"
                                                            : "没有符合搜索条件的应收。"}
                                                    </TableCell>
                                                </TableRow>
                                            ) : (
                                                visible.map((target) => {
                                                    const source = sources.get(
                                                        allocationSourceKey(
                                                            target,
                                                        ),
                                                    )
                                                    const selected =
                                                        field.state.value.includes(
                                                            target.targetId,
                                                        )
                                                    return (
                                                        <TableRow
                                                            key={
                                                                target.targetId
                                                            }
                                                            data-state={
                                                                selected
                                                                    ? "selected"
                                                                    : undefined
                                                            }
                                                        >
                                                            <TableCell>
                                                                <NativeCheckbox
                                                                    id={`customer-receivables-picker-${toAutomationIdSegment(target.targetId)}-select`}
                                                                    aria-label={`选择 ${target.label}`}
                                                                    checked={
                                                                        selected
                                                                    }
                                                                    disabled={
                                                                        disabled
                                                                    }
                                                                    onCheckedChange={(
                                                                        checked,
                                                                    ) =>
                                                                        field.handleChange(
                                                                            checked
                                                                                ? [
                                                                                      ...field
                                                                                          .state
                                                                                          .value,
                                                                                      target.targetId,
                                                                                  ]
                                                                                : field.state.value.filter(
                                                                                      (
                                                                                          id,
                                                                                      ) =>
                                                                                          id !==
                                                                                          target.targetId,
                                                                                  ),
                                                                        )
                                                                    }
                                                                />
                                                            </TableCell>
                                                            <TableCell>
                                                                <ReceiptAllocationSource
                                                                    idPrefix="customer-receivables-picker-source"
                                                                    target={
                                                                        target
                                                                    }
                                                                    source={
                                                                        canRead
                                                                            ? source?.data
                                                                            : undefined
                                                                    }
                                                                    canRead={
                                                                        canRead
                                                                    }
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
                                                            </TableCell>
                                                            <TableCell className="num">
                                                                {target.dueDate ||
                                                                    "—"}
                                                            </TableCell>
                                                            <TableCell data-align="end">
                                                                <MoneyValue
                                                                    value={
                                                                        target.openAmount
                                                                    }
                                                                />
                                                            </TableCell>
                                                        </TableRow>
                                                    )
                                                })
                                            )}
                                        </TableBody>
                                    </Table>
                                )}
                            </form.Field>
                        </div>
                    </TableToolbarScope>
                    <DialogFooter>
                        <Button
                            id="customer-receivables-picker-cancel"
                            type="button"
                            variant="outline"
                            onClick={onClose}
                        >
                            取消
                        </Button>
                        <Button
                            id="customer-receivables-picker-confirm"
                            type="submit"
                            disabled={disabled || targetIds.length === 0}
                        >
                            添加
                            {targetIds.length > 0
                                ? ` ${targetIds.length} 笔应收`
                                : "应收"}
                        </Button>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
