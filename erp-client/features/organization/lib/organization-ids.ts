/** 组织筛选控件写回的逗号分隔稳定 ID，转成查询参数需要的数组。 */
export function selectedOrgUnitIds(value: string): string[] {
    return value
        .split(",")
        .map((part) => part.trim())
        .filter(Boolean)
}
