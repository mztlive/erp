import type { ListColumnDef, ListRow } from "@/features/workspace-kit/types"

/** Client-side CSV export of currently filtered list rows (shipped helper). */
export function exportListRowsToCsv(
  rows: readonly ListRow[],
  columns: readonly ListColumnDef[],
  fileName: string
): void {
  const quote = (value: string) => `"${value.replaceAll('"', '""')}"`
  const header = columns.map((column) => quote(column.header)).join(",")
  const body = rows.map((row) =>
    columns
      .map((column) => {
        if (column.status && row.status) return quote(row.status.label)
        return quote(row.cells[column.key] ?? "")
      })
      .join(",")
  )
  const csv = [header, ...body].join("\n")
  const url = URL.createObjectURL(
    new Blob(["\uFEFF", csv], { type: "text/csv;charset=utf-8" })
  )
  const anchor = document.createElement("a")
  anchor.href = url
  anchor.download = fileName.endsWith(".csv") ? fileName : `${fileName}.csv`
  anchor.click()
  URL.revokeObjectURL(url)
}
