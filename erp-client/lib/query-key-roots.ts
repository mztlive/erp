/**
 * 跨 feature 失效只依赖稳定根键，不得为了刷新缓存导入对方内部 hook。
 * 各 feature 的完整 query key 工厂仍由本域公开入口持有。
 */
export const queryKeyRoots = {
    salesOrders: ["sales-orders"] as const,
    workspaceHome: ["workspace-home"] as const,
    entitySelectors: ["entity-selectors"] as const,
}
