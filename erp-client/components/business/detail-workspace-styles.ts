/**
 * Shared Tailwind classes for Feishu pure-white edge-to-edge detail workspaces.
 * Mirrors `listWorkspaceStyles` so list pages and detail pages share one cohesive visual system.
 */
export const detailWorkspaceStyles = {
    page: "erp-detail-workspace-page max-w-none gap-0 bg-card px-workspace-inline py-0 md:px-page-inline md:py-0 min-[75rem]:px-page-inline-lg",
    chrome: "sticky top-0 z-20 border-b border-border/60 bg-card transition-shadow duration-150",
    header: "flex flex-wrap items-center justify-between gap-x-4 gap-y-2.5 py-3.5",
    title: "min-w-0 max-w-full break-words text-page-title font-semibold tracking-tight text-foreground",
    meta: "flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-xs leading-4.5 text-muted-foreground",
    headerActions:
        "ml-auto flex max-w-full shrink-0 flex-wrap items-center justify-end gap-1.5 [&_[data-slot=button]]:h-control [&_[data-slot=button]]:px-3",
    quietButton:
        "text-body-compact font-normal text-muted-foreground shadow-none hover:bg-muted/80 hover:text-foreground",
    quietDangerButton:
        "text-body-compact font-normal text-muted-foreground shadow-none hover:bg-destructive-soft/75 hover:text-destructive-soft-foreground",
    subtleButton:
        "h-control-sm border-transparent bg-muted/65 px-2.5 text-body-compact font-normal text-foreground/80 shadow-none hover:bg-muted hover:text-foreground aria-expanded:border-primary/25 aria-expanded:bg-sidebar-accent/70 aria-expanded:text-sidebar-accent-foreground",
    tabsList:
        "h-auto w-full flex-nowrap justify-start gap-6 overflow-x-auto rounded-none border-0 bg-transparent p-0",
    tab: [
        "relative -mb-px inline-flex h-object-tab flex-none cursor-pointer items-center gap-1.5 rounded-none border-0 bg-transparent px-0 text-body-compact font-normal text-muted-foreground shadow-none transition-colors duration-150",
        "hover:text-foreground focus-visible:rounded-sm focus-visible:outline-2 focus-visible:-outline-offset-3 focus-visible:outline-primary",
        "data-active:bg-transparent data-active:font-medium data-active:text-primary data-active:shadow-none",
        "after:pointer-events-none after:absolute after:inset-x-0 after:bottom-0 after:h-0.5 after:rounded-full after:bg-primary after:opacity-0 data-active:after:opacity-100",
    ].join(" "),
    body: "min-w-0 flex-1",
    section: "min-w-0 py-6 first:pt-5 last:pb-10",
    sectionHeader:
        "flex flex-wrap items-center justify-between gap-x-4 gap-y-2",
    sectionTitle:
        "text-section-title font-semibold tracking-tight text-foreground",
    sectionSubtitle: "text-xs leading-4.5 text-muted-foreground",
    inlineTableWrapper: "w-full max-w-full border-y border-border/70",
} as const
