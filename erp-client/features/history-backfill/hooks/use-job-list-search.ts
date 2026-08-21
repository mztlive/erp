"use client"

/**
 * 已随 docs/ui-filter-design.md 收敛为显式提交模型：搜索草稿与结构化筛选
 * 统一由 useJobListFilters 管理（URL 是已生效状态唯一事实源，不再
 * 300ms 防抖即时写 URL）。保留本入口仅为兼容既有调用方；新代码请直接
 * 使用 useJobListFilters。
 */
export { useJobListFilters as useJobListSearch } from "@/features/history-backfill/hooks/use-job-list-filters"
