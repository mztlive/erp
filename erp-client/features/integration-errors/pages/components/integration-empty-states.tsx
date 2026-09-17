import { BusinessEmptyState } from "@/components/business"
import { Button } from "@/components/ui/button"

export function IntegrationEmptyScope() {
    return (
        <BusinessEmptyState
            kind="no-scope"
            title="当前角色无集成处理范围"
            description="当前权限与数据范围内没有可处理的异常或差异；不代表系统尚无记录。"
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
