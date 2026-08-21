export type DataTableAlignment = "start" | "center" | "end"

export type DataTableColumnWidth =
    | "default"
    /** 吸收整行剩余宽度的身份列；一张表最多一列使用，避免所有列被平均拉伸。 */
    | "flex"
    | "reference"
    | "status"
    | "amount"
    | "quantity"
    | "rate"
    | "tracks"

export type DataTableLayout = "inset" | "flush"

const dataTableColumnWidthClasses: Record<DataTableColumnWidth, string> = {
    default: "w-table-column-default min-w-table-column-default-min",
    flex: "w-full min-w-table-column-reference-min",
    reference: "w-table-column-reference min-w-table-column-reference-min",
    status: "w-table-column-status min-w-table-column-status-min",
    amount: "w-table-column-amount min-w-table-column-amount-min",
    quantity: "w-table-column-quantity min-w-table-column-quantity-min",
    rate: "w-table-column-rate min-w-table-column-rate-min",
    tracks: "w-table-column-tracks min-w-table-column-tracks-min",
}

export function alignmentClass(alignment: DataTableAlignment = "start") {
    if (alignment === "end") return undefined
    if (alignment === "center") return "text-center"
    return "text-left"
}

export function columnRuntimeWidth(
    enableColumnResizing: boolean,
    role: "selection" | "preview" | undefined,
    runtimeWidth: number | undefined,
) {
    return enableColumnResizing && !role ? runtimeWidth : undefined
}

export function sortableHeaderClass(alignment: DataTableAlignment = "start") {
    if (alignment === "end") {
        return "flex-row-reverse justify-start text-right"
    }
    if (alignment === "center") return "justify-center text-center"
    return "justify-start text-left"
}

export function pinningClass(
    pinned: false | "left" | "right",
    area: "header" | "cell",
) {
    if (!pinned) return undefined
    return area === "header"
        ? "sticky z-10 bg-table-header"
        : "sticky z-10 bg-card group-hover/row:bg-row-hover group-data-[state=selected]/row:bg-row-selected [[data-placeholder]_&]:group-hover/row:bg-card"
}

export function columnWidthClass(
    width: DataTableColumnWidth = "default",
    role?: "selection" | "preview",
) {
    if (role === "selection") {
        return "w-table-column-selection min-w-table-column-selection max-w-table-column-selection"
    }
    if (role === "preview") {
        return "w-table-column-preview min-w-table-column-preview max-w-table-column-preview [&_svg]:size-4"
    }
    return dataTableColumnWidthClasses[width]
}
