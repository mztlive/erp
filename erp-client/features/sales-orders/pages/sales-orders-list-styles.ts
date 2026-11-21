export const salesOrdersListStyles = {
    table: [
        "[&_[data-column-id=actions]]:w-32 [&_[data-column-id=actions]]:min-w-28",
        "max-md:[&_[data-column-id=document]]:min-w-44",
    ].join(" "),
} as const
