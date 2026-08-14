import Link from "next/link"
import { RefreshCwIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import type { IntegrationView } from "../../types"

export function IntegrationDetailNav({
    view,
    queueContextId,
    onRefresh,
}: {
    view: IntegrationView
    queueContextId: string
    onRefresh: () => void
}) {
    return (
        <div className="flex flex-wrap gap-2">
            <Button
                type="button"
                size="sm"
                variant="ghost"
                render={
                    <Link
                        href={`/governance/integration-errors?view=${view}&queueContextId=${encodeURIComponent(queueContextId)}`}
                    />
                }
            >
                返回队列
            </Button>
            <Button
                type="button"
                size="sm"
                variant="ghost"
                className="text-muted-foreground"
                onClick={onRefresh}
            >
                <RefreshCwIcon data-icon="inline-start" aria-hidden />
                刷新当前任务
            </Button>
        </div>
    )
}
