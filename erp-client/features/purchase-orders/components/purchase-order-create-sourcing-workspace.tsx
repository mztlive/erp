"use client"

import { useMemo, useState, type ReactNode } from "react"
import { SearchIcon, XIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import type { PaginationState } from "@/components/business/data-table"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { buildSourcingEditorRows } from "../lib/sourcing/editor-rows"
import type {
    SourcingEditorProps,
    SourcingBatchSelectionProps,
    SourcingParticipationProps,
} from "./create-sourcing/types"
import { SourcingProductList } from "./sourcing-product-list"
import { SourcingProductAdjustDialog } from "./sourcing-product-adjust-dialog"

type StatusFilter = "all" | "pending" | "excluded"

/** 全宽商品表格负责对比和定位；来源编辑复用整单表单，筛选与分页不修改草稿。 */
export function PurchaseOrderCreateSourcingWorkspace({
    toolbar,
    selectedProductIds,
    onToggleProducts,
    onSetParticipation,
    ...props
}: SourcingEditorProps &
    SourcingBatchSelectionProps &
    SourcingParticipationProps & { toolbar: ReactNode }) {
    const { form, order } = props
    const [query, setQuery] = useState("")
    const [status, setStatus] = useState<StatusFilter>("all")
    const [pagination, setPagination] = useState<PaginationState>({
        pageIndex: 0,
        pageSize: 50,
    })
    const [editingId, setEditingId] = useState<string | null>(null)
    const [returnFocusId, setReturnFocusId] = useState<string>()
    const [navigationIds, setNavigationIds] = useState<string[]>([])
    const rows = useMemo(
        () => buildSourcingEditorRows(order, form.state.values.lines),
        [order, form.state.values.lines],
    )
    const counts = {
        all: rows.length,
        pending: rows.filter((row) => row.needsAttention).length,
        excluded: rows.filter((row) => !row.selected && !row.partiallySelected)
            .length,
    }
    const visible = rows.filter(
        (row) =>
            (status === "all" ||
                (status === "pending"
                    ? row.needsAttention
                    : !row.selected && !row.partiallySelected)) &&
            `${row.product.itemName} ${row.product.itemSku ?? ""}`
                .toLocaleLowerCase()
                .includes(query.trim().toLocaleLowerCase()),
    )
    const changeQuery = (value: string) => {
        setQuery(value)
        setPagination((current) => ({ ...current, pageIndex: 0 }))
    }
    return (
        <section
            aria-label="供给商品列表"
            className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden"
        >
            <div className="flex shrink-0 flex-wrap items-center gap-3 border-b border-border/60 px-4 py-3">
                <div className="relative min-w-40 flex-1">
                    <SearchIcon
                        aria-hidden="true"
                        className="pointer-events-none absolute top-2.5 left-3 size-4 text-muted-foreground"
                    />
                    <Input
                        id="sourcing-products-search"
                        aria-label="搜索商品名称或规格编号"
                        placeholder="搜索商品名称或规格编号"
                        className="h-9 pr-9 pl-9"
                        value={query}
                        onChange={(event) => changeQuery(event.target.value)}
                    />
                    {query ? (
                        <Button
                            id="sourcing-products-search-clear"
                            type="button"
                            size="icon-sm"
                            variant="ghost"
                            aria-label="清除商品搜索"
                            className="absolute top-0.5 right-0.5"
                            onClick={() => changeQuery("")}
                        >
                            <XIcon />
                        </Button>
                    ) : null}
                </div>
                <div
                    role="group"
                    aria-label="商品方案状态"
                    className="flex flex-wrap gap-1"
                >
                    {(
                        [
                            ["all", "全部"],
                            ["pending", "待调整"],
                            ["excluded", "暂不分配"],
                        ] as const
                    ).map(([value, label]) => (
                        <Button
                            key={value}
                            id={`sourcing-products-filter-${value}`}
                            type="button"
                            size="sm"
                            variant={status === value ? "secondary" : "ghost"}
                            aria-pressed={status === value}
                            onClick={() => {
                                setStatus(value)
                                setPagination((current) => ({
                                    ...current,
                                    pageIndex: 0,
                                }))
                            }}
                        >
                            {label} {counts[value]}
                        </Button>
                    ))}
                </div>
            </div>
            <SourcingProductList
                rows={visible}
                totalCount={rows.length}
                toolbar={toolbar}
                selectedProductIds={selectedProductIds}
                onToggleProducts={onToggleProducts}
                pagination={pagination}
                onPaginationChange={setPagination}
                activeId={editingId}
                onSelect={(id) => {
                    setReturnFocusId(
                        `sourcing-list-${toAutomationIdSegment(id)}-edit`,
                    )
                    setNavigationIds(
                        visible.map((row) => row.product.salesOrderLineId),
                    )
                    setEditingId(id)
                }}
            />
            <SourcingProductAdjustDialog
                {...props}
                productId={editingId}
                onClose={() => setEditingId(null)}
                onSetParticipation={onSetParticipation}
                navigationIds={navigationIds}
                onNavigate={setEditingId}
                returnFocusId={returnFocusId}
            />
        </section>
    )
}
