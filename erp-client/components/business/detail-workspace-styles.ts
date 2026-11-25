/**
 * Shared Tailwind classes for Feishu pure-white edge-to-edge detail workspaces.
 * Mirrors `listWorkspaceStyles` so list pages and detail pages share one cohesive visual system.
 */
export const detailWorkspaceStyles = {
    page: "erp-detail-workspace-page max-w-none gap-0 bg-card px-workspace-inline py-0 md:px-page-inline md:py-0 @[70rem]/workspace:px-page-inline-lg",
    chrome: "sticky top-0 z-20 border-b border-border/60 bg-card transition-shadow duration-150",
    header: "flex flex-wrap items-center justify-between gap-x-4 gap-y-2.5 py-3.5",
    title: "min-w-0 max-w-full break-words text-page-title font-semibold tracking-tight text-foreground",
    meta: "flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-xs text-muted-foreground",
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
    sectionSubtitle: "text-xs text-muted-foreground",
    inlineTableWrapper: "w-full max-w-full border-y border-border/70",
    dataTableWrapper:
        "w-full max-w-full overflow-x-auto border-y border-border/70",
    summaryBar: "border-b border-border/60 bg-card",
    subTabs:
        "inline-flex max-w-full flex-wrap items-center gap-0.5 rounded-lg bg-muted/55 p-0.5",
    subTab: [
        "inline-flex h-control-sm cursor-pointer items-center justify-center gap-1.5 rounded-md px-2.5 text-body-compact font-normal text-muted-foreground transition-colors",
        "hover:text-foreground focus-visible:outline-2 focus-visible:outline-primary",
        "data-[active=true]:bg-card data-[active=true]:font-medium data-[active=true]:text-foreground data-[active=true]:shadow-2xs",
    ].join(" "),
    inlineLinkButton:
        "h-control-sm px-2 text-body-compact font-normal text-primary shadow-none hover:bg-sidebar-accent/70 hover:text-primary",
    formGridSurface: [
        "[&_:is([data-slot=input],[data-slot=textarea],[data-slot=input-group],[data-slot=popover-trigger]:has(>[data-slot=date-picker-value])):not(:disabled):not([data-disabled]):not([readonly]):not([aria-invalid=true]):not(:has([aria-invalid=true])):not(:has(>input:disabled)):not(:has(>textarea:disabled)):not(:has(>input[readonly])):not(:has(>textarea[readonly])):not([data-slot=input-group-control])]:border-border/95",
        "[&_:is([data-slot=input],[data-slot=textarea],[data-slot=input-group],[data-slot=popover-trigger]:has(>[data-slot=date-picker-value])):not(:disabled):not([data-disabled]):not([readonly]):not([aria-invalid=true]):not(:has([aria-invalid=true])):not(:has(>input:disabled)):not(:has(>textarea:disabled)):not(:has(>input[readonly])):not(:has(>textarea[readonly])):not([data-slot=input-group-control])]:bg-muted/26",
        "[&_:is([data-slot=input],[data-slot=textarea],[data-slot=input-group],[data-slot=popover-trigger]:has(>[data-slot=date-picker-value])):not(:disabled):not([data-disabled]):not([readonly]):not([aria-invalid=true]):not(:has([aria-invalid=true])):not(:has(>input:disabled)):not(:has(>textarea:disabled)):not(:has(>input[readonly])):not(:has(>textarea[readonly])):not([data-slot=input-group-control]):hover]:border-primary/42",
        "[&_:is([data-slot=input],[data-slot=textarea],[data-slot=input-group],[data-slot=popover-trigger]:has(>[data-slot=date-picker-value])):not(:disabled):not([data-disabled]):not([readonly]):not([aria-invalid=true]):not(:has([aria-invalid=true])):not(:has(>input:disabled)):not(:has(>textarea:disabled)):not(:has(>input[readonly])):not(:has(>textarea[readonly])):not([data-slot=input-group-control]):hover]:bg-muted/42",
        "[&_:is([data-slot=input]:focus-visible,[data-slot=textarea]:focus-visible,[data-slot=input-group]:focus-within,[data-slot=popover-trigger]:has(>[data-slot=date-picker-value]):focus-visible,[data-slot=popover-trigger]:has(>[data-slot=date-picker-value])[aria-expanded=true]):not([readonly]):not(:has(>input[readonly])):not(:has(>textarea[readonly])):not([aria-invalid=true]):not(:has([aria-invalid=true])):not([data-slot=input-group-control])]:border-primary",
        "[&_:is([data-slot=input]:focus-visible,[data-slot=textarea]:focus-visible,[data-slot=input-group]:focus-within,[data-slot=popover-trigger]:has(>[data-slot=date-picker-value]):focus-visible,[data-slot=popover-trigger]:has(>[data-slot=date-picker-value])[aria-expanded=true]):not([readonly]):not(:has(>input[readonly])):not(:has(>textarea[readonly])):not([aria-invalid=true]):not(:has([aria-invalid=true])):not([data-slot=input-group-control])]:bg-card",
        "[&_:is([data-slot=input],[data-slot=textarea])[readonly]:not([data-slot=input-group-control])]:cursor-default",
        "[&_:is([data-slot=input],[data-slot=textarea])[readonly]:not([data-slot=input-group-control])]:border-transparent",
        "[&_:is([data-slot=input],[data-slot=textarea])[readonly]:not([data-slot=input-group-control])]:bg-muted/45",
        "[&_:is([data-slot=input],[data-slot=textarea])[readonly]:not([data-slot=input-group-control])]:text-foreground/85",
        "[&_:is([data-slot=input],[data-slot=textarea])[readonly]:not([data-slot=input-group-control])]:shadow-none",
        "[&_[data-slot=input-group]:is(:has(>input[readonly]),:has(>textarea[readonly]))]:border-transparent",
        "[&_[data-slot=input-group]:is(:has(>input[readonly]),:has(>textarea[readonly]))]:bg-muted/45",
        "[&_[data-slot=input-group]:is(:has(>input[readonly]),:has(>textarea[readonly]))]:shadow-none",
    ].join(" "),
    inlineTableSurface: [
        "[--table-header:var(--card)]",
        "[&_[data-slot=editable-line-item-table]>.erp-raised-surface]:border-0",
        "[&_[data-slot=editable-line-item-table]>.erp-raised-surface]:bg-transparent",
        "[&_[data-slot=table-header]_tr]:border-b-border/80",
        "[&_[data-slot=table-head]:hover]:bg-transparent",
        "[&_[data-slot=table-head]:hover]:text-foreground/65",
        "[&_[data-slot=table-body]_[data-slot=table-row]]:border-b-grid/75",
        "[&:not([data-inset-cells])_:is([data-slot=table-head],[data-slot=table-cell]):first-child]:ps-0",
        "[&:not([data-inset-cells])_:is([data-slot=table-head],[data-slot=table-cell]):last-child]:pe-0",
        "[&_[data-slot=table-body]_:is([data-slot=input],[data-slot=textarea],[data-slot=input-group],[data-slot=popover-trigger]:has(>[data-slot=date-picker-value])):not(:disabled):not([data-disabled]):not([aria-invalid=true]):not(:has([aria-invalid=true])):not(:has(>input:disabled)):not(:has(>textarea:disabled)):not([data-slot=input-group-control])]:border-transparent",
        "[&_[data-slot=table-body]_:is([data-slot=input],[data-slot=textarea],[data-slot=input-group],[data-slot=popover-trigger]:has(>[data-slot=date-picker-value])):not(:disabled):not([data-disabled]):not([aria-invalid=true]):not(:has([aria-invalid=true])):not(:has(>input:disabled)):not(:has(>textarea:disabled)):not([data-slot=input-group-control])]:bg-muted/52",
        "[&_[data-slot=table-body]_:is([data-slot=input],[data-slot=textarea],[data-slot=input-group],[data-slot=popover-trigger]:has(>[data-slot=date-picker-value])):not(:disabled):not([data-disabled]):not([aria-invalid=true]):not(:has([aria-invalid=true])):not(:has(>input:disabled)):not(:has(>textarea:disabled)):not([data-slot=input-group-control]):hover]:bg-muted/85",
        "[&_[data-slot=table-body]_:is([data-slot=input]:focus-visible,[data-slot=textarea]:focus-visible,[data-slot=input-group]:focus-within,[data-slot=popover-trigger]:has(>[data-slot=date-picker-value]):focus-visible,[data-slot=popover-trigger]:has(>[data-slot=date-picker-value])[aria-expanded=true]):not([aria-invalid=true]):not(:has([aria-invalid=true])):not([data-slot=input-group-control])]:border-primary/65",
        "[&_[data-slot=table-body]_:is([data-slot=input]:focus-visible,[data-slot=textarea]:focus-visible,[data-slot=input-group]:focus-within,[data-slot=popover-trigger]:has(>[data-slot=date-picker-value]):focus-visible,[data-slot=popover-trigger]:has(>[data-slot=date-picker-value])[aria-expanded=true]):not([aria-invalid=true]):not(:has([aria-invalid=true])):not([data-slot=input-group-control])]:bg-card",
        "[&_[data-slot=table-body]_:is([data-slot=input]:focus-visible,[data-slot=textarea]:focus-visible,[data-slot=input-group]:focus-within,[data-slot=popover-trigger]:has(>[data-slot=date-picker-value]):focus-visible,[data-slot=popover-trigger]:has(>[data-slot=date-picker-value])[aria-expanded=true]):not([aria-invalid=true]):not(:has([aria-invalid=true])):not([data-slot=input-group-control])]:ring-3",
        "[&_[data-slot=table-body]_:is([data-slot=input]:focus-visible,[data-slot=textarea]:focus-visible,[data-slot=input-group]:focus-within,[data-slot=popover-trigger]:has(>[data-slot=date-picker-value]):focus-visible,[data-slot=popover-trigger]:has(>[data-slot=date-picker-value])[aria-expanded=true]):not([aria-invalid=true]):not(:has([aria-invalid=true])):not([data-slot=input-group-control])]:ring-ring",
        "[&_[data-slot=table-body]_:is([data-slot=input],[data-slot=textarea]):not([data-slot=input-group-control]):disabled]:border-transparent",
        "[&_[data-slot=table-body]_:is([data-slot=input],[data-slot=textarea]):not([data-slot=input-group-control]):disabled]:bg-transparent",
    ].join(" "),
} as const
