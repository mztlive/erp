"use client"

import * as React from "react"
import {
    ArrowRightIcon,
    FolderTreeIcon,
    MoreHorizontalIcon,
    RefreshCwIcon,
} from "lucide-react"
import { OptionCombobox } from "@/components/business"
import {
    TableToolbar,
    TableToolbarScope,
} from "@/components/business/table-toolbar"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import {
    Popover,
    PopoverContent,
    PopoverTrigger,
} from "@/components/ui/popover"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { useBatchWorkspace } from "../../hooks/use-batch-workspace"
import { useCategoryMappingList } from "../../hooks/use-category-mapping-list"
import { BatchClearButton } from "./batch-clear-button"

export type CategoryMappingListHandle = { showMissing: () => void }

export function BatchCategoryMappings({
    workspace,
    categories,
    listRef,
    disabled = false,
}: {
    workspace: ReturnType<typeof useBatchWorkspace>
    categories: Array<{ value: string; label: string }>
    listRef?: React.Ref<CategoryMappingListHandle>
    disabled?: boolean
}) {
    const { form, values } = workspace
    const rows = values.categoryMappings
    const single = rows.length === 1 ? rows[0] : null
    const list = useCategoryMappingList(rows)
    const searchRef = React.useRef<HTMLInputElement>(null)
    const sectionRef = React.useRef<HTMLElement>(null)
    const missingCount = rows.filter((row) => !row.categoryId).length
    const clearing = values.clearFields.includes("category_id")
    const nameCounts = new Map<string, number>()
    for (const row of rows)
        nameCounts.set(
            row.sourceName,
            (nameCounts.get(row.sourceName) ?? 0) + 1,
        )
    React.useImperativeHandle(listRef, () => ({
        showMissing: () => {
            list.refresh("", "missing")
            sectionRef.current?.scrollIntoView({
                block: "start",
                behavior: "smooth",
            })
            searchRef.current?.focus({ preventScroll: true })
        },
    }))
    const mappingField = (index: number) => {
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
                        disabled={disabled}
                        onValueChange={() => {
                            workspace.setDefaultClear("category_id", false)
                            list.setNotice(
                                `已设置「${row.sourceName}」的分类映射`,
                            )
                        }}
                        placeholder="搜索并选择公司分类"
                        className="min-w-0 w-full"
                        inputClassName="h-9"
                    />
                )}
            </form.AppField>
        )
    }
    const apply = () => {
        if (!list.unified || disabled) return
        const ids = new Set(list.applicable.map(({ row }) => row.sourceId))
        if (!ids.size) return
        workspace.setDefaultClear("category_id", false)
        form.setFieldValue("categoryMappings", (current) =>
            current.map((row) =>
                ids.has(row.sourceId)
                    ? { ...row, categoryId: list.unified ?? "" }
                    : row,
            ),
        )
        list.setNotice(
            `已更新 ${ids.size} 类映射${!list.overwrite && list.mappedCount ? `，保留 ${list.mappedCount} 类已有映射` : ""}`,
        )
        list.setSelected(new Set())
        list.setOverwrite(false)
    }
    return (
        <section
            ref={sectionRef}
            className="min-w-0 scroll-mt-4 rounded-lg border"
            aria-labelledby="supplier-catalog-batch-category-title"
        >
            <div className="flex flex-wrap items-center gap-2 px-3 py-2.5">
                <div className="mr-1 flex shrink-0 items-center gap-1.5">
                    <FolderTreeIcon
                        className="size-4 text-muted-foreground"
                        aria-hidden
                    />
                    <h2
                        id="supplier-catalog-batch-category-title"
                        className="text-sm font-semibold"
                    >
                        公司分类
                    </h2>
                </div>
                {rows.length > 1 ? (
                    <>
                        <Input
                            ref={searchRef}
                            id="supplier-catalog-batch-category-search"
                            aria-label="搜索来源分类名称或编码"
                            placeholder="搜索分类名称或编码"
                            value={list.keyword}
                            onChange={(event) =>
                                list.refresh(event.target.value)
                            }
                            className="h-8 min-w-32 max-w-64 flex-1"
                        />
                        <div
                            className="ml-auto flex items-center gap-0.5"
                            role="group"
                            aria-label="分类映射状态"
                        >
                            {(
                                [
                                    ["all", "全部", rows.length],
                                    ["missing", "未设置", missingCount],
                                    [
                                        "mapped",
                                        "已设置",
                                        rows.length - missingCount,
                                    ],
                                ] as const
                            ).map(([status, label, count]) => (
                                <Button
                                    key={status}
                                    id={`supplier-catalog-batch-category-status-${status}`}
                                    type="button"
                                    variant={
                                        list.status === status
                                            ? "secondary"
                                            : "ghost"
                                    }
                                    size="xs"
                                    aria-pressed={list.status === status}
                                    onClick={() =>
                                        list.refresh(list.keyword, status)
                                    }
                                >
                                    {label} {count}
                                </Button>
                            ))}
                        </div>
                    </>
                ) : (
                    <span className="ml-auto text-xs text-muted-foreground">
                        共 {rows.length} 类
                    </span>
                )}
                {rows.length > 1 && !clearing ? (
                    <Popover>
                        <PopoverTrigger
                            id="supplier-catalog-batch-category-more"
                            aria-label="更多操作"
                            title="更多操作"
                            disabled={disabled}
                            render={
                                <Button
                                    type="button"
                                    variant="ghost"
                                    size="icon-sm"
                                />
                            }
                        >
                            <MoreHorizontalIcon aria-hidden />
                        </PopoverTrigger>
                        <PopoverContent
                            align="end"
                            className="w-auto rounded-lg p-1"
                        >
                            <BatchClearButton
                                workspace={workspace}
                                field="category_id"
                                label="清除本批分类映射"
                                disabled={disabled}
                            />
                        </PopoverContent>
                    </Popover>
                ) : null}
            </div>
            {clearing ? (
                <div
                    className="mx-3 mb-2 flex flex-wrap items-center justify-between gap-2 rounded-md bg-muted px-2 py-1 text-xs"
                    role="status"
                >
                    本批已选择清除分类映射
                    <BatchClearButton
                        workspace={workspace}
                        field="category_id"
                        disabled={disabled}
                    />
                </div>
            ) : null}
            {single ? (
                <div className="flex flex-wrap items-center gap-3 border-t p-4">
                    <div className="flex min-w-0 items-center gap-2 text-sm">
                        <span className="wrap-anywhere">
                            {single.sourceName}
                        </span>
                        <span className="text-xs text-muted-foreground">
                            {single.count} 个 SKU
                        </span>
                        <ArrowRightIcon
                            className="size-4 shrink-0 text-muted-foreground"
                            aria-hidden
                        />
                    </div>
                    <div className="min-w-56 flex-1">{mappingField(0)}</div>
                    {!clearing ? (
                        <BatchClearButton
                            workspace={workspace}
                            field="category_id"
                            disabled={disabled}
                        />
                    ) : null}
                </div>
            ) : rows.length === 0 ? (
                <p className="p-4 text-sm text-muted-foreground">
                    暂无待映射分类
                </p>
            ) : (
                <TableToolbarScope>
                    <TableToolbar
                        className="gap-x-2 px-3 pt-0 pb-2 [&_[data-slot=table-toolbar-summary]]:flex-1"
                        actions={
                            <Button
                                id="supplier-catalog-batch-category-refresh"
                                type="button"
                                variant="ghost"
                                size="icon-sm"
                                aria-label="刷新列表"
                                title="刷新列表"
                                onClick={() => list.refresh()}
                            >
                                <RefreshCwIcon aria-hidden />
                            </Button>
                        }
                    >
                        <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
                            <span className="shrink-0 text-xs text-muted-foreground">
                                {list.chosen.length
                                    ? `已选 ${list.chosen.length} 类 · 已设 ${list.mappedCount}`
                                    : `当前 ${list.visible.length} 类`}
                            </span>
                            <OptionCombobox
                                id="supplier-catalog-batch-unified-category"
                                aria-label="批量设置公司分类"
                                options={categories}
                                value={list.unified}
                                onValueChange={list.setUnified}
                                placeholder="批量设置公司分类"
                                disabled={disabled}
                                className="w-44 min-w-32 flex-1 sm:max-w-56"
                                size="sm"
                            />
                            <Button
                                id="supplier-catalog-batch-apply-unified-category"
                                type="button"
                                variant="secondary"
                                size="sm"
                                disabled={
                                    disabled ||
                                    !list.unified ||
                                    !list.applicable.length
                                }
                                onClick={apply}
                            >
                                应用至 {list.applicable.length} 类
                            </Button>
                            <label
                                htmlFor="supplier-catalog-batch-category-overwrite"
                                className="flex shrink-0 items-center gap-1.5 text-xs text-muted-foreground"
                            >
                                <Checkbox
                                    id="supplier-catalog-batch-category-overwrite"
                                    className="rounded-sm border-input"
                                    checked={list.overwrite}
                                    disabled={disabled}
                                    onCheckedChange={(checked) =>
                                        list.setOverwrite(checked === true)
                                    }
                                />
                                覆盖已有映射
                            </label>
                        </div>
                    </TableToolbar>
                    <div className="border-t [&_[data-slot=table-container]]:max-h-[clamp(16rem,calc(100dvh-21rem),36rem)] [&_[data-slot=table-container]]:overflow-auto">
                        <Table className="min-w-[36rem] table-fixed">
                            <TableHeader className="sticky top-0 z-10">
                                <TableRow>
                                    <TableHead className="w-10">
                                        <Checkbox
                                            id="supplier-catalog-batch-categories-select-page"
                                            className="rounded-sm border-input"
                                            aria-label={`选择当前结果中的 ${list.visible.length} 个来源分类`}
                                            disabled={
                                                !list.visible.length || disabled
                                            }
                                            checked={
                                                list.visible.length > 0 &&
                                                list.chosen.length ===
                                                    list.visible.length
                                            }
                                            indeterminate={
                                                list.chosen.length > 0 &&
                                                list.chosen.length <
                                                    list.visible.length
                                            }
                                            onCheckedChange={(checked) =>
                                                list.setSelected(
                                                    checked === true
                                                        ? new Set(
                                                              list.visible.map(
                                                                  ({ row }) =>
                                                                      row.sourceId,
                                                              ),
                                                          )
                                                        : new Set(),
                                                )
                                            }
                                        />
                                    </TableHead>
                                    <TableHead className="w-[34%]">
                                        来源分类
                                    </TableHead>
                                    <TableHead className="w-20 text-right">
                                        SKU 数
                                    </TableHead>
                                    <TableHead>公司分类</TableHead>
                                </TableRow>
                            </TableHeader>
                            <TableBody>
                                {list.visible.map(({ row, index }) => {
                                    const key = toAutomationIdSegment(
                                        row.sourceId || "uncategorized",
                                    )
                                    return (
                                        <TableRow
                                            key={row.sourceId}
                                            data-state={
                                                list.selected.has(row.sourceId)
                                                    ? "selected"
                                                    : undefined
                                            }
                                        >
                                            <TableCell className="h-12 py-1.5">
                                                <Checkbox
                                                    id={`supplier-catalog-batch-category-select-${key}`}
                                                    className="rounded-sm border-input"
                                                    aria-label={`选择${row.sourceName}${(nameCounts.get(row.sourceName) ?? 0) > 1 ? `（${row.sourceId || "无来源编码"}）` : ""}`}
                                                    checked={list.selected.has(
                                                        row.sourceId,
                                                    )}
                                                    disabled={disabled}
                                                    onCheckedChange={(
                                                        checked,
                                                    ) =>
                                                        list.setSelected(
                                                            (current) => {
                                                                const next =
                                                                    new Set(
                                                                        current,
                                                                    )
                                                                if (
                                                                    checked ===
                                                                    true
                                                                )
                                                                    next.add(
                                                                        row.sourceId,
                                                                    )
                                                                else
                                                                    next.delete(
                                                                        row.sourceId,
                                                                    )
                                                                return next
                                                            },
                                                        )
                                                    }
                                                />
                                            </TableCell>
                                            <TableCell className="h-12 whitespace-normal py-1.5">
                                                <div className="wrap-anywhere leading-5">
                                                    {row.sourceName}
                                                </div>
                                                {(nameCounts.get(
                                                    row.sourceName,
                                                ) ?? 0) > 1 ? (
                                                    <div className="wrap-anywhere text-xs text-muted-foreground">
                                                        {row.sourceId
                                                            ? `来源编码：${row.sourceId}`
                                                            : "来源未提供分类编码"}
                                                    </div>
                                                ) : null}
                                            </TableCell>
                                            <TableCell className="num h-12 py-1.5 text-right text-xs text-muted-foreground">
                                                {row.count}
                                            </TableCell>
                                            <TableCell className="h-12 py-1.5">
                                                {mappingField(index)}
                                            </TableCell>
                                        </TableRow>
                                    )
                                })}
                                {!list.visible.length ? (
                                    <TableRow>
                                        <TableCell
                                            colSpan={4}
                                            className="py-10 text-center"
                                        >
                                            <p className="mb-2 text-sm text-muted-foreground">
                                                没有符合条件的来源分类
                                            </p>
                                            <Button
                                                id="supplier-catalog-batch-category-reset"
                                                type="button"
                                                variant="outline"
                                                size="sm"
                                                onClick={() =>
                                                    list.refresh("", "all")
                                                }
                                            >
                                                重置筛选
                                            </Button>
                                        </TableCell>
                                    </TableRow>
                                ) : null}
                            </TableBody>
                        </Table>
                    </div>
                </TableToolbarScope>
            )}
            <p
                role="status"
                aria-live="polite"
                className="px-4 text-xs text-muted-foreground empty:hidden not-empty:pb-3"
            >
                {list.notice}
            </p>
        </section>
    )
}
