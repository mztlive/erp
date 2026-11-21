/** 必填字段和可编辑表格列头共用的红色标记。 */
export function RequiredMark() {
    return (
        <span
            data-slot="required-mark"
            className="ml-0.5 leading-none text-destructive"
            aria-hidden="true"
        >
            *
        </span>
    )
}
