export const booksListStyles = {
    table: [
        "[&_[data-column-id=customer]]:w-[280px] [&_[data-column-id=customer]]:min-w-[220px] [&_[data-column-id=customer]]:pl-3.5",
        "[&_[data-column-id=form]]:w-26 [&_[data-column-id=form]]:min-w-26",
        "[&_[data-column-id=submitMode]]:w-28 [&_[data-column-id=submitMode]]:min-w-28",
        "[&_[data-column-id=status]]:w-28 [&_[data-column-id=status]]:min-w-28",
        "[&_[data-column-id=display]]:w-24 [&_[data-column-id=display]]:min-w-24",
        "[&_[data-column-id=proposal]]:w-40 [&_[data-column-id=proposal]]:min-w-36",
        "[&_[data-column-id=createdAt]]:w-[150px] [&_[data-column-id=createdAt]]:min-w-[150px]",
    ].join(" "),
} as const
