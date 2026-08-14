import { PageScaffold } from "@/components/business"

/** 复核队列加载中的骨架占位。 */
export function ReviewPageSkeleton() {
    return (
        <PageScaffold>
            <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
            <div className="h-24 animate-pulse rounded-lg bg-muted" />
            <div className="grid gap-4 xl:grid-cols-[minmax(0,64fr)_minmax(16rem,36fr)]">
                <div className="h-80 animate-pulse rounded-lg bg-muted" />
                <div className="h-64 animate-pulse rounded-lg bg-muted" />
            </div>
        </PageScaffold>
    )
}
