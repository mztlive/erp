/** 纯展示：导出 CSV（不触网）。 */

export function buildMasterDataExportCsv(
  rows: readonly {
    stableNo: string
    name: string
    revisionNo: number
    lifecycleStatusLabel: string
    revisionTimingLabel: string
    effectiveFrom: string
    effectiveTo?: string
    primaryBlocker?: string
  }[],
  filterSnapshotLabel: string
): string {
  const quote = (value: string) => `"${value.replaceAll('"', '""')}"`
  const header = [
    "资料编号",
    "名称",
    "版本",
    "启用状态",
    "版本状态",
    "生效开始",
    "生效结束",
    "不可用原因",
  ]
    .map(quote)
    .join(",")
  const body = rows
    .map((row) =>
      [
        row.stableNo,
        row.name,
        `v${row.revisionNo}`,
        row.lifecycleStatusLabel,
        row.revisionTimingLabel,
        row.effectiveFrom,
        row.effectiveTo ?? "长期",
        row.primaryBlocker ?? "",
      ]
        .map((v) => quote(v))
        .join(",")
    )
    .join("\n")
  const meta = [
    `# 筛选条件=${filterSnapshotLabel}`,
    `# 说明=导出时按权限重新核对；不含无权查看的敏感信息`,
  ].join("\n")
  return `${meta}\n${header}\n${body}`
}

export function downloadCsv(content: string, fileName: string) {
  const url = URL.createObjectURL(
    new Blob(["\uFEFF", content], { type: "text/csv;charset=utf-8" })
  )
  const anchor = document.createElement("a")
  anchor.href = url
  anchor.download = fileName.endsWith(".csv") ? fileName : `${fileName}.csv`
  anchor.click()
  URL.revokeObjectURL(url)
}
