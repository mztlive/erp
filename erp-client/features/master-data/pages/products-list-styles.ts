export const productsListStyles = {
    table: [
        "[&_[data-column-id=stableNo]]:w-[8.25rem] [&_[data-column-id=stableNo]]:min-w-30 [&_[data-column-id=stableNo]]:pl-3.5",
        "[&_[data-column-id=name]]:w-full [&_[data-column-id=name]]:min-w-60 [&_[data-column-id=name]]:pl-3.5",
        "[&_[data-column-id=revisionNo]]:w-20 [&_[data-column-id=revisionNo]]:min-w-16",
        "[&_[data-column-id=lifecycle]]:w-28 [&_[data-column-id=lifecycle]]:min-w-24",
        "[&_[data-column-id=skuNames]]:min-w-40",
        "[&_[data-column-id=skuPriceRange]]:w-[8.25rem] [&_[data-column-id=skuPriceRange]]:min-w-30",
        "[&_[data-column-id=skuCount]]:w-24 [&_[data-column-id=skuCount]]:min-w-20",
        "[&_[data-column-id=supply]]:w-32 [&_[data-column-id=supply]]:min-w-28",
        "[&_[data-column-id=listing]]:w-28 [&_[data-column-id=listing]]:min-w-24",
        "[&_[data-column-id=blocker]]:min-w-40",
        "[&_[data-column-id=actions]]:w-20 [&_[data-column-id=actions]]:min-w-20",
    ].join(" "),
} as const
