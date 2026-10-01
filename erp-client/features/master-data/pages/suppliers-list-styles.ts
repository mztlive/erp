export const suppliersListStyles = {
    table: [
        "[&_[data-column-id=capability]]:min-w-40",
        "[&_[data-column-id=qualification]]:min-w-36",
        "[&_[data-column-id=settlement]]:min-w-36",
        "[&_[data-column-id=entities]]:min-w-40",
        "[&_[data-column-id=invoice]]:min-w-36",
        "[&_[data-column-id=businessCategory]]:min-w-32",
    ].join(" "),
} as const
