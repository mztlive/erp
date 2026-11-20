export const productsListStyles = {
    table: [
        "[&_[data-column-id=skuNames]]:min-w-40",
        "[&_[data-column-id=blocker]]:min-w-40",
        "[&_[data-column-id=actions]]:w-32 [&_[data-column-id=actions]]:min-w-28",
    ].join(" "),
} as const
