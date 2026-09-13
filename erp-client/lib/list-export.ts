/** 在服务端分页上收集完整筛选结果；中途失败、重复行或总数变化不生成部分文件。 */
export const collectExportPages = async <T>(
    load: (
        page: number,
        pageSize: number,
    ) => Promise<{ items: readonly T[]; total: number }>,
    identity: (row: T) => string,
): Promise<T[]> => {
    const rows: T[] = []
    const seen = new Set<string>()
    let expected: number | undefined
    for (let page = 1; ; page += 1) {
        const result = await load(page, 100)
        expected ??= result.total
        if (result.total !== expected)
            throw { kind: "Validation", message: "查询结果已变化，请重新导出" }
        for (const row of result.items) {
            const id = identity(row)
            if (seen.has(id))
                throw {
                    kind: "Validation",
                    message: "查询结果已变化，请重新导出",
                }
            seen.add(id)
            rows.push(row)
        }
        if (rows.length === expected) return rows
        if (rows.length > expected || result.items.length === 0)
            throw { kind: "Validation", message: "查询结果不完整，请重新导出" }
    }
}

/** 生成带 BOM 的 CSV；所有文本按字段转义，电子表格公式以文本处理。 */
export const buildListCsv = (rows: readonly (readonly string[])[]): string =>
    "\uFEFF" +
    rows
        .map((row) =>
            row
                .map((value) => {
                    const safe = /^[=+@\-\t\r]/.test(value)
                        ? "'" + value
                        : value
                    return `"${safe.replaceAll('"', '""')}"`
                })
                .join(","),
        )
        .join("\r\n")

/** 文件内容完成后立即下载；不创建可在撤权后复用的后台下载入口。 */
export const downloadListCsv = (content: string, filename: string): void => {
    const url = URL.createObjectURL(
        new Blob([content], { type: "text/csv;charset=utf-8" }),
    )
    const anchor = document.createElement("a")
    anchor.href = url
    anchor.download = filename
    anchor.click()
    URL.revokeObjectURL(url)
}
