"use client"

import { QuantityValue } from "@/components/business"
import { OptionCombobox } from "@/components/business/option-combobox"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
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
    AddressImportChoice,
    AddressImportGroup,
} from "../lib/address-import-plan"

const STATUS = {
    error: "待修正",
    conflict: "数量冲突，待选择",
    skip: "跳过，保持原数量",
    add: "新增 1 个地址",
    update: "更新已有地址数量",
}

export function InquiryAddressImportGroups({
    groups,
    id,
    unit,
    disabled,
    onChoice,
    onRemove,
}: {
    groups: readonly AddressImportGroup[]
    id: string
    unit?: string
    disabled: boolean
    onChoice: (group: AddressImportGroup, choice: AddressImportChoice) => void
    onRemove: (group: AddressImportGroup) => void
}) {
    return (
        <div className="overflow-hidden rounded-md border">
            <Table className="min-w-[1040px] table-fixed">
                <colgroup>
                    <col className="w-24" />
                    <col className="w-40" />
                    <col />
                    <col className="w-40" />
                    <col className="w-72" />
                    <col className="w-24" />
                </colgroup>
                <TableHeader>
                    <TableRow>
                        <TableHead>来源行</TableHead>
                        <TableHead>收件人 / 电话</TableHead>
                        <TableHead>配送地址</TableHead>
                        <TableHead>数量对比</TableHead>
                        <TableHead>处理方式</TableHead>
                        <TableHead className="text-right">操作</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {groups.map((group) => {
                        const rowId = `${id}-${toAutomationIdSegment(group.id)}`
                        const existing = group.existing[0]
                        const sourceLines = group.rows.map((row) => row.line)
                        const consecutive = sourceLines.every(
                            (line, index) => line === sourceLines[0] + index,
                        )
                        const sourceLabel =
                            consecutive && sourceLines.length > 1
                                ? `${sourceLines[0]}–${sourceLines[sourceLines.length - 1]}`
                                : sourceLines.join("、")
                        const options = [
                            {
                                value: "keep",
                                label: existing
                                    ? "保持原数量，跳过本组"
                                    : "跳过本组",
                            },
                        ]
                        if (group.existing.length <= 1) {
                            group.quantities.forEach((quantity, index) =>
                                options.push({
                                    value: `use-${index}`,
                                    label: `采用导入数量 ${quantity}`,
                                }),
                            )
                            options.push({
                                value: "custom",
                                label: "填写结果数量",
                            })
                            if (group.rows.length > 1 || existing)
                                options.push({
                                    value: "sum",
                                    label: "合并并累加数量",
                                })
                        }
                        const chosenIndex = group.quantities.findIndex(
                            (quantity) => quantity === group.choice?.quantity,
                        )
                        const selected =
                            group.choice?.mode === "replace"
                                ? chosenIndex >= 0
                                    ? `use-${chosenIndex}`
                                    : "custom"
                                : (group.choice?.mode ?? null)
                        return (
                            <TableRow
                                key={group.id}
                                className="[&>td]:py-3 [&>td]:align-top"
                            >
                                <TableCell
                                    className="text-xs leading-5"
                                    title={`来源：第 ${sourceLines.join("、")} 行`}
                                >
                                    <p className="font-medium tabular-nums">
                                        {group.rows.length} 行
                                    </p>
                                    <p className="truncate text-muted-foreground">
                                        第 {sourceLabel} 行
                                    </p>
                                </TableCell>
                                <TableCell className="space-y-1 whitespace-normal wrap-anywhere">
                                    <p className="font-medium">
                                        {group.allocation.recipient || "—"}
                                    </p>
                                    <p className="text-xs text-muted-foreground tabular-nums">
                                        {group.allocation.phone || "—"}
                                    </p>
                                </TableCell>
                                <TableCell className="space-y-1 whitespace-normal text-xs leading-5 wrap-anywhere">
                                    <p className="text-muted-foreground">
                                        {group.allocation.region_name ||
                                            [
                                                group.rows[0].province,
                                                group.rows[0].city,
                                                group.rows[0].district,
                                            ].join(" ")}
                                    </p>
                                    <p>{group.allocation.address || "—"}</p>
                                </TableCell>
                                <TableCell className="whitespace-normal text-xs leading-5">
                                    <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-1">
                                        <dt className="text-muted-foreground">
                                            原有
                                        </dt>
                                        <dd className="text-right tabular-nums wrap-anywhere">
                                            {group.existing.length
                                                ? group.existing
                                                      .map(
                                                          (row) => row.quantity,
                                                      )
                                                      .join("、")
                                                : "—"}
                                        </dd>
                                        <dt className="text-muted-foreground">
                                            导入
                                        </dt>
                                        <dd className="text-right tabular-nums wrap-anywhere">
                                            {group.quantities.join("、")}
                                        </dd>
                                    </dl>
                                </TableCell>
                                <TableCell className="space-y-2 whitespace-normal">
                                    <OptionCombobox
                                        id={`${rowId}-decision`}
                                        aria-label={`选择${group.allocation.recipient || "本组地址"}的处理方式`}
                                        placeholder="请选择处理方式"
                                        options={options}
                                        value={selected}
                                        allowClear={false}
                                        className="w-full"
                                        disabled={
                                            disabled || group.sourceInvalid
                                        }
                                        onValueChange={(value) => {
                                            if (!value) return
                                            if (
                                                value === "keep" ||
                                                value === "sum"
                                            )
                                                onChoice(group, {
                                                    signature: group.signature,
                                                    mode: value,
                                                })
                                            else
                                                onChoice(group, {
                                                    signature: group.signature,
                                                    mode: "replace",
                                                    quantity:
                                                        value === "custom"
                                                            ? (group.resultQuantity ??
                                                              group
                                                                  .quantities[0])
                                                            : (group.quantities.find(
                                                                  (_, index) =>
                                                                      value ===
                                                                      `use-${index}`,
                                                              ) ?? ""),
                                                })
                                        }}
                                    />
                                    {group.choice?.mode === "replace" ? (
                                        <div className="flex items-center justify-between gap-3">
                                            <label
                                                htmlFor={`${rowId}-quantity`}
                                                className="shrink-0 text-xs text-muted-foreground"
                                            >
                                                结果数量
                                            </label>
                                            <Input
                                                id={`${rowId}-quantity`}
                                                aria-label="结果配送数量"
                                                aria-describedby={`${rowId}-status`}
                                                inputMode="decimal"
                                                className="h-8 min-w-0 max-w-36 text-right tabular-nums"
                                                value={
                                                    group.choice.quantity ?? ""
                                                }
                                                disabled={disabled}
                                                onChange={(event) =>
                                                    onChoice(group, {
                                                        signature:
                                                            group.signature,
                                                        mode: "replace",
                                                        quantity:
                                                            event.target.value,
                                                    })
                                                }
                                            />
                                        </div>
                                    ) : (
                                        group.resultQuantity != null && (
                                            <p className="flex items-baseline justify-between gap-3 text-sm">
                                                <span className="shrink-0 text-xs text-muted-foreground">
                                                    结果数量
                                                </span>
                                                <QuantityValue
                                                    value={group.resultQuantity}
                                                    unit={unit}
                                                />
                                            </p>
                                        )
                                    )}
                                    {group.choice?.mode === "sum" && (
                                        <p className="text-xs text-muted-foreground">
                                            {existing
                                                ? `原 ${existing.quantity} + `
                                                : ""}
                                            本次 {group.rows.length} 行全部数量
                                        </p>
                                    )}
                                    <p
                                        id={`${rowId}-status`}
                                        className={`text-xs leading-5 ${group.status === "error" ? "text-destructive" : group.status === "conflict" ? "text-warning-soft-foreground" : "text-muted-foreground"}`}
                                    >
                                        {group.errors.length
                                            ? group.errors.join("；")
                                            : group.status === "skip" &&
                                                !existing
                                              ? "跳过本组"
                                              : STATUS[group.status]}
                                    </p>
                                    {group.existing.length > 1 && (
                                        <p className="text-xs text-muted-foreground">
                                            已有多条相同地址，须在地址列表明确修改目标。
                                        </p>
                                    )}
                                </TableCell>
                                <TableCell className="text-right">
                                    <Button
                                        id={`${rowId}-remove`}
                                        type="button"
                                        variant="ghost"
                                        size="sm"
                                        aria-label={`移除${group.allocation.recipient || "当前地址"}的整组导入记录`}
                                        disabled={disabled}
                                        onClick={() => onRemove(group)}
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
    )
}
