"use client"

import { BusinessEmptyState } from "@/components/business"
import { Button } from "@/components/ui/button"

export function SettlementListEmptyState({
    empty,
    canCreate,
    onClearFilters,
    onCreateDraft,
}: {
    empty: string
    canCreate: boolean
    onClearFilters: () => void
    onCreateDraft: () => void
}) {
    return (
        <div className="p-6">
            {empty === "FILTER_NO_RESULT" ? (
                <BusinessEmptyState
                    kind="filter"
                    className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                    title="当前筛选无结果"
                    description="没有记录符合当前筛选条件，可清除筛选后重试。"
                    action={
                        <Button
                            id="supplier-settlements-list-empty-clear"
                            type="button"
                            variant="secondary"
                            className="rounded-lg shadow-none"
                            onClick={onClearFilters}
                        >
                            清除筛选
                        </Button>
                    }
                />
            ) : (
                <BusinessEmptyState
                    kind="no-data"
                    className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                    title="当前范围没有结算单"
                    description="可选择供应商与期间后重查，或新建结算草稿。"
                    action={
                        canCreate ? (
                            <Button
                                id="supplier-settlements-list-empty-create"
                                type="button"
                                onClick={onCreateDraft}
                            >
                                新建结算草稿
                            </Button>
                        ) : null
                    }
                />
            )}
        </div>
    )
}
