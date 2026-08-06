/**
 * W19 会话模块（P4 F8 后仅 re-export api 中的演示开关 no-op）。
 * 运行时数据路径已迁至 api.ts 真实 HTTP，不再持有 mock seed。
 */

export {
  setW19AuditAccessPolicyConfigured,
  setW19DemoEmptyReason,
  setW19FieldGranularityConfigured,
  setW19UserRoleTimePolicyConfigured,
} from "@/features/access-audit/api"
