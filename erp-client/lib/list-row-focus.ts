/**
 * 列表行焦点恢复。
 *
 * 预览 Sheet / 弹窗关闭后把焦点还给触发行，供列表键盘操作继续。
 * 行已卸载或 rowId 为空时静默跳过；业务 ID 一律经 CSS.escape 转义，
 * 避免包含引号或反斜杠时选择器失效。
 */
export function restoreListRowFocus(rowId: string | null | undefined): void {
    if (!rowId) return
    document
        .querySelector<HTMLElement>(`[data-row-id="${CSS.escape(rowId)}"]`)
        ?.focus()
}
