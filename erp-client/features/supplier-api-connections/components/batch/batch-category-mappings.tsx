"use client"

import * as React from "react"
import { ArrowRightIcon, FolderTreeIcon } from "lucide-react"
import { OptionCombobox } from "@/components/business"
import {
    TableToolbar,
    TableToolbarScope,
} from "@/components/business/table-toolbar"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import type { useBatchWorkspace } from "../../hooks/use-batch-workspace"
import { BatchClearButton } from "./batch-clear-button"

export function BatchCategoryMappings({
    workspace,
    categories,
}: {
    workspace: ReturnType<typeof useBatchWorkspace>
    categories: Array<{ value: string; label: string }>
}) {
    const { form, values } = workspace
    const rows = values.categoryMappings
    const single = rows.length === 1 ? rows[0] : null
    const [selected, setSelected] = React.useState<Set<string>>(new Set())
    const [unified, setUnified] = React.useState<string | null>(null)
    const selectedCount = rows.filter((row) =>
        selected.has(row.sourceId),
    ).length
    const missingCount = rows.filter((row) => !row.categoryId).length
    const removeClear = () => workspace.setDefaultClear("category_id", false)
    const mappingField = (index: number, className: string) => {
        const row = rows[index]
        const key = toAutomationIdSegment(row.sourceId || "uncategorized")
        return (
            <form.AppField name={`categoryMappings[${index}].categoryId`}>
                {(field) => (
                    <field.SelectField
                        id={`supplier-catalog-batch-category-map-${key}`}
                        label={`${row.sourceName}对应的公司分类`}
                        hideLabel
                        options={categories}
                        allowClear={false}
                        onValueChange={removeClear}
                        placeholder="选择公司分类"
                        className={className}
                    />
                )}
            </form.AppField>
        )
    }
    return (
        <section
            className={cn(
                "min-w-0 rounded-lg border p-4",
                single
                    ? "flex flex-wrap items-center gap-x-6 gap-y-3"
                    : "space-y-3",
            )}
            aria-labelledby="supplier-catalog-batch-category-title"
        >
            <div className="flex shrink-0 items-center gap-2">
                <FolderTreeIcon
                    className="size-4 text-muted-foreground"
                    aria-hidden
                />
                <h2
                    id="supplier-catalog-batch-category-title"
                    className="text-base font-semibold"
                >
                    公司分类
                </h2>
                {rows.length > 1 ? (
                    <div className="ml-auto">
                        <BatchClearButton
                            workspace={workspace}
                            field="category_id"
                            label="清除全部"
                        />
                    </div>
                ) : null}
            </div>
            {single ? (
                <div className="flex min-w-0 flex-1 flex-wrap items-center gap-3">
                    <div className="flex min-w-0 items-center gap-2 text-sm">
                        <span className="wrap-anywhere">
                            {single.sourceName}
                        </span>
                        <span className="shrink-0 text-xs text-muted-foreground">
                            {single.count} 个 SKU
                        </span>
                        <ArrowRightIcon
                            className="size-4 shrink-0 text-muted-foreground"
                            aria-hidden
                        />
                    </div>
                    {mappingField(0, "w-full sm:w-72")}
                    <BatchClearButton
                        workspace={workspace}
                        field="category_id"
                    />
                    {!single.categoryId &&
                    !values.clearFields.includes("category_id") ? (
                        <span className="text-xs text-warning">待选择</span>
                    ) : null}
                </div>
            ) : rows.length === 0 ? (
                <p className="text-sm text-muted-foreground">暂无待映射分类</p>
            ) : (
                <TableToolbarScope>
                    <TableToolbar
                        actions={
                            selectedCount > 0 ? (
                                <>
                                    <OptionCombobox
                                        id="supplier-catalog-batch-unified-category"
                                        aria-label="批量设置公司分类"
                                        options={categories}
                                        value={unified}
                                        onValueChange={setUnified}
                                        placeholder="批量设置公司分类"
                                        className="w-full sm:w-64"
                                    />
                                    <Button
                                        id="supplier-catalog-batch-apply-unified-category"
                                        type="button"
                                        variant="secondary"
                                        size="sm"
                                        disabled={!unified}
                                        onClick={() => {
                                            removeClear()
                                            form.setFieldValue(
                                                "categoryMappings",
                                                (current) =>
                                                    current.map((row) =>
                                                        selected.has(
                                                            row.sourceId,
                                                        )
                                                            ? {
                                                                  ...row,
                                                                  categoryId:
                                                                      unified ??
                                                                      "",
                                                              }
                                                            : row,
                                                    ),
                                            )
                                        }}
                                    >
                                        应用至 {selectedCount} 类
                                    </Button>
                                </>
                            ) : undefined
                        }
                    >
                        <span className="text-xs text-muted-foreground">
                            {rows.length} 个来源分类
                            {missingCount > 0 ? (
                                <span className="ml-2 text-warning">
                                    {missingCount} 个待补
                                </span>
                            ) : null}
                            {selectedCount > 0 ? (
                                <span className="ml-2">
                                    已选 {selectedCount} 个
                                </span>
                            ) : null}
                        </span>
                    </TableToolbar>
                    <Table>
                        <TableHeader>
                            <TableRow>
                                <TableHead className="w-10">
                                    <Checkbox
                                        id="supplier-catalog-batch-categories-select-page"
                                        aria-label="选择全部来源分类"
                                        checked={selectedCount === rows.length}
                                        indeterminate={
                                            selectedCount > 0 &&
                                            selectedCount < rows.length
                                        }
                                        onCheckedChange={(checked) =>
                                            setSelected(
                                                checked === true
                                                    ? new Set(
                                                          rows.map(
                                                              (row) =>
                                                                  row.sourceId,
                                                          ),
                                                      )
                                                    : new Set(),
                                            )
                                        }
                                    />
                                </TableHead>
                                <TableHead>来源分类</TableHead>
                                <TableHead>公司分类</TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {rows.map((row, index) => {
                                const key = toAutomationIdSegment(
                                    row.sourceId || "uncategorized",
                                )
                                return (
                                    <TableRow key={row.sourceId}>
                                        <TableCell>
                                            <Checkbox
                                                id={`supplier-catalog-batch-category-select-${key}`}
                                                aria-label={`选择${row.sourceName}`}
                                                checked={selected.has(
                                                    row.sourceId,
                                                )}
                                                onCheckedChange={(checked) =>
                                                    setSelected((current) => {
                                                        const next = new Set(
                                                            current,
                                                        )
                                                        if (checked === true)
                                                            next.add(
                                                                row.sourceId,
                                                            )
                                                        else
                                                            next.delete(
                                                                row.sourceId,
                                                            )
                                                        return next
                                                    })
                                                }
                                            />
                                        </TableCell>
                                        <TableCell>
                                            <span>{row.sourceName}</span>
                                            <span className="ml-3 text-xs text-muted-foreground">
                                                {row.count} 个 SKU
                                            </span>
                                        </TableCell>
                                        <TableCell>
                                            {mappingField(
                                                index,
                                                "min-w-56 max-w-md",
                                            )}
                                        </TableCell>
                                    </TableRow>
                                )
                            })}
                        </TableBody>
                    </Table>
                </TableToolbarScope>
            )}
        </section>
    )
}
