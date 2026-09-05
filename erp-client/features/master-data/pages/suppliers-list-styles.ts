export const suppliersListStyles = {
    table: [
        "[&_[data-column-id=name]]:min-w-[14.5rem] [&_[data-column-id=name]]:pl-3.5 min-[1200px]:[&_[data-column-id=name]]:min-w-60",
        "[&_[data-column-id=revisionNo]]:w-20 [&_[data-column-id=revisionNo]]:min-w-16",
        "[&_[data-column-id=lifecycle]]:w-28 [&_[data-column-id=lifecycle]]:min-w-24",
        "[&_[data-column-id=revisionTiming]]:w-28 [&_[data-column-id=revisionTiming]]:min-w-24",
        "[&_[data-column-id=blocker]]:min-w-40",
        "[&_[data-column-id=actions]]:w-36 [&_[data-column-id=actions]]:min-w-32",
    ].join(" "),
} as const
