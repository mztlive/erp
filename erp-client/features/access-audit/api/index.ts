/**
 * W19 权限与审计 · 真实 HTTP API（P4 F8）。
 * 后端域：access_control + iam roles/admins。
 * 聚合视图按资源模块组装；无后端资源时返回空列表并登记 gap，不造业务数据。
 */

export { fetchAccessList } from "./list"
export { fetchEffectiveAccess } from "./effective-access"
export { fetchAuditEvent } from "./audit-event"
export { previewAccessChange, submitAccessChange } from "./changes"
