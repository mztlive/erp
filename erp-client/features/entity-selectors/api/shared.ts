/** 各选择器列表接口统一使用的页大小。 */
export const OPTION_PAGE_SIZE = 30

export function activeStatus(status: string) {
    return status.toLowerCase() === "active"
}
