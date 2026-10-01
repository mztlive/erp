export const productsListStyles = {
    table: [
        "[&_[data-column-id=skuNames]]:min-w-40",
        "[&_[data-column-id=blocker]]:min-w-40",
        "[&_[data-column-id=actions]]:w-44 [&_[data-column-id=actions]]:min-w-40",
    ].join(" "),
} as const
