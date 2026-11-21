export const suppliersListStyles = {
    capabilityFilter: [
        "w-40 max-w-full min-w-0",
        "[&>button]:border-transparent [&>button]:bg-muted/65 [&>button:hover]:bg-muted",
        "[&>button:focus-visible]:border-primary/60 [&>button:focus-visible]:bg-card",
        "data-[has-value=true]:[&>button]:border-primary/25 data-[has-value=true]:[&>button]:bg-sidebar-accent/70",
        "data-[has-value=true]:[&>button:hover]:bg-sidebar-accent data-[has-value=true]:[&>button]:text-sidebar-accent-foreground",
        "data-[has-value=true]:[&>button>span:first-child]:text-sidebar-accent-foreground/75 data-[has-value=true]:[&>button>span:nth-child(2)]:font-medium",
    ].join(" "),
    table: [
        "[&_[data-column-id=capability]]:min-w-40",
        "[&_[data-column-id=qualification]]:min-w-36",
        "[&_[data-column-id=settlement]]:min-w-36",
        "[&_[data-column-id=entities]]:min-w-40",
        "[&_[data-column-id=invoice]]:min-w-36",
        "[&_[data-column-id=businessCategory]]:min-w-32",
    ].join(" "),
} as const
