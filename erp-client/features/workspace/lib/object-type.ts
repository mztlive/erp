/**
 * 把业务对象种类收成小写稳定码，兼容 PascalCase 与 snake_case。
 *
 * @param businessObjectType 业务对象种类。
 * @returns 例如 `SalesOrder` → `sales_order`。
 */
export function normalizeObjectType(businessObjectType: string): string {
    return businessObjectType
        .trim()
        .replace(/([a-z])([A-Z])/g, "$1_$2")
        .toLowerCase()
}
