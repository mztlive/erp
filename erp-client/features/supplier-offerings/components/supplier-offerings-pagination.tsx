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
        <>
            <span className="text-xs text-muted-foreground">
                第 {page} / {totalPages} 页
            </span>
            <div className="flex items-center gap-2">
                <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    disabled={page <= 1 || disabled}
                    onClick={() => onPageChange(Math.max(1, page - 1))}
                >
                    上一页
                </Button>
                <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    disabled={page >= totalPages || disabled}
                    onClick={() => onPageChange(page + 1)}
                >
                    下一页
                </Button>
            </div>
        </>
    )
}
