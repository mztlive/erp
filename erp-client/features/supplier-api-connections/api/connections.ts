/**
 * W20 · API 供应商连接 · HTTP 适配层（兼容再导出）。
 * 实现按资源拆分：mapping（后端载荷映射）/ list / center / commands。
 */

export {
    bindCredentialReference,
    bindEndpointReference,
    createConnection,
    disableConnection,
    enableConnection,
    runHealthCheck,
    startCatalogSync,
    updateCapabilities,
} from "./commands"
export { fetchConnectionCenter } from "./center"
export {
    fetchConnectionList,
    fetchOpaqueReferenceOptions,
    type ListQueryInput,
} from "./list"
