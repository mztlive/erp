/**
 * W04 合同中心 — 真实 HTTP 适配层。
 * 保持 hooks/queries.ts 消费的导出签名与返回类型稳定；实现已按资源拆分到
 * 同目录 list/center/upload/export（后端 Page/DTO 映射见 wire-types / helpers）。
 *
 * 后端路由：
 * - GET/POST /admin/contracts
 * - GET /admin/contracts/{id}
 * - POST /admin/contracts/{id}/revisions
 * - POST /admin/contracts/{id}/terminate
 * - POST /admin/file-assets/upload（multipart）
 */

export { fetchContracts } from "./list"
export { fetchContractCenter } from "./center"
export { uploadContractPdf } from "./upload"
export { createContractExportJob } from "./export"
