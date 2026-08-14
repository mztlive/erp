/**
 * W29 后端 DTO 映射与筛选（再导出枢纽）。
 * 实现已按资源拆分，保持既有导入路径不变：
 * - ./backend-types     后端 DTO 形状
 * - ./shared-mappers    两资源共用的纯辅助函数
 * - ./error-task-mappers    mapErrorTask
 * - ./difference-mappers    mapDifference
 * - ./query-filter          matchesQuery
 */

export type {
    BackendDifference,
    BackendErrorTask,
    BackendReplayResult,
} from "./backend-types"
export { mapErrorTask } from "./error-task-mappers"
export { mapDifference } from "./difference-mappers"
export { errorClassToBackend } from "./shared-mappers"
export { matchesQuery } from "./query-filter"
