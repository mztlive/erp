"use client"

import { PencilIcon, ReplaceIcon, Trash2Icon } from "lucide-react"
import { QuantityValue } from "@/components/business"
import { TableRowActions } from "@/components/business/table-row-actions"
import {
    TableToolbar,
    TableToolbarScope,
} from "@/components/business/table-toolbar"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { NativeCheckbox } from "@/components/ui/checkbox"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { RegionName } from "@/features/regions"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { allocationRestrictions } from "../lib/delivery-progress"
import type { InquiryAllocation, SalesInquiry } from "../types"

export function InquiryDeliveryAddressList({
    prefix,
    allocations,
    unit,
    inquiry,
    selectedIds,
    disabled,
    onSelectionChange,
    onAdd,
    onImport,
    onEdit,
    onRemove,
}: {
    prefix: string
    allocations: readonly InquiryAllocation[]
    unit?: string
    inquiry?: SalesInquiry
    selectedIds: ReadonlySet<string>
    disabled: boolean
    onSelectionChange: (ids: Set<string>) => void
    onAdd: () => void
    onImport: () => void
    onEdit: (allocation: InquiryAllocation, replace?: boolean) => void
    onRemove: (ids: ReadonlySet<string>) => void
}) {
    const selectable = allocations.filter(
        (item) => !allocationRestrictions(inquiry, item.allocation_id).occupied,
    )
    const selected = selectable.filter((item) =>
        selectedIds.has(item.allocation_id),
    )
    return (
        <TableToolbarScope>
            <section aria-label="配送地址" className="space-y-2">
                <TableToolbar
                    actions={
                        <>
                            <Button
                                id={`${prefix}-import-addresses`}
                                type="button"
                                size="sm"
                                variant="outline"
                                disabled={disabled}
                                onClick={onImport}
                            >
                                批量添加
                            </Button>
                            <Button
                                id={`${prefix}-add-address`}
                                type="button"
                                size="sm"
                                variant="outline"
                                disabled={disabled || allocations.length >= 100}
                                onClick={onAdd}
                            >
                                添加地址
                            </Button>
                        </>
                    }
                >
                    <div className="flex flex-wrap items-center gap-3">
                        <span className="font-medium">
                            配送地址 · {allocations.length}/100
                        </span>
                        {selected.length > 0 && (
                            <>
                                <span className="text-muted-foreground">
                                    已选 {selected.length} 项
                                </span>
                                <Button
                                    id={`${prefix}-remove-selected-addresses`}
                                    type="button"
                                    variant="ghost"
                                    size="sm"
                                    disabled={disabled}
                                    onClick={() =>
                                        onRemove(
                                            new Set(
                                                selected.map(
                                                    (item) =>
                                                        item.allocation_id,
                                                ),
                                            ),
                                        )
                                    }
                                >
                                    <Trash2Icon aria-hidden="true" />
                                    批量移除
                                </Button>
                                <Button
                                    id={`${prefix}-clear-address-selection`}
                                    type="button"
                                    variant="ghost"
                                    size="sm"
                                    onClick={() => onSelectionChange(new Set())}
                                >
                                    清空选择
                                </Button>
                            </>
                        )}
                    </div>
                </TableToolbar>
                {allocations.length === 0 ? (
                    <p className="rounded-md border border-dashed px-4 py-8 text-center text-sm text-muted-foreground">
                        暂无配送地址。添加时默认分配剩余需求数量。
                    </p>
                ) : (
                    <div className="overflow-hidden rounded-md border">
                        <Table className="min-w-[480px] table-fixed">
                            <TableHeader>
                                <TableRow>
                                    <TableHead className="w-10">
                                        <NativeCheckbox
                                            id={`${prefix}-select-all-addresses`}
                                            aria-label="全选可移除的地址"
                                            checked={
                                                selectable.length > 0 &&
                                                selected.length ===
                                                    selectable.length
                                            }
                                            indeterminate={
                                                selected.length > 0 &&
                                                selected.length <
                                                    selectable.length
                                            }
                                            disabled={
                                                disabled ||
                                                selectable.length === 0
                                            }
                                            onCheckedChange={(checked) =>
                                                onSelectionChange(
                                                    new Set(
                                                        checked
                                                            ? selectable.map(
                                                                  (item) =>
                                                                      item.allocation_id,
                                                              )
                                                            : [],
                                                    ),
                                                )
                                            }
                                        />
                                    </TableHead>
                                    <TableHead>收货信息</TableHead>
                                    <TableHead className="w-24 text-right">
                                        数量
                                    </TableHead>
                                    <TableHead className="w-56 text-right">
                                        操作
                                    </TableHead>
                                </TableRow>
                            </TableHeader>
                            <TableBody>
                                {allocations.map((allocation) => {
                                    const id = allocation.allocation_id
                                    const rowPrefix = `${prefix}-address-${toAutomationIdSegment(id)}`
                                    const { frozen, occupied } =
                                        allocationRestrictions(inquiry, id)
                                    const incomplete =
                                        !allocation.recipient.trim() ||
                                        !allocation.phone.trim() ||
                                        !allocation.region_code ||
                                        !allocation.address.trim()
                                    return (
                                        <TableRow
                                            key={id}
                                            data-state={
                                                selectedIds.has(id)
                                                    ? "selected"
                                                    : undefined
                                            }
                                        >
                                            <TableCell>
                                                <NativeCheckbox
                                                    id={`${rowPrefix}-select`}
                                                    aria-label={`选择${allocation.recipient || "未填写收货人"}的地址`}
                                                    title={
                                                        occupied
                                                            ? "已有销售占用，不可移除"
                                                            : undefined
                                                    }
                                                    checked={
                                                        !occupied &&
                                                        selectedIds.has(id)
                                                    }
                                                    disabled={
                                                        disabled || occupied
                                                    }
                                                    onCheckedChange={(
                                                        checked,
                                                    ) => {
                                                        const next = new Set(
                                                            selectedIds,
                                                        )
                                                        if (checked)
                                                            next.add(id)
                                                        else next.delete(id)
                                                        onSelectionChange(next)
                                                    }}
                                                />
                                            </TableCell>
                                            <TableCell className="whitespace-normal py-3">
                                                <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
                                                    <span className="font-medium">
                                                        {allocation.recipient ||
                                                            "未填写收货人"}
                                                    </span>
                                                    <span className="text-xs text-muted-foreground">
                                                        {allocation.phone ||
                                                            "未填写电话"}
                                                    </span>
                                                    {occupied ? (
                                                        <Badge variant="secondary">
                                                            已占用
                                                        </Badge>
                                                    ) : incomplete ? (
                                                        <Badge variant="warning">
                                                            待补全
                                                        </Badge>
                                                    ) : null}
                                                </div>
                                                <p className="mt-1 text-xs leading-5 text-muted-foreground wrap-anywhere">
                                                    {allocation.region_name ||
                                                        (allocation.region_code ? (
                                                            <RegionName
                                                                code={
                                                                    allocation.region_code
                                                                }
                                                            />
                                                        ) : (
                                                            "未选择地区"
                                                        ))}{" "}
                                                    {allocation.address ||
                                                        "未填写详细地址"}
                                                </p>
                                            </TableCell>
                                            <TableCell className="text-right">
                                                <QuantityValue
                                                    value={allocation.quantity}
                                                    unit={unit}
                                                />
                                            </TableCell>
                                            <TableCell>
                                                <TableRowActions
                                                    moreId={`${rowPrefix}-more`}
                                                    moreIconOnly
                                                    moreLabel={`${allocation.recipient || "配送地址"}更多操作`}
                                                    actions={[
                                                        {
                                                            id: `${rowPrefix}-edit`,
                                                            label: frozen
                                                                ? "调整数量"
                                                                : "编辑",
                                                            icon: PencilIcon,
                                                            disabled,
                                                            onClick: () =>
                                                                onEdit(
                                                                    allocation,
                                                                ),
                                                        },
                                                        {
                                                            id: `${rowPrefix}-replace`,
                                                            label: "替换地址",
                                                            icon: ReplaceIcon,
                                                            hidden: !frozen,
                                                            disabled:
                                                                disabled ||
                                                                occupied,
                                                            disabledReason:
                                                                occupied
                                                                    ? "已有销售占用，不可替换地址"
                                                                    : undefined,
                                                            placement: "inline",
                                                            onClick: () =>
                                                                onEdit(
                                                                    allocation,
                                                                    true,
                                                                ),
                                                        },
                                                        {
                                                            id: `${rowPrefix}-remove`,
                                                            label: "移除地址",
                                                            icon: Trash2Icon,
                                                            destructive: true,
                                                            disabled:
                                                                disabled ||
                                                                occupied,
                                                            disabledReason:
                                                                occupied
                                                                    ? "已有销售占用，不可移除地址"
                                                                    : undefined,
                                                            onClick: () =>
                                                                onRemove(
                                                                    new Set([
                                                                        id,
                                                                    ]),
                                                                ),
                                                        },
                                                    ]}
                                                />
                                            </TableCell>
                                        </TableRow>
                                    )
                                })}
                            </TableBody>
                        </Table>
                    </div>
                )}
                {selectable.length < allocations.length && (
                    <p className="text-xs text-muted-foreground">
                        已有销售占用的地址不参与批量移除。
                    </p>
                )}
            </section>
        </TableToolbarScope>
    )
}
