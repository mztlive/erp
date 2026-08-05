export function applyFieldPermissions<T>(
  row: T,
  fieldHide: "none" | "cost" | "profit" | undefined,
  handlers: {
    cost?: (row: T) => T
    profit?: (row: T) => T
  }
): T {
  if (fieldHide === "cost" && handlers.cost) return handlers.cost(row)
  if (fieldHide === "profit" && handlers.profit) return handlers.profit(row)
  return row
}
