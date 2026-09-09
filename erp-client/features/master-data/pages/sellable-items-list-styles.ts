export const sellableItemsListStyles = {
    gallery: "min-h-0 flex-1 pt-1",
    table: [
        "[&_[data-column-id=name]]:min-w-[14.5rem] [&_[data-column-id=name]]:pl-3.5 min-[1200px]:[&_[data-column-id=name]]:min-w-60",
        "[&_[data-column-id=price]]:w-[8.25rem] [&_[data-column-id=price]]:min-w-30",
        "[&_[data-column-id=marketPrice]]:w-[8.25rem] [&_[data-column-id=marketPrice]]:min-w-30",
        "[&_[data-column-id=productNo]]:w-28 [&_[data-column-id=productNo]]:min-w-24",
        "[&_[data-column-id=supplyRegions]]:w-28 [&_[data-column-id=supplyRegions]]:min-w-24",
        "[&_[data-column-id=supplierCount]]:w-32 [&_[data-column-id=supplierCount]]:min-w-30",
        String.raw`[&_[data-column-id=\_\_preview]]:w-8 [&_[data-column-id=\_\_preview]]:min-w-8 [&_[data-column-id=\_\_preview]]:max-w-8 [&_[data-column-id=\_\_preview]]:px-2`,
    ].join(" "),
} as const
