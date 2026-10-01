export const dictionaryListStyles = {
    table: [
        "[&_[data-column-id=period]]:w-40 [&_[data-column-id=period]]:min-w-36",
        "[&_[data-column-id=blocker]]:min-w-40",
        "[&_[data-column-id=actions]]:w-36 [&_[data-column-id=actions]]:min-w-32",
    ].join(" "),
} as const
