export const salesOrdersListStyles = {
    table: [
        "[&_[data-column-id=document]]:w-[250px] [&_[data-column-id=document]]:min-w-[250px] [&_[data-column-id=document]]:pl-3.5",
        "[&_[data-column-id=nature]]:w-26 [&_[data-column-id=nature]]:min-w-26",
        "[&_[data-column-id=contract]]:w-50 [&_[data-column-id=contract]]:min-w-50",
        "[&_[data-column-id=tracks]]:w-55 [&_[data-column-id=tracks]]:min-w-55",
        "[&_[data-column-id=amount]]:w-30 [&_[data-column-id=amount]]:min-w-30",
        "[&_[data-column-id=owner]]:w-[90px] [&_[data-column-id=owner]]:min-w-[90px]",
        "[&_[data-column-id=currentOwner]]:w-[150px] [&_[data-column-id=currentOwner]]:min-w-[150px]",
        "[&_[data-column-id=submittedAt]]:w-[150px] [&_[data-column-id=submittedAt]]:min-w-[150px]",
    ].join(" "),
} as const
