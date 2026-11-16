import { BusinessEmptyState } from "@/components/business"
import { Button } from "@/components/ui/button"

export function IntegrationEmptyScope() {
    return (
        <BusinessEmptyState
            kind="no-scope"
            title="暂无处理权限"
            description="当前账号授权范围内没有待处理的外部异常或对账差异。"
            className="rounded-lg border-0 bg-transparent shadow-none ring-0"
        />
    )
}

export function IntegrationEmptyQueue({
    onClearFilters,
}: {
    onClearFilters: () => void
}) {
    return (
        <BusinessEmptyState
            kind="filter"
            title="无匹配处理项"
            description="当前筛选没有结果。可切换视图、清除筛选，或返回工作台。"
            className="rounded-lg border-0 bg-transparent shadow-none ring-0"
            action={
                <Button
                    id="integration-queue-empty-clear-filters"
                    type="button"
                    size="sm"
                    variant="secondary"
                    className="rounded-lg shadow-none"
                    onClick={onClearFilters}
                >
                    清除筛选
                </Button>
            }
        />
    )
}

export function IntegrationEmptySelection() {
    return (
        <BusinessEmptyState
            kind="filter"
            title="未选择处理项"
            description="从左侧队列选择任务或差异。"
            className="rounded-lg border-0 bg-transparent shadow-none ring-0"
        />
    )
}
