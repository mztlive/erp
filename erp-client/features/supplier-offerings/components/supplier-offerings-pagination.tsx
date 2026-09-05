"use client"

import { Button } from "@/components/ui/button"

export type SupplierOfferingsPaginationProps = {
    page: number
    totalPages: number
    disabled: boolean
    onPageChange: (page: number) => void
}

/** 供给列表分页控件；页码写入 URL，由查询消费。 */
export function SupplierOfferingsPagination({
    page,
    totalPages,
    disabled,
    onPageChange,
}: SupplierOfferingsPaginationProps) {
    return (
        <div className="flex items-center justify-between pt-1 text-sm">
            <span className="text-xs text-muted-foreground">
                第 {page} / {totalPages} 页
            </span>
            <div className="flex items-center gap-2">
                <Button
                    id="supplier-offerings-pagination-prev"
                    type="button"
                    size="sm"
                    variant="outline"
                    className="h-8 px-3 text-xs"
                    disabled={page <= 1 || disabled}
                    onClick={() => onPageChange(Math.max(1, page - 1))}
                >
                    上一页
                </Button>
                <Button
                    id="supplier-offerings-pagination-next"
                    type="button"
                    size="sm"
                    variant="outline"
                    className="h-8 px-3 text-xs"
                    disabled={page >= totalPages || disabled}
                    onClick={() => onPageChange(page + 1)}
                >
                    下一页
                </Button>
            </div>
        </div>
    )
}
