"use client"
import * as React from "react"
import { Input } from "@/components/ui/input"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { OptionCombobox } from "@/components/business/option-combobox"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { BatchEditor } from "../../hooks/use-batch-supply-editor"
import {
    EXTRA_FIELDS,
    GRID_FIELDS,
    lockedRow,
    STATUS_LABELS,
    type BatchRow,
    type TextFieldKey,
} from "../../lib/batch-supply"
import { AVAILABILITY_STATUS_LABELS } from "../../types"

const availabilityOptions = Object.entries(AVAILABILITY_STATUS_LABELS).map(
    ([value, label]) => ({ value, label }),
)

export function BatchSupplyTable({
    editor,
    rows,
}: {
    editor: BatchEditor
    rows: BatchRow[]
}) {
    const fields =
        editor.mode === "availability"
            ? [{ key: "availableQuantity" as const, label: "可供数量" }]
            : GRID_FIELDS.filter(
                  (field) =>
                      editor.mode === "create" ||
                      field.key !== "supplierSkuCode",
              )
    function input(
        row: BatchRow,
        key: TextFieldKey,
        label: string,
        grid = false,
    ) {
        const id = `batch-supply-${toAutomationIdSegment(row.rowId)}-${toAutomationIdSegment(key)}`
        return (
            <Input
                id={id}
                aria-label={`${row.skuCode} ${label}`}
                aria-invalid={
                    row.status === "INVALID" || row.status === "FAILED"
                }
                aria-describedby={
                    row.message
                        ? `batch-supply-${toAutomationIdSegment(row.rowId)}-message`
                        : undefined
                }
                value={row[key]}
                type={
                    key === "validFrom" || key === "validTo" ? "date" : "text"
                }
                className="min-w-24"
                disabled={
                    editor.busy ||
                    lockedRow(row) ||
                    (key === "bulkPrice" && row.samePrice)
                }
                onChange={(event) =>
                    editor.editCell(row.rowId, key, event.target.value)
                }
                onPaste={(event) => {
                    const text = event.clipboardData.getData("text/plain")
                    if (grid && /[\t\n\r]/.test(text)) {
                        event.preventDefault()
                        editor.paste(
                            row.rowId,
                            key,
                            text,
                            fields.map((field) => field.key),
                        )
                    }
                }}
            />
        )
    }
    return (
        <div className="overflow-hidden rounded-lg border">
            <Table
                data-density="comfortable"
                className={
                    editor.mode === "availability"
                        ? "min-w-[620px]"
                        : "min-w-[1080px]"
                }
            >
                <TableHeader>
                    <TableRow>
                        <TableHead className="w-10">
                            <Checkbox
                                id="batch-supply-select-all"
                                aria-label="勾选全部未完成行"
                                nativeButton
                                render={
                                    <button
                                        type="button"
                                        aria-label="勾选全部未完成行"
                                    />
                                }
                                disabled={editor.busy}
                                checked={
                                    rows.some(
                                        (row) => row.status !== "SUCCEEDED",
                                    ) &&
                                    rows.every(
                                        (row) =>
                                            row.selected ||
                                            row.status === "SUCCEEDED",
                                    )
                                }
                                onCheckedChange={(checked) =>
                                    editor.form.setFieldValue(
                                        "rows",
                                        rows.map((row) =>
                                            row.status === "SUCCEEDED"
                                                ? row
                                                : {
                                                      ...row,
                                                      selected: checked,
                                                  },
                                        ),
                                    )
                                }
                            />
                        </TableHead>
                        <TableHead className="min-w-44">
                            公司商品 / SKU
                        </TableHead>
                        {fields.map((field) => (
                            <TableHead key={field.key} className="min-w-28">
                                {field.label}
                            </TableHead>
                        ))}
                        <TableHead className="min-w-32">可供状态</TableHead>
                        <TableHead className="min-w-32">处理结果</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {rows.map((row) => {
                        const id = `batch-supply-${toAutomationIdSegment(row.rowId)}`
                        const locked = lockedRow(row) || editor.busy
                        return (
                            <React.Fragment key={row.rowId}>
                                <TableRow>
                                    <TableCell className="align-top">
                                        <Checkbox
                                            id={`${id}-selected`}
                                            aria-label={`勾选 ${row.skuCode}`}
                                            nativeButton
                                            render={
                                                <button
                                                    type="button"
                                                    aria-label={`勾选 ${row.skuCode}`}
                                                />
                                            }
                                            disabled={
                                                editor.busy ||
                                                row.status === "SUCCEEDED"
                                            }
                                            checked={row.selected}
                                            onCheckedChange={(checked) =>
                                                editor.selectRow(
                                                    row.rowId,
                                                    checked,
                                                )
                                            }
                                        />
                                    </TableCell>
                                    <TableCell className="align-top">
                                        <p className="font-medium">
                                            {row.skuName}
                                        </p>
                                        <p className="mt-1 text-xs text-muted-foreground">
                                            {row.skuCode}
                                        </p>
                                        {editor.mode !== "create" && (
                                            <p className="text-xs text-muted-foreground">
                                                订货码：{row.supplierSkuCode}
                                            </p>
                                        )}
                                    </TableCell>
                                    {fields.map((field) => (
                                        <TableCell
                                            key={field.key}
                                            className="align-top"
                                        >
                                            {input(
                                                row,
                                                field.key,
                                                field.label,
                                                true,
                                            )}
                                        </TableCell>
                                    ))}
                                    <TableCell className="align-top">
                                        {editor.mode === "revise" ? (
                                            <span className="text-muted-foreground">
                                                保持现状
                                            </span>
                                        ) : (
                                            <OptionCombobox
                                                id={`${id}-availability`}
                                                aria-label={`${row.skuCode} 可供状态`}
                                                disabled={locked}
                                                allowClear={false}
                                                options={availabilityOptions}
                                                value={row.availabilityStatus}
                                                onValueChange={(value) => {
                                                    if (value) {
                                                        editor.editRow(
                                                            row.rowId,
                                                            {
                                                                availabilityStatus:
                                                                    value as BatchRow["availabilityStatus"],
                                                            },
                                                        )
                                                    }
                                                }}
                                            />
                                        )}
                                    </TableCell>
                                    <TableCell className="align-top">
                                        <span
                                            className={
                                                row.status === "SUCCEEDED"
                                                    ? "text-success-soft-foreground"
                                                    : row.message
                                                      ? "text-destructive"
                                                      : "text-muted-foreground"
                                            }
                                        >
                                            {STATUS_LABELS[row.status]}
                                        </span>
                                        {editor.mode === "create" &&
                                            !locked && (
                                                <Button
                                                    id={`${id}-remove`}
                                                    type="button"
                                                    variant="ghost"
                                                    size="sm"
                                                    onClick={() =>
                                                        editor.form.setFieldValue(
                                                            "rows",
                                                            rows.filter(
                                                                (item) =>
                                                                    item.rowId !==
                                                                    row.rowId,
                                                            ),
                                                        )
                                                    }
                                                >
                                                    移除
                                                </Button>
                                            )}
                                    </TableCell>
                                </TableRow>
                                <TableRow>
                                    <TableCell aria-label="行补充信息" />
                                    <TableCell
                                        colSpan={fields.length + 3}
                                        className="whitespace-normal"
                                    >
                                        {row.message && (
                                            <p
                                                id={`${id}-message`}
                                                className="mb-2 text-xs text-destructive"
                                                role="alert"
                                            >
                                                {row.message}
                                            </p>
                                        )}
                                        {editor.mode === "availability" ? (
                                            <label
                                                htmlFor={`${id}-unknown`}
                                                className="flex items-center gap-2 text-xs text-muted-foreground"
                                            >
                                                <Checkbox
                                                    id={`${id}-unknown`}
                                                    aria-label="数量未提供；0 表示明确为零"
                                                    nativeButton
                                                    render={
                                                        <button
                                                            type="button"
                                                            aria-label="数量未提供；0 表示明确为零"
                                                        />
                                                    }
                                                    disabled={locked}
                                                    checked={
                                                        row.quantityMode ===
                                                        "unknown"
                                                    }
                                                    onCheckedChange={(
                                                        checked,
                                                    ) =>
                                                        editor.editRow(
                                                            row.rowId,
                                                            {
                                                                quantityMode:
                                                                    checked
                                                                        ? "unknown"
                                                                        : "provided",
                                                                availableQuantity:
                                                                    "",
                                                            },
                                                        )
                                                    }
                                                />
                                                数量未提供；0 表示明确为零
                                            </label>
                                        ) : (
                                            <details>
                                                <summary
                                                    id={`${id}-details`}
                                                    className="w-fit cursor-pointer text-xs text-muted-foreground"
                                                >
                                                    日期、费用
                                                    {editor.mode === "create"
                                                        ? "与可供数量"
                                                        : ""}{" "}
                                                    ·{" "}
                                                    {row.validFrom ||
                                                        "未填生效日期"}{" "}
                                                    —{" "}
                                                    {row.validTo || "长期有效"}
                                                </summary>
                                                <div className="mt-3 grid grid-cols-3 gap-3">
                                                    {EXTRA_FIELDS.filter(
                                                        (field) =>
                                                            editor.mode ===
                                                                "create" ||
                                                            field.key !==
                                                                "supplierProductCode",
                                                    ).map((field) => (
                                                        <label
                                                            key={field.key}
                                                            htmlFor={`${id}-${toAutomationIdSegment(field.key)}`}
                                                            className="space-y-1 text-xs"
                                                        >
                                                            {field.label}
                                                            {input(
                                                                row,
                                                                field.key,
                                                                field.label,
                                                            )}
                                                        </label>
                                                    ))}
                                                    {editor.mode ===
                                                        "create" && (
                                                        <label
                                                            htmlFor={`${id}-availablequantity`}
                                                            className="space-y-1 text-xs"
                                                        >
                                                            可供数量（空白表示未提供）
                                                            {input(
                                                                row,
                                                                "availableQuantity",
                                                                "可供数量",
                                                            )}
                                                        </label>
                                                    )}
                                                    <label
                                                        htmlFor={`${id}-same-price`}
                                                        className="flex items-center gap-2 text-xs"
                                                    >
                                                        <Checkbox
                                                            id={`${id}-same-price`}
                                                            aria-label="两种价格相同"
                                                            nativeButton
                                                            render={
                                                                <button
                                                                    type="button"
                                                                    aria-label="两种价格相同"
                                                                />
                                                            }
                                                            disabled={locked}
                                                            checked={
                                                                row.samePrice
                                                            }
                                                            onCheckedChange={(
                                                                checked,
                                                            ) =>
                                                                editor.editRow(
                                                                    row.rowId,
                                                                    {
                                                                        samePrice:
                                                                            checked,
                                                                        bulkPrice:
                                                                            checked
                                                                                ? row.dropshipPrice
                                                                                : row.bulkPrice,
                                                                    },
                                                                )
                                                            }
                                                        />
                                                        两种价格相同
                                                    </label>
                                                </div>
                                            </details>
                                        )}
                                    </TableCell>
                                </TableRow>
                            </React.Fragment>
                        )
                    })}
                </TableBody>
            </Table>
            {!rows.length && (
                <div className="p-10 text-center text-muted-foreground">
                    先添加公司 SKU，或导入供给配置文件。每批最多 100 行。
                </div>
            )}
        </div>
    )
}
