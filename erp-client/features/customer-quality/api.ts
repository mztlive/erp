/**
 * 兼容垫片：外部功能（features/customers）按原路径导入本模块。
 * 真实实现已移至 api/customer-quality.ts。
 */
export {
    fetchCustomerQuality,
    fetchCustomerQualityPeriodPolicy,
    startCustomerQualityExport,
    type PeriodPolicyInput,
} from "./api/customer-quality"
