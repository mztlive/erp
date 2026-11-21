export const booksListStyles = {
    table: [
        "[&_[data-column-id=actions]]:w-24 [&_[data-column-id=actions]]:min-w-20",
        "max-sm:[&_[data-column-id=customer]]:min-w-44",
    ].join(" "),
} as const
