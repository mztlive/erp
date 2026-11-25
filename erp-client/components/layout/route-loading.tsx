import { Skeleton } from "@/components/ui/skeleton"

/** 路由壳的 Suspense 纯白通栏骨架占位，与 DetailWorkspaceSkeleton 视觉一致。 */
export function RouteLoading({ label }: { label: string }) {
    return (
        <div
            role="status"
            aria-busy="true"
            className="flex min-h-full w-full flex-1 flex-col bg-card px-workspace-inline md:px-page-inline @[70rem]/workspace:px-page-inline-lg"
        >
            <div className="flex flex-wrap items-center justify-between gap-4 border-b border-border/60 py-3.5">
                <div className="space-y-1.5">
                    <Skeleton className="h-6 w-44" />
                    <p className="text-xs text-muted-foreground">{label}</p>
                </div>
                <div className="flex items-center gap-2" aria-hidden="true">
                    <Skeleton className="h-control w-20" />
                    <Skeleton className="h-control w-24" />
                </div>
            </div>
            <div
                className="flex flex-wrap items-center gap-2 border-b border-border/60 py-workspace-toolbar-block"
                aria-hidden="true"
            >
                <Skeleton className="h-control w-60" />
                <Skeleton className="h-control w-28" />
                <Skeleton className="h-control w-28" />
            </div>
            <div className="flex flex-1 flex-col gap-3 py-4" aria-hidden="true">
                <Skeleton className="h-table-header w-full bg-muted/75" />
                {Array.from({ length: 6 }, (_, index) => (
                    <Skeleton
                        key={index}
                        className="h-row w-full bg-muted/55"
                    />
                ))}
            </div>
        </div>
    )
}
