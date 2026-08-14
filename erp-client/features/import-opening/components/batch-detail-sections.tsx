/**
 * 批次详情分区组件统一出口。
 * 各分区拆分在 components/*-section.tsx，保留本模块路径避免下游改动导入。
 */

export { OverviewSection } from "./overview-section"
export { FilesSection } from "./files-section"
export { TrialSection } from "./trial-section"
export { ConfirmSection } from "./confirm-section"
export { ImportExecutionActions } from "./execution-actions"
export { ProgressSection } from "./progress-section"
export { ResultSection } from "./result-section"
export { AuditSection } from "./audit-section"
