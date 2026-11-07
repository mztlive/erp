"use client"

import { Button } from "@/components/ui/button"
import { masterDataCopy } from "@/features/master-data/lib/copy"

/** 空列表里的「清除筛选」，样式与各主数据列表现有按钮一致。 */
export function MasterDataClearFiltersButton({
    id,
    onClick,
}: {
    id: string
    onClick: () => void
}) {
    return (
        <Button
            id={id}
            type="button"
            variant="secondary"
            size="sm"
            className="rounded-lg shadow-none"
            onClick={onClick}
        >
            清除筛选
        </Button>
    )
}

/** 空列表里的「新建」。 */
export function MasterDataEmptyCreateButton({
    id,
    onClick,
}: {
    id: string
    onClick: () => void
}) {
    return (
        <Button
            id={id}
            type="button"
            variant="secondary"
            size="sm"
            className="rounded-lg shadow-none"
            onClick={onClick}
        >
            {masterDataCopy.actionCreate}
        </Button>
    )
}
