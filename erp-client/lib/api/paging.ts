/**
 * 分页 / 排序参数与分页响应类型，以及查询字符串序列化工具。
 *
 * 与后端契约保持一致（P0-foundation.md 4.1）：
 * - 列表查询统一 page / page_size / sort_by / sort_dir，域内筛选字段扁平透传
 * - 分页响应形状为 items + total + page + page_size
 */

/** 排序方向。 */
export type SortDir = "asc" | "desc"

/** 分页列表查询参数（可叠加任意扁平筛选字段）。 */
export interface PageParams {
  /** 页码，从 1 开始。 */
  page: number
  /** 每页条数。 */
  page_size: number
  /** 排序列名（可选）。 */
  sort_by?: string
  /** 排序方向（可选，默认由后端决定）。 */
  sort_dir?: SortDir
}

/** 后端分页响应统一形状。 */
export interface Page<T> {
  items: T[]
  total: number
  page: number
  page_size: number
}

/**
 * 将扁平查询参数序列化为查询字符串（不含前导 "?"）。
 *
 * - null / undefined / 空字符串 会被跳过（语义为「不过滤」）
 * - 其余值（数字、布尔、字符串、数组等）经 String 转换后透传
 *
 * @param params PageParams 与任意扁平筛选字段。
 * @returns 形如 "page=1&page_size=20&sort_by=code&sort_dir=asc" 的字符串。
 */
export const toQueryString = (params: Record<string, unknown>): string => {
  const search = new URLSearchParams()
  for (const [key, value] of Object.entries(params)) {
    if (value === undefined || value === null || value === "") continue
    search.set(key, String(value))
  }
  return search.toString()
}
