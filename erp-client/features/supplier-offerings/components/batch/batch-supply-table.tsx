"use client"
import * as React from "react"
import { Input } from "@/components/ui/input"
import { Button } from "@/components/ui/button"
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
        <div className="overflow-x-auto rounded-lg border">
            <table
                className={
                    editor.mode === "availability"
                        ? "w-full min-w-[620px] text-sm"
                        : "w-full min-w-[1080px] text-sm"
                }
            >
                <thead className="bg-muted/40">
                    <tr>
                        <th className="w-10 p-3">
                            <input
                                id="batch-supply-select-all"
                                type="checkbox"
                                aria-label="勾选全部未完成行"
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
                                onChange={(event) =>
                                    editor.form.setFieldValue(
                                        "rows",
                                        rows.map((row) =>
                                            row.status === "SUCCEEDED"
                                                ? row
                                                : {
                                                      ...row,
                                                      selected:
                                                          event.target.checked,
                                                  },
                                        ),
                                    )
                                }
                            />
                        </th>
                        <th className="min-w-44 p-3 text-left">
                            公司商品 / SKU
                        </th>
                        {fields.map((field) => (
                            <th
                                key={field.key}
                                className="min-w-28 p-2 text-left text-xs"
                            >
                                {field.label}
                            </th>
                        ))}
                        <th className="min-w-32 p-2 text-left">可供状态</th>
                        <th className="min-w-32 p-2 text-left">处理结果</th>
                    </tr>
                </thead>
                <tbody>
                    {rows.map((row) => {
                        const id = `batch-supply-${toAutomationIdSegment(row.rowId)}`
                        const locked = lockedRow(row) || editor.busy
                        return (
                            <React.Fragment key={row.rowId}>
                                <tr className="border-t align-top">
                                    <td className="p-3">
                                        <input
                                            id={`${id}-selected`}
                                            type="checkbox"
                                            aria-label={`勾选 ${row.skuCode}`}
                                            disabled={
                                                editor.busy ||
                                                row.status === "SUCCEEDED"
                                            }
                                            checked={row.selected}
                                            onChange={(event) =>
                                                editor.selectRow(
                                                    row.rowId,
                                                    event.target.checked,
                                                )
                                            }
                                        />
                                    </td>
                                    <td className="p-3">
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
                                    </td>
                                    {fields.map((field) => (
                                        <td key={field.key} className="p-2">
                                            {input(
                                                row,
                                                field.key,
                                                field.label,
                                                true,
                                            )}
                                        </td>
                                    ))}
                                    <td className="p-2">
                                        {editor.mode === "revise" ? (
                                            <span className="text-muted-foreground">
                                                保持现状
                                            </span>
                                        ) : (
                                            <select
                                                id={`${id}-availability`}
                                                aria-label={`${row.skuCode} 可供状态`}
                                                disabled={locked}
                                                className="h-9 w-full rounded-md border bg-background px-2"
                                                value={row.availabilityStatus}
                                                onChange={(event) =>
                                                    editor.editRow(row.rowId, {
                                                        availabilityStatus:
                                                            event.target
                                                                .value as BatchRow["availabilityStatus"],
                                                    })
                                                }
                                            >
                                                {Object.entries(
                                                    AVAILABILITY_STATUS_LABELS,
                                                ).map(([key, label]) => (
                                                    <option
                                                        key={key}
                                                        value={key}
                                                    >
                                                        {label}
                                                    </option>
                                                ))}
                                            </select>
                                        )}
                                    </td>
                                    <td className="p-2">
                                        <span
                                            className={
                                                row.status === "SUCCEEDED"
                                                    ? "text-emerald-700"
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
                                    </td>
                                </tr>
                                <tr>
                                    <td aria-label="行补充信息" />
                                    <td
                                        colSpan={fields.length + 3}
                                        className="px-3 pb-3"
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
                                                <input
                                                    id={`${id}-unknown`}
                                                    type="checkbox"
                                                    disabled={locked}
                                                    checked={
                                                        row.quantityMode ===
                                                        "unknown"
                                                    }
                                                    onChange={(event) =>
                                                        editor.editRow(
                                                            row.rowId,
                                                            {
                                                                quantityMode:
                                                                    event.target
                                                                        .checked
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
                                                        <input
                                                            id={`${id}-same-price`}
                                                            type="checkbox"
                                                            disabled={locked}
                                                            checked={
                                                                row.samePrice
                                                            }
                                                            onChange={(event) =>
                                                                editor.editRow(
                                                                    row.rowId,
                                                                    {
                                                                        samePrice:
                                                                            event
                                                                                .target
                                                                                .checked,
                                                                        bulkPrice:
                                                                            event
                                                                                .target
                                                                                .checked
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
                                    </td>
                                </tr>
                            </React.Fragment>
                        )
                    })}
                </tbody>
            </table>
            {!rows.length && (
                <div className="p-10 text-center text-muted-foreground">
                    先添加公司 SKU，或导入供给配置文件。每批最多 100 行。
                </div>
            )}
        </div>
    )
}
