export const suppliersListStyles = {
    table: [
        "[&_[data-column-id=name]]:min-w-[14.5rem] [&_[data-column-id=name]]:pl-3.5 min-[1200px]:[&_[data-column-id=name]]:min-w-60",
        "[&_[data-column-id=lifecycle]]:w-28 [&_[data-column-id=lifecycle]]:min-w-24",
        "[&_[data-column-id=capability]]:min-w-40",
        "[&_[data-column-id=qualification]]:min-w-36",
        "[&_[data-column-id=settlement]]:min-w-36",
        "[&_[data-column-id=entities]]:min-w-40",
        "[&_[data-column-id=invoice]]:min-w-36",
        "[&_[data-column-id=businessCategory]]:min-w-32",
        "[&_[data-column-id=revisionNo]]:w-20 [&_[data-column-id=revisionNo]]:min-w-16",
    ].join(" "),
} as const
