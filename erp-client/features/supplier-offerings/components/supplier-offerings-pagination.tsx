"use client"

import {
    ChevronLeftIcon,
    ChevronRightIcon,
    ChevronsLeftIcon,
    ChevronsRightIcon,
} from "lucide-react"

import { Button } from "@/components/ui/button"

type SupplierOfferingsPaginationProps = {
    page: number
    pageSize: number
    total: number
    loading: boolean
    onPageChange: (page: number) => void
}

/** 固定页长，保留供给链接的页码口径与原生翻页 ID。 */
export function SupplierOfferingsPagination({
    page,
    pageSize,
    total,
    loading,
    onPageChange,
}: SupplierOfferingsPaginationProps) {
    const totalPages = Math.max(1, Math.ceil(total / pageSize))
    const needsPager = page > 1 || totalPages > 1
    const controls = [
        {
            key: "first",
            label: "第一页",
            icon: ChevronsLeftIcon,
            target: 1,
            disabled: page <= 1,
        },
        {
            key: "prev",
            label: "上一页",
            icon: ChevronLeftIcon,
            target: page - 1,
            disabled: page <= 1,
        },
        {
            key: "next",
            label: "下一页",
            icon: ChevronRightIcon,
            target: page + 1,
            disabled: page >= totalPages,
        },
        {
            key: "last",
            label: "最后一页",
            icon: ChevronsRightIcon,
            target: totalPages,
            disabled: page >= totalPages,
        },
    ]

    return (
        <div
            data-slot="data-table-pagination"
            className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2"
        >
            <span aria-live="polite" className="text-muted-foreground">
                <span className="num">{pageSize}</span> 条 / 页 · 共{" "}
                <span className="num">{total.toLocaleString("zh-CN")}</span> 条
                {!needsPager ? " · 已全部显示" : null}
            </span>
            {needsPager ? (
                <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
                    <span className="num text-muted-foreground">
                        第 {page} / {totalPages} 页
                    </span>
                    <div
                        role="group"
                        aria-label="翻页"
                        className="flex items-center gap-1"
                    >
                        {controls.map(
                            ({ key, label, icon: Icon, target, disabled }) => (
                                <Button
                                    key={key}
                                    id={`supplier-offerings-pagination-${key}`}
                                    type="button"
                                    size="icon-sm"
                                    variant="outline"
                                    disabled={loading || disabled}
                                    onClick={() => onPageChange(target)}
                                    aria-label={label}
                                    title={label}
                                >
                                    <Icon aria-hidden="true" />
                                </Button>
                            ),
                        )}
                    </div>
                </div>
            ) : null}
        </div>
    )
}
