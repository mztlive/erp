/**
 * 客户资料 HTTP 适配层。
 *
 * 客户创建和修订只允许通过 customer-profiles 根命令提交，避免 Party、客户角色、
 * 归属和从属事实出现部分成功。Wire DTO 只存在于 api/ 内部，页面消费 camelCase 视图。
 *
 * 本文件是公共入口：按资源拆分到 api/directory、api/assignment、api/center、
 * api/mutations，这里只做再导出，既有导入路径保持不变。
 */

export { fetchCustomerDirectory } from "./directory"
export { applyCustomerAssignment } from "./assignment"
export { fetchCustomerCenter } from "./center"
export {
    createCustomer,
    saveCustomerDetails,
    queryCustomerMutationByIdempotency,
    revealCustomerSensitiveField,
} from "./mutations"
