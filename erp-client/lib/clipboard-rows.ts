/** 保留 Excel 空单元格、引号内换行及转义引号，同时记录源文本行号。 */
export function clipboardRows(text: string) {
    const result: { cells: string[]; line: number; raw: string }[] = []
    let cells: string[] = [],
        cell = "",
        quoted = false,
        line = 1,
        start = 1,
        offset = 0
    const source = text.replace(/^\uFEFF/, "")
    for (let i = 0; i < source.length; i++) {
        const char = source[i]
        if (char === '"' && (quoted || cell === "")) {
            if (quoted && source[i + 1] === '"') {
                cell += '"'
                i++
            } else quoted = !quoted
        } else if (
            !quoted &&
            (char === "\t" || char === "\r" || char === "\n")
        ) {
            cells.push(cell)
            cell = ""
            if (char !== "\t") {
                result.push({
                    cells,
                    line: start,
                    raw: source.slice(offset, i),
                })
                cells = []
                if (char === "\r" && source[i + 1] === "\n") i++
                line++
                start = line
                offset = i + 1
            }
        } else {
            cell += char
            if (char === "\n") line++
        }
    }
    if (quoted) throw new Error("文本中的双引号未闭合，请检查后重新解析。")
    cells.push(cell)
    result.push({ cells, line: start, raw: source.slice(offset) })
    return result.filter((row) => row.cells.some((value) => value.trim()))
}
