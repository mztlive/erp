/** Shared Tailwind classes for business list workspaces (Feishu Bitable pure-white edge-to-edge surface + soft borderless toolbar). */
export const listWorkspaceStyles = {
    page: "erp-list-workspace-page max-w-none gap-0 bg-card px-workspace-inline py-0 md:px-page-inline md:py-0 min-[75rem]:px-page-inline-lg",
    header: "flex shrink-0 flex-wrap items-center justify-between gap-x-4 gap-y-2 border-b border-border/60 bg-card py-2 md:min-h-12 md:py-0",
    eyebrow:
        "inline-flex shrink-0 items-center rounded-sm bg-muted/80 px-1.5 py-0.5 text-tiny leading-4 font-medium text-muted-foreground",
    title: "text-page-title font-semibold tracking-tight text-foreground",
    description: "mt-0.5 text-xs text-muted-foreground/85",
    headerActions:
        "flex max-w-full shrink-0 flex-wrap items-center justify-end gap-1.5 [&_[data-slot=button]]:h-control [&_[data-slot=button]]:px-3",
    quietButton:
        "text-body-compact font-normal text-muted-foreground shadow-none hover:bg-muted/80 hover:text-foreground",
    exportButton: "text-body-compact shadow-none",
    workSurface:
        "flex min-w-0 flex-1 flex-col overflow-hidden rounded-lg border border-border/75 bg-card",
    viewBar:
        "flex shrink-0 items-center justify-between gap-3 border-b border-border/65 bg-card px-4",
    views: "flex min-w-0 gap-6 overflow-x-auto",
    view: "relative -mb-px inline-flex min-h-workspace-view shrink-0 cursor-pointer items-center gap-1.5 text-body-compact font-normal text-muted-foreground transition-colors duration-150 ease-linear hover:text-foreground focus-visible:rounded-sm focus-visible:outline-2 focus-visible:-outline-offset-3 focus-visible:outline-primary",
    activeView:
        "font-medium text-primary after:pointer-events-none after:absolute after:inset-x-0 after:bottom-0 after:h-0.5 after:rounded-full after:bg-primary",
    viewCount: "text-xs font-normal tabular-nums text-muted-foreground/75",
    viewHint:
        "text-xs whitespace-nowrap text-muted-foreground/70 max-[1280px]:hidden",
    toolbar: [
        "flex shrink-0 flex-wrap items-center gap-x-2.5 gap-y-2 border-b border-border/60 bg-card px-4 py-workspace-toolbar-block transition-[border-color,padding] duration-200 ease-out motion-reduce:transition-none",
        "[&_[data-slot=button]]:shadow-none [&_[data-slot=input-group]]:text-body-compact [&_[data-slot=input-group]]:shadow-none",
    ].join(" "),
    filters: "min-w-0 flex-1",
    table: [
        "min-h-0 flex-1",
        "[&_[data-slot=data-table]]:h-full [&_[data-slot=data-table]]:gap-0",
        "[&_[data-slot=data-table-surface]]:flex-1 [&_[data-slot=data-table-surface]]:rounded-none [&_[data-slot=data-table-surface]]:border-0",
        "[&_[data-slot=table-row]:focus-visible]:outline-2 [&_[data-slot=table-row]:focus-visible]:-outline-offset-2 [&_[data-slot=table-row]:focus-visible]:outline-primary",
        "[&_[data-slot=data-table-pagination]]:mt-auto [&_[data-slot=data-table-pagination]]:shrink-0 [&_[data-slot=data-table-pagination]]:border-t [&_[data-slot=data-table-pagination]]:border-border/60 [&_[data-slot=data-table-pagination]]:bg-card [&_[data-slot=data-table-pagination]]:px-4 [&_[data-slot=data-table-pagination]]:py-table-pagination-block [&_[data-slot=data-table-pagination]]:text-xs",
    ].join(" "),
} as const

export const listWorkspaceEmptyStateClassName =
    "rounded-none border-0 bg-transparent px-6 py-16 shadow-none ring-0"
