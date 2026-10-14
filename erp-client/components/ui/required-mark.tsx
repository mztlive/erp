/** 必填字段和可编辑表格列头共用的红色标记。 */
export function RequiredMark() {
    return (
        <span className="ml-1 text-destructive" aria-hidden="true">
            *
        </span>
    )
}
